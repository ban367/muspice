//! アルバムアートの編集（埋め込み・取り除き）と、情報の取得のコマンド
//!
//! 埋め込む画像は、Rust側で開くダイアログで選ぶ。WebViewからはパスも画像データも受け取らない
//! （ADR-005・ADR-035）。編集画面（ADR-094）は、選んだ画像を「保存」まで書き込まないため、選んだ画像を
//! Rust側が預かり（`PendingAlbumArt`）、画面には見本の画像（data URL）と、預かった画像を指す印を返す。
//! 画像は音楽ファイルのタグへ書き込み、書き込み後のファイルのサイズと更新日時を記録する
//! （再スキャンで、自分の書き込みを外部の変更として読み直さないため）。

use super::metadata_cmd::{BulkUpdateResult, file_state_of};
use super::{file_dialog, run_blocking};
use crate::error::{AppError, AppResult};
use crate::events::TracksEdited;
use crate::library::to_count;
use crate::media_controls::MediaControls;
use crate::metadata::{
    EmbeddedPicture, MAX_ALBUM_ART_BYTES, remove_file_album_art, set_file_album_art,
    sniff_image_mime_type,
};
use crate::models::AlbumArtInfo;
use crate::settings::Language;
use crate::state::AppState;
use crate::validation::validate_track_id;
use serde::Serialize;
use specta::Type;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 1回のトランザクションで記録するトラック数（メタデータの一括編集と同じ）
const BATCH_SIZE: usize = 50;

/// ダイアログの文言（設定の言語に合わせる）
struct Labels {
    pick_title: &'static str,
    filter: &'static str,
}

impl Labels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Ja => Self {
                pick_title: "アルバムアートにする画像を選ぶ",
                filter: "画像（JPEG・PNG）",
            },
            Language::En => Self {
                pick_title: "Choose an Image for the Album Art",
                filter: "Images (JPEG, PNG)",
            },
        }
    }
}

/// 編集画面で選んだ、まだ書き込んでいない画像（編集画面を閉じると捨てる）
#[derive(Default)]
pub struct PendingAlbumArt {
    /// （画像を指す印, 画像）。選び直すと置き換える
    picture: Mutex<Option<(String, EmbeddedPicture)>>,
}

impl PendingAlbumArt {
    fn hold(&self, picture: EmbeddedPicture) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        if let Ok(mut current) = self.picture.lock() {
            *current = Some((token.clone(), picture));
        }
        token
    }

    /// 印の画像を返す（違う印・預かっていない時はNone）
    fn get(&self, token: &str) -> Option<EmbeddedPicture> {
        let current = self.picture.lock().ok()?;
        current
            .as_ref()
            .filter(|(held, _)| held == token)
            .map(|(_, picture)| picture.clone())
    }

    pub fn clear(&self) {
        if let Ok(mut current) = self.picture.lock() {
            *current = None;
        }
    }
}

/// 編集画面で選んだ画像（まだ書き込んでいない）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickedAlbumArt {
    /// 預かった画像を指す印（保存する時に`set_album_art`へ渡す）
    pub token: String,
    /// 見本に出す画像（`data:`のURL）
    pub data_url: String,
    pub mime_type: String,
    /// 大きさ（バイト）
    pub size: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// トラックのアルバムアートの情報（埋め込みの画像か、フォルダの画像か・種類・大きさ）を取得する
///
/// アートがなければNone。
#[tauri::command]
#[specta::specta]
pub async fn get_album_art_info(
    track_id: String,
    state: State<'_, AppState>,
) -> AppResult<Option<AlbumArtInfo>> {
    validate_track_id(&track_id)?;

    let file_path =
        state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &track_id))?;

    run_blocking(move || Ok(crate::album_art::info_for_file(Path::new(&file_path)))).await
}

/// アルバムアートにする画像ファイルを選び、書き込まずに預かる（編集画面の「画像を選ぶ」）
///
/// 画像（JPEG・PNG、10MBまで）を確かめて預かり、見本の画像と印を返す。選び直すと、前の画像は捨てる。
/// 画像を選ばなかった場合はNoneを返す。
#[tauri::command]
#[specta::specta]
pub async fn pick_album_art(app: AppHandle) -> AppResult<Option<PickedAlbumArt>> {
    run_blocking(move || {
        let labels = Labels::for_language(crate::menu::current_language(&app));
        let Some(file) = file_dialog(&app)
            .set_title(labels.pick_title)
            .add_filter(labels.filter, &["jpg", "jpeg", "png"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|e| AppError::Io(format!("選んだファイルを扱えません: {}", e)))?;
        let picture = read_picture_file(&path)?;
        Ok(Some(hold_picture(&app.state::<PendingAlbumArt>(), picture)))
    })
    .await
}

/// 画像を預かり、画面に返す情報を作る
fn hold_picture(pending: &PendingAlbumArt, picture: EmbeddedPicture) -> PickedAlbumArt {
    let dimensions = crate::metadata::image_dimensions(&picture.data);
    let data_url = format!(
        "data:{};base64,{}",
        picture.mime_type,
        encode_base64(&picture.data)
    );
    let size = u32::try_from(picture.data.len()).unwrap_or(u32::MAX);
    let mime_type = picture.mime_type.clone();
    PickedAlbumArt {
        token: pending.hold(picture),
        data_url,
        mime_type,
        size,
        width: dimensions.map(|(width, _)| width),
        height: dimensions.map(|(_, height)| height),
    }
}

/// 預かった画像（`pick_album_art`の印）を、トラックのファイルにアルバムアートとして埋め込む
///
/// 画像は縮小せず、そのまま埋め込む。表示している画像（フロントカバー）を置き換える。
/// 書き込めなかったトラックは、結果の`errors`へ理由を入れて残りを続ける。預かっていない印は`VALIDATION`。
#[tauri::command]
#[specta::specta]
pub async fn set_album_art(
    track_ids: Vec<String>,
    token: String,
    app: AppHandle,
) -> AppResult<BulkUpdateResult> {
    validate_track_ids(&track_ids)?;
    super::reject_cue_tracks(&app, &track_ids)?;
    let picture = app.state::<PendingAlbumArt>().get(&token).ok_or_else(|| {
        AppError::Validation("画像が見つかりません。もう一度選んでください".to_string())
    })?;

    run_blocking(move || {
        let state = app.state::<AppState>();
        let result = embed_album_art(&state, &track_ids, &picture)?;
        forget_now_playing_artwork(&app, &track_ids);
        notify_album_art_edited(&app, &result);
        Ok(result)
    })
    .await
}

/// トラックのファイルから、埋め込みの画像をすべて取り除く
///
/// 埋め込みの画像がないトラックは書き換えず、結果の件数にも数えない。
#[tauri::command]
#[specta::specta]
pub async fn remove_album_art(
    track_ids: Vec<String>,
    app: AppHandle,
) -> AppResult<BulkUpdateResult> {
    validate_track_ids(&track_ids)?;
    super::reject_cue_tracks(&app, &track_ids)?;

    run_blocking(move || {
        let state = app.state::<AppState>();
        let result = strip_album_art(&state, &track_ids)?;
        forget_now_playing_artwork(&app, &track_ids);
        notify_album_art_edited(&app, &result);
        Ok(result)
    })
    .await
}

fn validate_track_ids(track_ids: &[String]) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "トラックIDが指定されていません".to_string(),
        ));
    }
    track_ids.iter().try_for_each(|id| validate_track_id(id))
}

/// OSのNow Playingへ渡している画像が、書き換えた曲のものなら捨てる（次の更新で読み直す）
fn forget_now_playing_artwork(app: &AppHandle, track_ids: &[String]) {
    if let Some(media_controls) = app.try_state::<MediaControls>() {
        media_controls.forget_artwork(track_ids);
    }
}

/// 書き換えた曲の画像を、ほかのウィンドウ（メインウィンドウの一覧）に読み直させる
fn notify_album_art_edited(app: &AppHandle, result: &BulkUpdateResult) {
    if result.updated_track_ids.is_empty() {
        return;
    }
    let event = TracksEdited {
        track_ids: result.updated_track_ids.clone(),
        album_art: true,
    };
    if let Err(e) = event.emit(app) {
        log::warn!("Failed to emit the album art edit event: {}", e);
    }
}

/// バイト列をBase64（RFC 4648。パディングあり）の文字列にする（見本の画像のdata URLに使う）
fn encode_base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let group = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        for position in 0..4 {
            if position <= chunk.len() {
                let index = (group >> (18 - 6 * position)) & 0x3f;
                encoded.push(char::from(ALPHABET[index as usize]));
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

/// 選んだ画像ファイルを読む（JPEG・PNGで、大きすぎないこと）
fn read_picture_file(path: &Path) -> AppResult<EmbeddedPicture> {
    let size = std::fs::metadata(path)
        .map_err(|e| AppError::Io(format!("画像を読めません: {}", e)))?
        .len();
    if size > MAX_ALBUM_ART_BYTES {
        return Err(AppError::Validation(format!(
            "画像が大きすぎます（{}MBまで）",
            MAX_ALBUM_ART_BYTES / (1024 * 1024)
        )));
    }

    let data = std::fs::read(path).map_err(|e| AppError::Io(format!("画像を読めません: {}", e)))?;
    let mime_type = sniff_image_mime_type(&data)
        .ok_or_else(|| AppError::Validation("JPEGまたはPNGの画像を選んでください".to_string()))?
        .to_string();
    Ok(EmbeddedPicture { data, mime_type })
}

/// 画像を、各トラックのファイルに埋め込む
fn embed_album_art(
    state: &AppState,
    track_ids: &[String],
    picture: &EmbeddedPicture,
) -> AppResult<BulkUpdateResult> {
    write_album_art(state, track_ids, |path| {
        set_file_album_art(path, picture).map(|()| true)
    })
}

/// 各トラックのファイルから、埋め込みの画像を取り除く
fn strip_album_art(state: &AppState, track_ids: &[String]) -> AppResult<BulkUpdateResult> {
    write_album_art(state, track_ids, remove_file_album_art)
}

/// 各トラックのファイルのアルバムアートを書き換え、ファイルの状態を記録する
///
/// `write`は、ファイルを書き換えたかを返す（書き換えなかったトラックは、件数に数えない）。
/// 書き換えたトラックは、アルバムアートのキャッシュから捨てる。
fn write_album_art(
    state: &AppState,
    track_ids: &[String],
    write: impl Fn(&Path) -> AppResult<bool>,
) -> AppResult<BulkUpdateResult> {
    let mut result = BulkUpdateResult::default();

    for chunk in track_ids.chunks(BATCH_SIZE) {
        // 1. ロック外: ファイルへ書き込む
        // （トラックID, ファイルのサイズ, ファイルの更新日時）
        let mut written: Vec<(String, Option<i64>, Option<i64>)> = Vec::new();

        for track_id in chunk {
            // 同じ曲を書き換えている処理が終わるまで待つ（1曲ずつ取り、書き込んだら外す）
            let _file = state.lock_track_file(track_id);
            let file_path =
                state.with_db(|db| crate::repository::find_file_path_by_track_id(db, track_id));
            let outcome = file_path.and_then(|file_path| {
                let path = Path::new(&file_path);
                write(path)
                    .map(|changed| changed.then(|| file_state_of(path)))
                    .map_err(|e| AppError::Metadata(format!("{}: {}", file_path, e)))
            });
            match outcome {
                Ok(Some((file_size, file_modified_at))) => {
                    written.push((track_id.clone(), file_size, file_modified_at))
                }
                Ok(None) => {}
                Err(e) => {
                    result.errors.push(e.to_string());
                    result.failed_count += 1;
                }
            }
        }

        if written.is_empty() {
            continue;
        }

        // 2. 書き換えたトラックの画像を読み直させる（記録に失敗しても、古い画像を出さない）
        let written_ids: Vec<String> = written.iter().map(|(id, _, _)| id.clone()).collect();
        if let Ok(mut cache) = state.album_art_cache.lock() {
            cache.invalidate(&written_ids);
        }

        // 3. ロック内: ファイルのサイズ・更新日時をまとめて記録する
        state.with_db(|db| {
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;
            for (track_id, file_size, file_modified_at) in &written {
                if let Some(file_size) = file_size {
                    crate::repository::set_track_file_state(
                        &tx,
                        track_id,
                        *file_size,
                        *file_modified_at,
                    )?;
                }
            }
            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })
        })?;
        result.updated_count += to_count(written.len())?;
        result.updated_track_ids.extend(written_ids);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album_art::picture_for_track;
    use crate::metadata::extract_album_art;
    use crate::metadata::test_images::{jpeg, png, write_wav};
    use std::path::PathBuf;

    struct Fixture {
        state: AppState,
        dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let conn = crate::db::open_in_memory();
            let dir =
                std::env::temp_dir().join(format!("muspice-art-cmd-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self {
                state: AppState::new(conn),
                dir,
            }
        }

        /// 音楽ファイルを作り、ライブラリに登録する（`exists`がfalseなら、登録だけする）
        fn add_track(&self, id: &str, exists: bool) -> PathBuf {
            let path = self.dir.join(format!("{id}.wav"));
            if exists {
                write_wav(&path);
            }
            self.state
                .with_db(|db| {
                    db.execute(
                        "INSERT INTO tracks (id, file_path, file_name, title, format, file_size,
                                             file_modified_at, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?1, 'wav', 1, 1, '2026-01-01', '2026-01-01')",
                        rusqlite::params![id, path.to_string_lossy(), format!("{id}.wav")],
                    )
                    .map_err(|e| AppError::Database(e.to_string()))
                })
                .unwrap();
            path
        }

        fn file_state(&self, id: &str) -> (i64, Option<i64>) {
            self.state
                .with_db(|db| {
                    db.query_row(
                        "SELECT file_size, file_modified_at FROM tracks WHERE id = ?1",
                        [id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(|e| AppError::Database(e.to_string()))
                })
                .unwrap()
        }

        fn updated_at(&self, id: &str) -> String {
            self.state
                .with_db(|db| {
                    db.query_row("SELECT updated_at FROM tracks WHERE id = ?1", [id], |row| {
                        row.get(0)
                    })
                    .map_err(|e| AppError::Database(e.to_string()))
                })
                .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn picture(data: Vec<u8>) -> EmbeddedPicture {
        EmbeddedPicture {
            mime_type: sniff_image_mime_type(&data).unwrap().to_string(),
            data,
        }
    }

    #[test]
    fn test_embed_writes_files_and_records_their_state() {
        let fixture = Fixture::new();
        let a = fixture.add_track("a", true);
        let b = fixture.add_track("b", true);
        let untouched = fixture.add_track("c", true);

        let result =
            embed_album_art(&fixture.state, &ids(&["a", "b"]), &picture(jpeg(600, 600))).unwrap();

        assert_eq!((result.updated_count, result.failed_count), (2, 0));
        for (id, path) in [("a", &a), ("b", &b)] {
            assert_eq!(
                extract_album_art(path).unwrap().unwrap().data,
                jpeg(600, 600)
            );
            // 書き込み後のファイルのサイズ・更新日時を記録する（再スキャンで変更とみなさない）
            let (size, modified_at) = fixture.file_state(id);
            assert_eq!(size, std::fs::metadata(path).unwrap().len() as i64);
            assert_eq!(modified_at, crate::library::get_file_modified_at(path));
            // タグの内容（一覧に出す項目）は変わらないため、更新日時は変えない
            assert_eq!(fixture.updated_at(id), "2026-01-01");
        }
        assert!(extract_album_art(&untouched).unwrap().is_none());
        assert_eq!(fixture.file_state("c"), (1, Some(1)));
    }

    #[test]
    fn test_embed_reports_tracks_that_cannot_be_written() {
        let fixture = Fixture::new();
        let a = fixture.add_track("a", true);
        fixture.add_track("missing", false);

        let result = embed_album_art(
            &fixture.state,
            &ids(&["missing", "a", "unknown"]),
            &picture(png(100, 100)),
        )
        .unwrap();

        // 書き込めなかったトラックがあっても、残りは続ける
        assert_eq!((result.updated_count, result.failed_count), (1, 2));
        assert_eq!(result.errors.len(), 2);
        assert!(result.errors[0].contains("missing.wav"));
        assert!(extract_album_art(&a).unwrap().is_some());
        // 書き込めなかったトラックの記録は変えない
        assert_eq!(fixture.file_state("missing"), (1, Some(1)));
    }

    #[test]
    fn test_strip_removes_pictures_and_skips_tracks_without_them() {
        let fixture = Fixture::new();
        let with_art = fixture.add_track("a", true);
        fixture.add_track("b", true);
        embed_album_art(&fixture.state, &ids(&["a"]), &picture(jpeg(50, 50))).unwrap();
        let plain_state = fixture.file_state("b");

        let result = strip_album_art(&fixture.state, &ids(&["a", "b"])).unwrap();

        // 埋め込みの画像がないトラックは書き換えず、件数にも数えない
        assert_eq!((result.updated_count, result.failed_count), (1, 0));
        assert!(extract_album_art(&with_art).unwrap().is_none());
        assert_eq!(fixture.file_state("b"), plain_state);
        let (size, _) = fixture.file_state("a");
        assert_eq!(size, std::fs::metadata(&with_art).unwrap().len() as i64);
    }

    #[test]
    fn test_writing_album_art_drops_cached_pictures_of_those_tracks() {
        let fixture = Fixture::new();
        let a = fixture.add_track("a", true);
        let b = fixture.add_track("b", true);
        let path = |path: &PathBuf| path.to_string_lossy().into_owned();
        // キャッシュに「アートなし」を記録させる
        assert!(picture_for_track(&fixture.state, "a", &path(&a)).is_none());
        assert!(picture_for_track(&fixture.state, "b", &path(&b)).is_none());

        embed_album_art(&fixture.state, &ids(&["a"]), &picture(jpeg(50, 50))).unwrap();

        let cached = |id: &str| fixture.state.album_art_cache.lock().unwrap().get(id);
        assert!(cached("a").is_none());
        assert!(cached("b").is_some());
        assert_eq!(
            picture_for_track(&fixture.state, "a", &path(&a))
                .unwrap()
                .data,
            jpeg(50, 50)
        );

        strip_album_art(&fixture.state, &ids(&["a"])).unwrap();
        assert!(cached("a").is_none());
        assert!(picture_for_track(&fixture.state, "a", &path(&a)).is_none());
    }

    #[test]
    fn test_encode_base64_matches_rfc4648() {
        // RFC 4648の例
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode_base64(input.as_bytes()), expected, "{input}");
        }
        assert_eq!(encode_base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    /// 選んだ画像は、印で取り出せる。選び直すと前の印は使えなくなり、編集画面を閉じると捨てる
    #[test]
    fn test_pending_album_art_holds_the_latest_picture() {
        let pending = PendingAlbumArt::default();
        let first = hold_picture(&pending, picture(png(4, 3)));
        assert!(first.data_url.starts_with("data:image/png;base64,"));
        assert_eq!((first.width, first.height), (Some(4), Some(3)));
        assert_eq!(first.mime_type, "image/png");
        assert!(pending.get(&first.token).is_some());

        let second = hold_picture(&pending, picture(jpeg(8, 8)));
        assert_ne!(first.token, second.token);
        assert!(pending.get(&first.token).is_none());
        assert_eq!(
            pending.get(&second.token).map(|p| p.mime_type),
            Some("image/jpeg".to_string())
        );

        pending.clear();
        assert!(pending.get(&second.token).is_none());
    }

    #[test]
    fn test_read_picture_file_accepts_only_jpeg_and_png_within_the_size_limit() {
        let fixture = Fixture::new();
        let file = |name: &str, data: &[u8]| {
            let path = fixture.dir.join(name);
            std::fs::write(&path, data).unwrap();
            path
        };

        let picture = read_picture_file(&file("a.png", &png(10, 10))).unwrap();
        assert_eq!(picture.mime_type, "image/png");
        // 拡張子ではなく、中身で判定する
        let picture = read_picture_file(&file("b.png", &jpeg(10, 10))).unwrap();
        assert_eq!(picture.mime_type, "image/jpeg");

        assert!(matches!(
            read_picture_file(&file("c.jpg", b"GIF89a")),
            Err(AppError::Validation(_))
        ));
        let huge = fixture.dir.join("huge.jpg");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_ALBUM_ART_BYTES + 1)
            .unwrap();
        assert!(matches!(
            read_picture_file(&huge),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            read_picture_file(&fixture.dir.join("none.jpg")),
            Err(AppError::Io(_))
        ));
    }

    #[test]
    fn test_validate_track_ids() {
        assert!(validate_track_ids(&[]).is_err());
        assert!(validate_track_ids(&ids(&["not-a-uuid"])).is_err());
        assert!(validate_track_ids(&ids(&["550e8400-e29b-41d4-a716-446655440000"])).is_ok());
    }
}
