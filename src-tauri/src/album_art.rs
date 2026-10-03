//! アルバムアートの配信（`albumart`カスタムプロトコル）
//!
//! WebViewは`albumart://localhost/<トラックID>`（Windowsでは
//! `http://albumart.localhost/<トラックID>`）を`<img>`の`src`に指定して画像を読み込む。
//! base64に変換してIPCで渡す方式と比べ、画像をバイト列のまま返せ、フロントエンドに
//! 画像データを溜め込む必要もない。
//!
//! 配信できるのはDBに登録済みのトラックのアートだけで、任意のファイルは読めない。
//! 抽出したアートは容量上限付きのLRUキャッシュに保持し、WebViewにはキャッシュさせない
//! （インポートでファイルが差し替わった場合に古い画像を表示しないため）。

use crate::metadata::{EmbeddedPicture, extract_album_art};
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, UriSchemeResponder};

/// カスタムプロトコルのスキーム名
pub const SCHEME: &str = "albumart";

/// キャッシュに保持する画像データの合計サイズの上限
const CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// キャッシュに保持するエントリ数の上限（アートがないトラックの記録を含む）
const CACHE_MAX_ENTRIES: usize = 2000;

/// 抽出済みアルバムアートのLRUキャッシュ
///
/// アートがないトラックも`None`として記録し、同じファイルを何度も読まないようにする。
pub struct AlbumArtCache {
    entries: HashMap<String, Option<EmbeddedPicture>>,
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

    /// キャッシュを参照する（ヒットしたエントリは最新として扱う）
    ///
    /// 戻り値の外側の`None`は未キャッシュ、内側の`None`はアートがないことを表す。
    pub fn get(&mut self, track_id: &str) -> Option<Option<EmbeddedPicture>> {
        let value = self.entries.get(track_id)?.clone();
        self.touch(track_id);
        Some(value)
    }

    /// エントリを追加し、上限を超えた分を古いものから捨てる
    ///
    /// 単体で上限を超える画像はキャッシュしない。
    pub fn insert(&mut self, track_id: String, picture: Option<EmbeddedPicture>) {
        let size = picture.as_ref().map_or(0, |p| p.data.len());
        if size > self.max_bytes {
            return;
        }

        self.remove(&track_id);
        self.total_bytes += size;
        self.order.push_back(track_id.clone());
        self.entries.insert(track_id, picture);

        while self.total_bytes > self.max_bytes || self.entries.len() > self.max_entries {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(Some(evicted)) = self.entries.remove(&oldest) {
                self.total_bytes -= evicted.data.len();
            }
        }
    }

    /// すべてのエントリを捨てる（インポートでファイルが差し替わった場合など）
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_bytes = 0;
    }

    fn remove(&mut self, track_id: &str) {
        if let Some(previous) = self.entries.remove(track_id) {
            self.total_bytes -= previous.map_or(0, |p| p.data.len());
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

/// トラックのアルバムアートをファイルから抽出してキャッシュに登録する（同期処理）
fn load_picture(app: &AppHandle, track_id: &str) -> Response<Vec<u8>> {
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

    let picture = match extract_album_art(Path::new(&file_path)) {
        Ok(picture) => picture,
        Err(e) => {
            // 読めないファイルは「アートなし」として扱い、何度も読みに行かない
            log::warn!("アルバムアートの抽出に失敗しました: {}: {}", file_path, e);
            None
        }
    };

    if let Ok(mut cache) = state.album_art_cache.lock() {
        cache.insert(track_id.to_string(), picture.clone());
    }
    picture_response(picture)
}

/// `albumart`プロトコルのリクエストを処理する
///
/// キャッシュにあればそのまま返し、なければブロッキング処理用スレッドでファイルから抽出する。
/// 一覧表示では表示中のカードの数だけ要求が同時に届くため、抽出は同時実行数を制限する。
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
        if let Some(picture) = cached {
            responder.respond(picture_response(picture));
            return;
        }

        let Ok(_permit) = state.album_art_limiter.acquire().await else {
            responder.respond(empty_response(StatusCode::SERVICE_UNAVAILABLE));
            return;
        };
        let app_for_load = app.clone();
        let response =
            tauri::async_runtime::spawn_blocking(move || load_picture(&app_for_load, &track_id))
                .await
                .unwrap_or_else(|_| empty_response(StatusCode::INTERNAL_SERVER_ERROR));
        responder.respond(response);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(size: usize) -> Option<EmbeddedPicture> {
        Some(EmbeddedPicture {
            data: vec![0; size],
            mime_type: "image/jpeg".to_string(),
        })
    }

    #[test]
    fn test_cache_returns_inserted_entries_including_missing_art() {
        let mut cache = AlbumArtCache::new(100, 10);
        cache.insert("a".into(), picture(10));
        cache.insert("b".into(), None);

        assert_eq!(cache.get("a").flatten().map(|p| p.data.len()), Some(10));
        assert!(matches!(cache.get("b"), Some(None)));
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
        cache.insert("a".into(), None);
        cache.insert("b".into(), None);
        cache.insert("c".into(), None);

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
    fn test_track_id_from_path() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(track_id_from_path(&format!("/{}", id)), Some(id));
        assert_eq!(track_id_from_path("/"), None);
        assert_eq!(track_id_from_path("/../etc/passwd"), None);
        assert_eq!(track_id_from_path(&format!("/{}/extra", id)), None);
    }
}
