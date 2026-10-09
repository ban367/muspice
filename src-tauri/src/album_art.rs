//! アルバムアートの配信（`albumart`カスタムプロトコル）
//!
//! WebViewは`albumart://localhost/<トラックID>`（Windowsでは
//! `http://albumart.localhost/<トラックID>`）を`<img>`の`src`に指定して画像を読み込む。
//! base64に変換してIPCで渡す方式と比べ、画像をバイト列のまま返せ、フロントエンドに
//! 画像データを溜め込む必要もない。
//!
//! 表示する画像は、ファイルに埋め込まれた画像を優先し、なければ同じフォルダの画像
//! （`cover.jpg`・`folder.jpg`など）を使う（ADR-035）。
//!
//! 配信できるのはDBに登録済みのトラックのアートだけで、任意のファイルは読めない
//! （フォルダの画像も、決まった名前のファイルだけを読む）。
//! 抽出したアートは容量上限付きのLRUキャッシュに保持し、WebViewにはキャッシュさせない
//! （インポートでファイルが差し替わった場合に古い画像を表示しないため）。

use crate::metadata::{
    EmbeddedPicture, MAX_ALBUM_ART_BYTES, extract_album_art, image_dimensions,
    sniff_image_mime_type,
};
use crate::models::{AlbumArtInfo, AlbumArtSource};
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, UriSchemeResponder};

/// カスタムプロトコルのスキーム名
pub const SCHEME: &str = "albumart";

/// キャッシュに保持する画像データの合計サイズの上限
const CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// キャッシュに保持するエントリ数の上限（アートがないトラックの記録を含む）
const CACHE_MAX_ENTRIES: usize = 2000;

/// フォルダの画像として探す名前（拡張子を除く。優先順。大文字と小文字は区別しない）
const FOLDER_IMAGE_NAMES: &[&str] = &["cover", "folder", "front", "album", "albumart"];

/// フォルダの画像として扱う拡張子（優先順。大文字と小文字は区別しない）
const FOLDER_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];

/// ファイル・フォルダの更新日時（読めない場合はNone）
fn modified_at(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// ファイル名がフォルダの画像の名前なら、その優先順位（小さいほど優先）を返す
fn folder_image_rank(path: &Path) -> Option<(usize, usize)> {
    let stem = path.file_stem()?.to_str()?.to_lowercase();
    let extension = path.extension()?.to_str()?.to_lowercase();
    Some((
        FOLDER_IMAGE_NAMES.iter().position(|name| *name == stem)?,
        FOLDER_IMAGE_EXTENSIONS
            .iter()
            .position(|ext| *ext == extension)?,
    ))
}

/// フォルダの画像を読む（大きすぎる・JPEGでもPNGでもないファイルは使わない）
///
/// @returns 画像と、読む前のファイルの更新日時
fn read_folder_image(path: &Path) -> Option<(EmbeddedPicture, Option<SystemTime>)> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_ALBUM_ART_BYTES {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    let mime_type = sniff_image_mime_type(&data)?.to_string();
    Some((
        EmbeddedPicture { data, mime_type },
        metadata.modified().ok(),
    ))
}

/// フォルダの画像を探して読む（名前の優先順に試し、最初に読めたものを使う）
fn find_folder_image(dir: &Path) -> Option<(PathBuf, EmbeddedPicture, Option<SystemTime>)> {
    let mut candidates: Vec<((usize, usize), PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            Some((folder_image_rank(&path)?, path))
        })
        .collect();
    // 同じ優先順位のファイル（大文字と小文字だけが違う名前）は、名前の順にして結果を一定にする
    candidates.sort();

    candidates.into_iter().find_map(|(_, path)| {
        let (picture, modified) = read_folder_image(&path)?;
        Some((path, picture, modified))
    })
}

/// 埋め込みの画像がなかった曲の、フォルダの状態
///
/// フォルダの画像は、アプリの外で置かれる・差し替えられる・消される。キャッシュを参照する時に
/// 今の状態と比べ、変わっていれば読み直す（再スキャン・再起動を待たずに表示へ反映するため）。
#[derive(Debug, Clone, PartialEq)]
struct FolderStamp {
    dir: PathBuf,
    /// フォルダの更新日時（ファイルの追加・削除・名前の変更で変わる）
    dir_modified: Option<SystemTime>,
    /// 表示している画像と、その更新日時（中身の書き換えで変わる）
    image: Option<(PathBuf, Option<SystemTime>)>,
}

impl FolderStamp {
    /// 記録した時から、フォルダと画像が変わっていないか
    fn is_current(&self) -> bool {
        self.dir_modified == modified_at(&self.dir)
            && self
                .image
                .as_ref()
                .is_none_or(|(path, modified)| *modified == modified_at(path))
    }
}

/// ファイルから読んだアルバムアート
struct LoadedArt {
    picture: Option<EmbeddedPicture>,
    /// フォルダの画像を使った場合、その場所
    folder_image: Option<PathBuf>,
    /// 埋め込みの画像がなかった場合の、フォルダの状態
    stamp: Option<FolderStamp>,
}

/// トラックのアルバムアートを読む（埋め込みの画像を優先し、なければフォルダの画像）
fn load_art(file_path: &Path) -> LoadedArt {
    match extract_album_art(file_path) {
        Ok(Some(picture)) => {
            return LoadedArt {
                picture: Some(picture),
                folder_image: None,
                stamp: None,
            };
        }
        Ok(None) => {}
        // 読めないファイル（見つからない曲など）は「埋め込みの画像なし」として扱う
        Err(e) => log::warn!(
            "アルバムアートの抽出に失敗しました: {}: {}",
            file_path.display(),
            e
        ),
    }

    let Some(dir) = file_path.parent() else {
        return LoadedArt {
            picture: None,
            folder_image: None,
            stamp: None,
        };
    };
    // フォルダの更新日時は、画像を探す前に読む（探している間の変更を、次の参照で検出するため）
    let dir_modified = modified_at(dir);
    let (folder_image, picture, image_modified) = match find_folder_image(dir) {
        Some((path, picture, modified)) => (Some(path), Some(picture), modified),
        None => (None, None, None),
    };
    LoadedArt {
        picture,
        stamp: Some(FolderStamp {
            dir: dir.to_path_buf(),
            dir_modified,
            image: folder_image.clone().map(|path| (path, image_modified)),
        }),
        folder_image,
    }
}

/// トラックのアルバムアートの情報（どこの画像か・種類・大きさ）を、ファイルから読む
///
/// アートがなければNone。キャッシュは使わない（アルバムアートの画面を開いた時だけ呼ぶ）。
pub fn info_for_file(file_path: &Path) -> Option<AlbumArtInfo> {
    let art = load_art(file_path);
    let picture = art.picture?;
    let dimensions = image_dimensions(&picture.data);
    Some(AlbumArtInfo {
        source: match art.folder_image {
            Some(_) => AlbumArtSource::Folder,
            None => AlbumArtSource::Embedded,
        },
        file_name: art
            .folder_image
            .and_then(|path| Some(path.file_name()?.to_string_lossy().into_owned())),
        size: u32::try_from(picture.data.len()).unwrap_or(u32::MAX),
        mime_type: picture.mime_type,
        width: dimensions.map(|(width, _)| width),
        height: dimensions.map(|(_, height)| height),
    })
}

/// キャッシュの1件（アートがないトラックも記録する）
#[derive(Debug, Clone)]
pub struct CachedArt {
    /// 画像（アートがなければNone）
    pub picture: Option<EmbeddedPicture>,
    /// 埋め込みの画像がなかった場合の、フォルダの状態（埋め込みの画像ならNone）
    folder: Option<FolderStamp>,
}

impl CachedArt {
    /// 今も使えるか（フォルダの画像・アートなしは、フォルダが変わっていれば使えない）
    ///
    /// ファイルシステムを見るため、ブロッキング処理用のスレッドで呼ぶ。
    fn is_current(&self) -> bool {
        self.folder.as_ref().is_none_or(FolderStamp::is_current)
    }
}

impl From<LoadedArt> for CachedArt {
    fn from(art: LoadedArt) -> Self {
        Self {
            picture: art.picture,
            folder: art.stamp,
        }
    }
}

/// 抽出済みアルバムアートのLRUキャッシュ
///
/// アートがないトラックも記録し、同じファイルを何度も読まないようにする。
pub struct AlbumArtCache {
    entries: HashMap<String, CachedArt>,
    /// 参照順（先頭が最も古い）
    order: VecDeque<String>,
    total_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
}

impl Default for AlbumArtCache {
    fn default() -> Self {
        Self::new(CACHE_MAX_BYTES, CACHE_MAX_ENTRIES)
    }
}

impl AlbumArtCache {
    pub fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            total_bytes: 0,
            max_bytes,
            max_entries,
        }
    }

    /// キャッシュを参照する（ヒットしたエントリは最新として扱う。未キャッシュならNone）
    pub fn get(&mut self, track_id: &str) -> Option<CachedArt> {
        let value = self.entries.get(track_id)?.clone();
        self.touch(track_id);
        Some(value)
    }

    /// エントリを追加し、上限を超えた分を古いものから捨てる
    ///
    /// 単体で上限を超える画像はキャッシュしない。
    pub fn insert(&mut self, track_id: String, art: CachedArt) {
        let size = Self::size_of(&art);
        if size > self.max_bytes {
            return;
        }

        self.remove(&track_id);
        self.total_bytes += size;
        self.order.push_back(track_id.clone());
        self.entries.insert(track_id, art);

        while self.total_bytes > self.max_bytes || self.entries.len() > self.max_entries {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                self.total_bytes -= Self::size_of(&evicted);
            }
        }
    }

    /// トラックのエントリを捨てる（アプリがアルバムアートを書き換えた場合）
    pub fn invalidate(&mut self, track_ids: &[String]) {
        for track_id in track_ids {
            self.remove(track_id);
        }
    }

    /// すべてのエントリを捨てる（インポートでファイルが差し替わった場合など）
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_bytes = 0;
    }

    fn size_of(art: &CachedArt) -> usize {
        art.picture.as_ref().map_or(0, |p| p.data.len())
    }

    fn remove(&mut self, track_id: &str) {
        if let Some(previous) = self.entries.remove(track_id) {
            self.total_bytes -= Self::size_of(&previous);
            self.order.retain(|id| id != track_id);
        }
    }

    fn touch(&mut self, track_id: &str) {
        if let Some(position) = self.order.iter().position(|id| id == track_id)
            && let Some(id) = self.order.remove(position)
        {
            self.order.push_back(id);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// リクエストのパス（`/<トラックID>`）からトラックIDを取り出す
fn track_id_from_path(path: &str) -> Option<&str> {
    let track_id = path.strip_prefix('/').unwrap_or(path);
    validate_track_id(track_id).ok().map(|_| track_id)
}

fn empty_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .body(Vec::new())
        .expect("固定のヘッダーでレスポンスを組み立てるため失敗しない")
}

fn picture_response(picture: Option<EmbeddedPicture>) -> Response<Vec<u8>> {
    match picture {
        Some(picture) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, picture.mime_type)
            // 表示の一貫性はRust側のキャッシュで担保し、WebViewには保持させない
            .header(header::CACHE_CONTROL, "no-store")
            .body(picture.data)
            .unwrap_or_else(|_| empty_response(StatusCode::INTERNAL_SERVER_ERROR)),
        None => empty_response(StatusCode::NOT_FOUND),
    }
}

/// トラックのアルバムアートを返す（同期処理）
///
/// `cached`（参照した時点のキャッシュ）が今も使えればそれを返し、そうでなければファイルから
/// 読んでキャッシュに登録する。
fn load_picture(app: &AppHandle, track_id: &str, cached: Option<CachedArt>) -> Response<Vec<u8>> {
    if let Some(art) = cached
        && art.is_current()
    {
        return picture_response(art.picture);
    }

    let state = app.state::<AppState>();

    let file_path =
        match state.with_db(|db| crate::repository::try_find_file_path_by_track_id(db, track_id)) {
            Ok(Some(path)) => path,
            Ok(None) => return empty_response(StatusCode::NOT_FOUND),
            Err(e) => {
                log::error!("アルバムアートのパス取得に失敗しました: {}", e);
                return empty_response(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    picture_response(load_and_cache(&state, track_id, &file_path))
}

/// アルバムアートをファイルから読んでキャッシュに登録する（同期処理）
fn load_and_cache(state: &AppState, track_id: &str, file_path: &str) -> Option<EmbeddedPicture> {
    let art = CachedArt::from(load_art(Path::new(file_path)));

    if let Ok(mut cache) = state.album_art_cache.lock() {
        cache.insert(track_id.to_string(), art.clone());
    }
    art.picture
}

/// トラックのアルバムアートを取得する（キャッシュになければファイルから読む。同期処理）
///
/// OSのNow Playing（`media_controls`）へ渡す画像に使う。再生中の曲はプレーヤーバーにも
/// 表示しているため、たいていはキャッシュにある。`file_path`は、呼び出し側がDBから取得したもの。
pub fn picture_for_track(
    state: &AppState,
    track_id: &str,
    file_path: &str,
) -> Option<EmbeddedPicture> {
    let cached = state
        .album_art_cache
        .lock()
        .ok()
        .and_then(|mut cache| cache.get(track_id));
    match cached {
        Some(art) if art.is_current() => art.picture,
        _ => load_and_cache(state, track_id, file_path),
    }
}

/// `albumart`プロトコルのリクエストを処理する
///
/// キャッシュにある埋め込みの画像はそのまま返し、それ以外はブロッキング処理用スレッドで
/// 読む（フォルダの画像・アートなしのキャッシュは、フォルダが変わっていないかを確かめる）。
/// 一覧表示では表示中のカードの数だけ要求が同時に届くため、読み込みは同時実行数を制限する。
pub fn handle_request(app: AppHandle, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let Some(track_id) = track_id_from_path(request.uri().path()).map(str::to_string) else {
        responder.respond(empty_response(StatusCode::BAD_REQUEST));
        return;
    };

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();

        let cached = state
            .album_art_cache
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(&track_id));
        let cached = match cached {
            Some(art) if art.folder.is_none() => {
                responder.respond(picture_response(art.picture));
                return;
            }
            other => other,
        };

        let Ok(_permit) = state.album_art_limiter.acquire().await else {
            responder.respond(empty_response(StatusCode::SERVICE_UNAVAILABLE));
            return;
        };
        let app_for_load = app.clone();
        let response = tauri::async_runtime::spawn_blocking(move || {
            load_picture(&app_for_load, &track_id, cached)
        })
        .await
        .unwrap_or_else(|_| empty_response(StatusCode::INTERNAL_SERVER_ERROR));
        responder.respond(response);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_images::{jpeg, png};
    use crate::metadata::{remove_file_album_art, set_file_album_art};
    use std::fs;

    fn picture(size: usize) -> CachedArt {
        CachedArt {
            picture: Some(EmbeddedPicture {
                data: vec![0; size],
                mime_type: "image/jpeg".to_string(),
            }),
            folder: None,
        }
    }

    fn no_art() -> CachedArt {
        CachedArt {
            picture: None,
            folder: None,
        }
    }

    fn cached_size(cache: &mut AlbumArtCache, track_id: &str) -> Option<usize> {
        cache.get(track_id)?.picture.map(|p| p.data.len())
    }

    #[test]
    fn test_cache_returns_inserted_entries_including_missing_art() {
        let mut cache = AlbumArtCache::new(100, 10);
        cache.insert("a".into(), picture(10));
        cache.insert("b".into(), no_art());

        assert_eq!(cached_size(&mut cache, "a"), Some(10));
        assert!(cache.get("b").is_some_and(|art| art.picture.is_none()));
        assert!(cache.get("c").is_none());
    }

    #[test]
    fn test_cache_evicts_least_recently_used_when_over_byte_limit() {
        let mut cache = AlbumArtCache::new(25, 10);
        cache.insert("a".into(), picture(10));
        cache.insert("b".into(), picture(10));
        // aを参照して最新にしておくと、次の追加ではbが捨てられる
        cache.get("a");
        cache.insert("c".into(), picture(10));

        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn test_cache_evicts_when_over_entry_limit() {
        let mut cache = AlbumArtCache::new(1000, 2);
        cache.insert("a".into(), no_art());
        cache.insert("b".into(), no_art());
        cache.insert("c".into(), no_art());

        assert_eq!(cache.len(), 2);
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn test_cache_replaces_entry_and_tracks_size() {
        let mut cache = AlbumArtCache::new(25, 10);
        cache.insert("a".into(), picture(20));
        cache.insert("a".into(), picture(5));
        cache.insert("b".into(), picture(20));

        // 置き換え前の20バイトが残っていれば、bの追加でaが捨てられる
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_some());
    }

    #[test]
    fn test_cache_skips_pictures_larger_than_limit() {
        let mut cache = AlbumArtCache::new(10, 10);
        cache.insert("big".into(), picture(11));
        assert!(cache.get("big").is_none());
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = AlbumArtCache::new(100, 10);
        cache.insert("a".into(), picture(10));
        cache.clear();
        assert!(cache.get("a").is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_invalidate_removes_only_the_given_tracks() {
        let mut cache = AlbumArtCache::new(25, 10);
        cache.insert("a".into(), picture(10));
        cache.insert("b".into(), picture(10));
        cache.invalidate(&["a".to_string(), "missing".to_string()]);

        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        // 捨てた分の容量が空く（空いていなければ、cの追加でbが捨てられる）
        cache.insert("c".into(), picture(10));
        assert!(cache.get("b").is_some());
    }

    #[test]
    fn test_track_id_from_path() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(track_id_from_path(&format!("/{}", id)), Some(id));
        assert_eq!(track_id_from_path("/"), None);
        assert_eq!(track_id_from_path("/../etc/passwd"), None);
        assert_eq!(track_id_from_path(&format!("/{}/extra", id)), None);
    }

    /// テスト用の一時フォルダ（終わったら消す）
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("muspice-art-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        /// 音楽ファイル（タグのないWAV）を置く
        fn track(&self, name: &str) -> PathBuf {
            let path = self.path(name);
            crate::metadata::test_images::write_wav(&path);
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// 更新日時を先へ進める（同じ時刻のうちに書き換えても、変更として見えるようにする）
    #[cfg(unix)]
    fn bump_modified(path: &Path) {
        let later = SystemTime::now() + std::time::Duration::from_secs(60);
        fs::File::open(path).unwrap().set_modified(later).unwrap();
    }

    #[test]
    fn test_folder_image_rank_matches_known_names_ignoring_case() {
        let rank = |name: &str| folder_image_rank(Path::new(name));

        assert_eq!(rank("cover.jpg"), Some((0, 0)));
        assert_eq!(rank("Cover.JPG"), Some((0, 0)));
        assert_eq!(rank("FOLDER.jpeg"), Some((1, 1)));
        assert_eq!(rank("front.png"), Some((2, 2)));
        assert_eq!(rank("album.jpg"), Some((3, 0)));
        assert_eq!(rank("AlbumArt.jpg"), Some((4, 0)));
        // ほかの名前・ほかの種類の画像・拡張子のないファイルは対象にしない
        assert_eq!(rank("back.jpg"), None);
        assert_eq!(rank("cover (1).jpg"), None);
        assert_eq!(rank("._cover.jpg"), None);
        assert_eq!(rank("cover.gif"), None);
        assert_eq!(rank("cover"), None);
        assert_eq!(rank("AlbumArtSmall.jpg"), None);
    }

    #[test]
    fn test_find_folder_image_prefers_names_in_order() {
        let dir = TempDir::new();
        fs::write(dir.path("folder.jpg"), jpeg(20, 20)).unwrap();
        fs::write(dir.path("front.png"), png(30, 30)).unwrap();
        fs::write(dir.path("notes.txt"), b"memo").unwrap();

        let (path, picture, _) = find_folder_image(&dir.0).unwrap();
        assert_eq!(path, dir.path("folder.jpg"));
        assert_eq!(picture.mime_type, "image/jpeg");
        assert_eq!(picture.data, jpeg(20, 20));

        // 名前の優先順位が、拡張子の優先順位より先
        fs::write(dir.path("cover.png"), png(10, 10)).unwrap();
        let (path, picture, _) = find_folder_image(&dir.0).unwrap();
        assert_eq!(path, dir.path("cover.png"));
        assert_eq!(picture.mime_type, "image/png");
    }

    #[test]
    fn test_find_folder_image_skips_files_that_cannot_be_used() {
        let dir = TempDir::new();
        // 画像ではないファイル・大きすぎるファイル・同じ名前のフォルダは使わず、次の候補を使う
        fs::write(dir.path("cover.jpg"), b"not an image").unwrap();
        fs::File::create(dir.path("folder.jpg"))
            .unwrap()
            .set_len(MAX_ALBUM_ART_BYTES + 1)
            .unwrap();
        fs::create_dir(dir.path("front.jpg")).unwrap();
        assert!(find_folder_image(&dir.0).is_none());

        // 拡張子と中身が違っていても、中身の種類で返す
        fs::write(dir.path("album.jpg"), png(8, 8)).unwrap();
        let (path, picture, _) = find_folder_image(&dir.0).unwrap();
        assert_eq!(path, dir.path("album.jpg"));
        assert_eq!(picture.mime_type, "image/png");

        assert!(find_folder_image(&dir.path("no-such-folder")).is_none());
    }

    #[test]
    fn test_load_art_prefers_embedded_picture_over_folder_image() {
        let dir = TempDir::new();
        let track = dir.track("01.wav");
        fs::write(dir.path("cover.jpg"), jpeg(20, 20)).unwrap();

        // 埋め込みの画像がなければ、フォルダの画像
        let art = load_art(&track);
        assert_eq!(art.picture.unwrap().data, jpeg(20, 20));
        assert_eq!(art.folder_image, Some(dir.path("cover.jpg")));
        assert!(art.stamp.is_some());

        // 埋め込みの画像があれば、そちらを使う（フォルダの状態は見ない）
        let embedded = EmbeddedPicture {
            data: png(40, 30),
            mime_type: "image/png".to_string(),
        };
        set_file_album_art(&track, &embedded).unwrap();
        let art = load_art(&track);
        assert_eq!(art.picture.unwrap().data, png(40, 30));
        assert_eq!(art.folder_image, None);
        assert!(art.stamp.is_none());

        // 埋め込みの画像を取り除くと、フォルダの画像に戻る
        assert!(remove_file_album_art(&track).unwrap());
        assert_eq!(load_art(&track).folder_image, Some(dir.path("cover.jpg")));
    }

    #[test]
    fn test_load_art_of_missing_file_uses_folder_image() {
        let dir = TempDir::new();
        let missing = dir.path("missing.mp3");
        assert!(load_art(&missing).picture.is_none());

        fs::write(dir.path("folder.png"), png(5, 5)).unwrap();
        assert_eq!(load_art(&missing).picture.unwrap().mime_type, "image/png");
    }

    /// フォルダの画像・アートなしのキャッシュは、フォルダか画像が変わると使えなくなる
    #[cfg(unix)]
    #[test]
    fn test_cached_art_without_embedded_picture_follows_folder_changes() {
        let dir = TempDir::new();
        let track = dir.track("01.wav");

        // アートなし → フォルダに画像が置かれた
        let none = CachedArt::from(load_art(&track));
        assert!(none.picture.is_none());
        assert!(none.is_current());
        fs::write(dir.path("cover.jpg"), jpeg(20, 20)).unwrap();
        bump_modified(&dir.0);
        assert!(!none.is_current());

        // フォルダの画像 → 中身が書き換えられた
        let folder = CachedArt::from(load_art(&track));
        assert!(folder.picture.is_some());
        assert!(folder.is_current());
        fs::write(dir.path("cover.jpg"), jpeg(40, 40)).unwrap();
        bump_modified(&dir.path("cover.jpg"));
        assert!(!folder.is_current());

        // フォルダの画像 → 消された
        let folder = CachedArt::from(load_art(&track));
        assert!(folder.is_current());
        fs::remove_file(dir.path("cover.jpg")).unwrap();
        assert!(!folder.is_current());
    }

    #[test]
    fn test_cached_embedded_picture_does_not_depend_on_folder() {
        let dir = TempDir::new();
        let track = dir.track("01.wav");
        let embedded = EmbeddedPicture {
            data: jpeg(20, 20),
            mime_type: "image/jpeg".to_string(),
        };
        set_file_album_art(&track, &embedded).unwrap();

        let cached = CachedArt::from(load_art(&track));
        fs::write(dir.path("cover.jpg"), jpeg(40, 40)).unwrap();
        assert!(cached.folder.is_none());
        assert!(cached.is_current());
    }

    #[test]
    fn test_info_for_file_describes_where_the_art_comes_from() {
        let dir = TempDir::new();
        let track = dir.track("01.wav");
        assert_eq!(info_for_file(&track), None);

        fs::write(dir.path("Folder.JPG"), jpeg(640, 480)).unwrap();
        assert_eq!(
            info_for_file(&track),
            Some(AlbumArtInfo {
                source: AlbumArtSource::Folder,
                file_name: Some("Folder.JPG".to_string()),
                mime_type: "image/jpeg".to_string(),
                size: jpeg(640, 480).len() as u32,
                width: Some(640),
                height: Some(480),
            })
        );

        let embedded = EmbeddedPicture {
            data: png(300, 200),
            mime_type: "image/png".to_string(),
        };
        set_file_album_art(&track, &embedded).unwrap();
        assert_eq!(
            info_for_file(&track),
            Some(AlbumArtInfo {
                source: AlbumArtSource::Embedded,
                file_name: None,
                mime_type: "image/png".to_string(),
                size: png(300, 200).len() as u32,
                width: Some(300),
                height: Some(200),
            })
        );
    }
}
