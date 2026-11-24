//! メタデータ編集関連コマンド
//!
//! メタデータ（タイトルなど）と評価は、音楽ファイルのタグを正とする。編集は常にファイルへ
//! 書き込み、データベースには同じ値を記録する（一覧・検索のため）。ファイルへ書き込めない
//! 場合はエラーにし、データベースも変えない。

use super::import::create_track_from_file;
use super::organize::{KeepOrganizedResult, keep_music_folder_organized};
use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::library::to_count;
use crate::metadata::{update_file_metadata, validate_metadata};
use crate::models::{Metadata, Track};
use crate::multi_value;
use crate::state::AppState;
use crate::validation::{validate_string_length, validate_track_id};
use chrono::Utc;
use std::path::Path;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 1回のトランザクションで書き込むトラック数（インポートと同じ）
const BATCH_SIZE: usize = 50;

/// メタデータの内容と各フィールドの長さをまとめてバリデーション
/// コメントの長さの上限（文字数）
const MAX_COMMENT_LENGTH: usize = 2_000;

/// 歌詞の長さの上限（文字数）
const MAX_LYRICS_LENGTH: usize = 50_000;

/// アーティスト・ジャンルの入力を、複数の値の形にそろえる（`A ;B` → `A; B`）
///
/// ファイルから読み直した時と同じ文字列を、データベースに記録するため（`multi_value::display`）。
/// 複数の値でない入力は、そのまま。
fn normalize_multi_values(mut metadata: Metadata) -> Metadata {
    for value in [&mut metadata.artist, &mut metadata.genre] {
        if let Some(text) = value
            && text.contains(multi_value::SEPARATOR)
        {
            *text = multi_value::join(&multi_value::split(text));
        }
    }
    metadata
}

fn validate_metadata_input(metadata: &Metadata) -> AppResult<()> {
    validate_metadata(metadata)?;
    validate_string_length(&metadata.title, "タイトル", 255)?;
    validate_string_length(&metadata.artist, "アーティスト", 255)?;
    validate_string_length(&metadata.album, "アルバム", 255)?;
    validate_string_length(&metadata.genre, "ジャンル", 100)?;
    validate_string_length(&metadata.album_artist, "アルバムアーティスト", 255)?;
    validate_string_length(&metadata.composer, "作曲者", 255)?;
    validate_string_length(&metadata.grouping, "グループ", 255)?;
    validate_string_length(&metadata.comment, "コメント", MAX_COMMENT_LENGTH)?;
    validate_string_length(&metadata.lyrics, "歌詞", MAX_LYRICS_LENGTH)?;
    for (value, name) in [
        (&metadata.title_sort, "タイトルの読み"),
        (&metadata.artist_sort, "アーティストの読み"),
        (&metadata.album_sort, "アルバムの読み"),
        (&metadata.album_artist_sort, "アルバムアーティストの読み"),
    ] {
        validate_string_length(value, name, 255)?;
    }
    Ok(())
}

/// トラックのタグ（編集画面で扱うすべての項目）を、ファイルから読む
///
/// 作曲者・コメント・歌詞などはデータベースに保存していないため、編集画面を開く時に呼ぶ。
/// ファイルが見つからない・読めない場合はエラー（その曲は、タグを書き込むこともできない）。
#[tauri::command]
#[specta::specta]
pub async fn get_track_tags(track_id: String, state: State<'_, AppState>) -> AppResult<Metadata> {
    validate_track_id(&track_id)?;

    let file_path =
        state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &track_id))?;

    run_blocking(move || crate::metadata::read_file_tags(Path::new(&file_path))).await
}

/// 曲の歌詞を取得（曲と同じ名前の`.lrc`ファイルを優先し、なければ埋め込みの歌詞。どちらもなければnull）
///
/// 時刻付きの歌詞は、LRC形式の文字列のまま返す（解釈はフロントエンドが行う）。
#[tauri::command]
#[specta::specta]
pub async fn get_track_lyrics(
    track_id: String,
    state: State<'_, AppState>,
) -> AppResult<Option<crate::lyrics::TrackLyrics>> {
    validate_track_id(&track_id)?;

    let file_path =
        state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &track_id))?;
    // CUEシートの曲には、歌詞を読まない（曲ごとの歌詞のファイル・タグがない。ADR-123）
    if crate::cue::is_virtual(&file_path) {
        return Ok(None);
    }

    run_blocking(move || crate::lyrics::read_track_lyrics(Path::new(&file_path))).await
}

/// 書き込み後のファイルのサイズと更新日時を取得する
///
/// データベースに記録し、再スキャンで自分の書き込みを変更として検出しないようにする。
pub(super) fn file_state_of(path: &Path) -> (Option<i64>, Option<i64>) {
    (
        crate::library::get_file_size(path).ok(),
        crate::library::get_file_modified_at(path),
    )
}

/// トラックのメタデータを更新（ファイルのタグとデータベース）
///
/// 編集画面のすべての項目を反映する。値のない項目は、タグからも取り除く。
/// 設定「音楽フォルダを整理された状態に保つ」がオンで、音楽フォルダの中の曲なら、保存した後に
/// 書式どおりの場所へ移し直す（ADR-071）。
///
/// @returns 移し直した結果（移し直さない場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn update_track_metadata(
    track_id: String,
    metadata: Metadata,
    app: AppHandle,
) -> AppResult<Option<KeepOrganizedResult>> {
    validate_track_id(&track_id)?;
    validate_metadata_input(&metadata)?;
    super::reject_cue_tracks(&app, std::slice::from_ref(&track_id))?;
    let metadata = normalize_multi_values(metadata);

    // ファイルへの書き込み（ファイルI/OのためDBロック外・ブロッキング処理用スレッドで実行）と、
    // データベースへの記録を、同じ曲を書き換えている処理（ラウドネスの解析など）と重ねない
    let writer = app.clone();
    let written_id = track_id.clone();
    run_blocking(move || {
        let state = writer.state::<AppState>();
        let _file = state.lock_track_file(&written_id);
        let file_path =
            state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &written_id))?;
        let path = Path::new(&file_path);
        update_file_metadata(path, &metadata, true)?;
        let (file_size, file_modified_at) = file_state_of(path);

        // データベースのメタデータと、ファイルのサイズ・更新日時を1つのトランザクションで更新する
        // （片方だけ反映されると、次の再スキャンで自分の書き込みを外部の変更として読み直すため）
        state.with_db(|db| {
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;
            crate::repository::update_track_metadata(&tx, &written_id, &metadata)?;
            if let Some(file_size) = file_size {
                crate::repository::set_track_file_state(
                    &tx,
                    &written_id,
                    file_size,
                    file_modified_at,
                )?;
            }
            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })
        })
    })
    .await?;
    notify_tracks_edited(&app, std::slice::from_ref(&track_id));

    // ファイルの移動はブロッキング処理用スレッドで行う
    run_blocking(move || Ok(keep_music_folder_organized(&app, &[track_id]))).await
}

/// タグを書き換えた曲を、ほかのウィンドウ（メインウィンドウの一覧）に知らせる（ADR-094）
fn notify_tracks_edited(app: &AppHandle, track_ids: &[String]) {
    if track_ids.is_empty() {
        return;
    }
    let event = crate::events::TracksEdited {
        track_ids: track_ids.to_vec(),
        album_art: false,
    };
    if let Err(e) = event.emit(app) {
        log::warn!("Failed to emit the tag edit event: {}", e);
    }
}

/// 一括編集の結果
#[derive(Debug, Clone, Default, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BulkUpdateResult {
    /// ファイルとデータベースを更新できたトラック数
    pub updated_count: u32,
    /// 更新できなかったトラック数（ファイルが見つからない・書き込めないなど）
    pub failed_count: u32,
    /// 更新できなかったトラックの理由（ファイルごと）
    pub errors: Vec<String>,
    /// 音楽フォルダの曲を、書式どおりの場所へ移し直した結果（移し直さない場合はnull。ADR-071）
    pub organized: Option<KeepOrganizedResult>,
    /// 更新できたトラックのID（移し直す曲を決めるために使う。画面には渡さない）
    #[serde(skip)]
    pub updated_track_ids: Vec<String>,
}

/// 複数トラックのメタデータを一括更新（ファイルのタグとデータベース）
///
/// 値がある項目だけを変える。ファイルへ書き込めなかったトラックは、データベースも変えずに
/// 結果の`errors`へ理由を入れ、残りのトラックの更新を続ける。設定「音楽フォルダを整理された状態に
/// 保つ」がオンなら、更新できた音楽フォルダの中の曲を、書式どおりの場所へ移し直す（`organized`）。
#[tauri::command]
#[specta::specta]
pub async fn update_multiple_tracks_metadata(
    track_ids: Vec<String>,
    metadata: Metadata,
    app: AppHandle,
) -> AppResult<BulkUpdateResult> {
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "トラックIDが指定されていません".to_string(),
        ));
    }

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    validate_metadata_input(&metadata)?;
    super::reject_cue_tracks(&app, &track_ids)?;
    let metadata = normalize_multi_values(metadata);

    // ファイルの書き込みとバッチ書き込みはブロッキング処理用スレッドで行う
    run_blocking(move || {
        let updates: Vec<(&String, &Metadata)> =
            track_ids.iter().map(|id| (id, &metadata)).collect();
        let mut result = update_each_blocking(app.state::<AppState>().inner(), &updates)?;
        notify_tracks_edited(&app, &result.updated_track_ids);
        result.organized = keep_music_folder_organized(&app, &result.updated_track_ids);
        Ok(result)
    })
    .await
}

/// 曲ごとのメタデータの変更（タグの一括ツール）
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadataChange {
    pub track_id: String,
    /// 変える項目（値がある項目だけを変える。文字列の項目は、空の値で項目を取り除く）
    pub metadata: Metadata,
}

/// 曲ごとに違う値で、メタデータをまとめて更新（ファイルのタグとデータベース）
///
/// タグの一括ツール（ファイル名からの推定・連番の振り直し・検索と置換）が、確認した変更を
/// 書き込むために使う。扱いは`update_multiple_tracks_metadata`と同じで、値がある項目だけを変え、
/// ファイルへ書き込めなかったトラックは結果の`errors`へ理由を入れて残りを続ける。
/// 文字列の項目に空の値を渡すと、その項目をタグから取り除く。音楽フォルダの曲の移し直しも、
/// `update_multiple_tracks_metadata`と同じ。
#[tauri::command]
#[specta::specta]
pub async fn apply_metadata_changes(
    changes: Vec<TrackMetadataChange>,
    app: AppHandle,
) -> AppResult<BulkUpdateResult> {
    if changes.is_empty() {
        return Err(AppError::Validation("変更が指定されていません".to_string()));
    }

    // 1件でも不正な入力があれば、何も書き込まない
    for change in &changes {
        validate_track_id(&change.track_id)?;
        validate_metadata_input(&change.metadata)?;
    }
    let changed_ids: Vec<String> = changes
        .iter()
        .map(|change| change.track_id.clone())
        .collect();
    super::reject_cue_tracks(&app, &changed_ids)?;

    let changes: Vec<TrackMetadataChange> = changes
        .into_iter()
        .map(|change| TrackMetadataChange {
            track_id: change.track_id,
            metadata: normalize_multi_values(change.metadata),
        })
        .collect();

    run_blocking(move || {
        let updates: Vec<(&String, &Metadata)> = changes
            .iter()
            .map(|change| (&change.track_id, &change.metadata))
            .collect();
        let mut result = update_each_blocking(app.state::<AppState>().inner(), &updates)?;
        result.organized = keep_music_folder_organized(&app, &result.updated_track_ids);
        Ok(result)
    })
    .await
}

/// 一括編集・タグの一括ツールの本体（同期処理）: トラックごとのメタデータを書き込む
fn update_each_blocking(
    state: &AppState,
    updates: &[(&String, &Metadata)],
) -> AppResult<BulkUpdateResult> {
    let mut result = BulkUpdateResult::default();

    for chunk in updates.chunks(BATCH_SIZE) {
        // 1. ロック外: ファイルへ書き込む（書き込めたトラックだけをデータベースに反映する）
        // （トラックID, メタデータ, ファイルのサイズ, ファイルの更新日時）
        let mut written: Vec<(&String, &Metadata, Option<i64>, Option<i64>)> = Vec::new();

        for &(track_id, metadata) in chunk {
            // 同じ曲を書き換えている処理が終わるまで待つ（1曲ずつ取り、書き込んだら外す）
            let _file = state.lock_track_file(track_id);
            let file_path =
                state.with_db(|db| crate::repository::find_file_path_by_track_id(db, track_id));
            let outcome = file_path.and_then(|file_path| {
                let path = Path::new(&file_path);
                update_file_metadata(path, metadata, false)
                    .map(|()| file_state_of(path))
                    .map_err(|e| AppError::Metadata(format!("{}: {}", file_path, e)))
            });
            match outcome {
                Ok((file_size, file_modified_at)) => {
                    written.push((track_id, metadata, file_size, file_modified_at))
                }
                Err(e) => {
                    result.errors.push(e.to_string());
                    result.failed_count += 1;
                }
            }
        }

        if written.is_empty() {
            continue;
        }

        // 2. ロック内: バッチをまとめて書き込む
        state.with_db(|db| {
            let now = Utc::now().to_rfc3339();
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;

            for (track_id, metadata, file_size, file_modified_at) in &written {
                crate::repository::update_track_metadata_partial(&tx, track_id, metadata, &now)?;
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
        result
            .updated_track_ids
            .extend(written.iter().map(|(track_id, ..)| (*track_id).clone()));
    }

    Ok(result)
}

/// メタデータ更新の結果
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RefreshMetadataResult {
    pub updated_count: i32,
    pub skipped_count: i32,
    pub error_count: i32,
    pub errors: Vec<String>,
}

/// ライブラリ全体のメタデータを更新
///
/// 全トラックのファイルを読み直し、タグの内容（タイトルなど・評価・トラック番号・
/// ReplayGain）と長さなどをデータベースに反映する。お気に入り・再生回数は変えない。
#[tauri::command]
#[specta::specta]
pub async fn refresh_library_metadata(app: AppHandle) -> AppResult<RefreshMetadataResult> {
    // 全ファイルの読み取りとバッチ書き込みはブロッキング処理用スレッドで行う
    run_blocking(move || refresh_library_metadata_blocking(app.state::<AppState>().inner())).await
}

/// `refresh_library_metadata`の本体（同期処理）
///
/// 再スキャン・インポートと同じ曲を同時に書き換えないよう、終わるまで待ってから始める。
fn refresh_library_metadata_blocking(state: &AppState) -> AppResult<RefreshMetadataResult> {
    let _scan = state.lock_library_scan();
    let mut updated_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;
    let mut errors = Vec::new();

    // 全トラックのファイルパスを取得
    let tracks = state.with_db(|db| crate::repository::find_track_paths(db))?;

    let total_tracks = tracks.len();
    log::info!("Starting metadata update: {} tracks", total_tracks);

    // バッチ処理で更新
    //
    // ファイル読み取り（メタデータ抽出）はDBロックの外で行い、
    // ロックはバッチ単位の書き込みの間だけ保持する。
    for (batch_idx, chunk) in tracks.chunks(BATCH_SIZE).enumerate() {
        // 1. ロック外: ファイルからトラック情報を読み直す
        // （トラック, ファイルの更新日時）
        let mut pending: Vec<(Track, Option<i64>)> = Vec::new();

        for (_, file_path) in chunk {
            let path = Path::new(file_path);

            // ファイルが存在しない場合はスキップ
            if !path.exists() {
                skipped_count += 1;
                continue;
            }

            match create_track_from_file(path) {
                Ok(track) => pending.push((track, crate::library::get_file_modified_at(path))),
                Err(e) => {
                    errors.push(format!("{}: {}", file_path, e));
                    error_count += 1;
                }
            }
        }

        // 2. ロック内: バッチをまとめて書き込む
        if !pending.is_empty() {
            state.with_db(|db| {
                let tx = db.transaction().map_err(|e| {
                    AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
                })?;

                for (track, modified_at) in &pending {
                    match crate::repository::update_track_by_file_path(&tx, track, *modified_at) {
                        Ok(()) => updated_count += 1,
                        Err(e) => {
                            errors.push(format!("{}: DB更新失敗 - {}", track.file_path, e));
                            error_count += 1;
                        }
                    }
                }

                tx.commit().map_err(|e| {
                    AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
                })
            })?;
        }

        let processed = batch_idx * BATCH_SIZE + chunk.len();
        log::info!(
            "Metadata update progress: {}/{} tracks processed",
            processed,
            total_tracks
        );
    }

    log::info!(
        "Metadata update finished: updated={}, skipped={}, errors={}",
        updated_count,
        skipped_count,
        error_count
    );

    if updated_count > 0 {
        // ファイルの画像が差し替わった可能性があるため、アルバムアートを読み直させる
        if let Ok(mut cache) = state.album_art_cache.lock() {
            cache.clear();
        }
    }

    Ok(RefreshMetadataResult {
        updated_count,
        skipped_count,
        error_count,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_multi_values_joins_artists_and_genres() {
        let input = Metadata {
            title: Some("A ;B".to_string()),
            artist: Some("A ;B;; A".to_string()),
            genre: Some(" Rock".to_string()),
            album: Some("X;Y".to_string()),
            ..Default::default()
        };
        let normalized = normalize_multi_values(input);
        // アーティスト・ジャンルだけ、複数の値の形にそろえる（複数の値でない入力は、そのまま）
        assert_eq!(normalized.artist.as_deref(), Some("A; B"));
        assert_eq!(normalized.genre.as_deref(), Some(" Rock"));
        assert_eq!(normalized.title.as_deref(), Some("A ;B"));
        assert_eq!(normalized.album.as_deref(), Some("X;Y"));
        // `;`だけの入力は、空（項目を取り除く指定）になる
        let empty = normalize_multi_values(Metadata {
            genre: Some(" ; ".to_string()),
            ..Default::default()
        });
        assert_eq!(empty.genre.as_deref(), Some(""));
    }

    /// 実際のファイルとDBを持つ、曲ごとの書き込みのテスト用の状態
    struct Fixture {
        state: AppState,
        dir: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let conn = crate::db::open_in_memory();
            let dir =
                std::env::temp_dir().join(format!("muspice-meta-cmd-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self {
                state: AppState::new(conn),
                dir,
            }
        }

        /// 音楽ファイルを作り、ライブラリに登録する（`exists`がfalseなら、登録だけする）
        fn add_track(&self, id: &str, exists: bool) -> std::path::PathBuf {
            let path = self.dir.join(format!("{id}.wav"));
            if exists {
                crate::metadata::test_images::write_wav(&path);
            }
            self.state
                .with_db(|db| {
                    db.execute(
                        "INSERT INTO tracks (id, file_path, file_name, title, artist, genre, format,
                                             file_size, file_modified_at, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?1, '元のアーティスト', 'Rock', 'wav', 1, 1,
                                 '2026-01-01', '2026-01-01')",
                        rusqlite::params![id, path.to_string_lossy(), format!("{id}.wav")],
                    )
                    .map_err(|e| AppError::Database(e.to_string()))
                })
                .unwrap();
            path
        }

        fn track(&self, id: &str) -> Track {
            self.state
                .with_db(|db| crate::repository::find_track_by_id(db, id))
                .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// 同じ曲を書き換えている処理（ラウドネスの解析など）があれば、終わるまで待ってから書く
    #[test]
    fn test_update_each_waits_for_another_writer_of_the_same_track() {
        let fixture = Fixture::new();
        let path = fixture.add_track("a", true);
        let id = "a".to_string();
        let metadata = Metadata {
            title: Some("Edited".to_string()),
            ..Metadata::default()
        };
        let (sender, done) = std::sync::mpsc::channel();

        std::thread::scope(|scope| {
            let writing = fixture.state.lock_track_file("a");
            let (state, id, metadata) = (&fixture.state, &id, &metadata);
            scope.spawn(move || {
                update_each_blocking(state, &[(id, metadata)]).unwrap();
                sender.send(()).unwrap();
            });
            // 印を持っている間は、ファイルを読まずに待つ
            assert!(
                done.recv_timeout(std::time::Duration::from_millis(200))
                    .is_err()
            );
            assert_eq!(crate::metadata::read_file_tags(&path).unwrap().title, None);
            drop(writing);
            done.recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
        });
        assert_eq!(
            crate::metadata::read_file_tags(&path)
                .unwrap()
                .title
                .as_deref(),
            Some("Edited")
        );
        assert_eq!(fixture.track("a").title.as_deref(), Some("Edited"));
    }

    /// 曲ごとに違う値を、ファイルのタグとDBへ書き込む（値のある項目だけを変える）
    #[test]
    fn test_update_each_writes_different_values_per_track() {
        let fixture = Fixture::new();
        let a = fixture.add_track("a", true);
        let b = fixture.add_track("b", true);
        let id_a = "a".to_string();
        let id_b = "b".to_string();
        let change_a = Metadata {
            title: Some("曲A".to_string()),
            track_number: Some(1),
            track_total: Some(2),
            ..Default::default()
        };
        let change_b = Metadata {
            title: Some("曲B".to_string()),
            track_number: Some(2),
            track_total: Some(2),
            // 空の値は、その項目を取り除く
            genre: Some(String::new()),
            ..Default::default()
        };

        let result =
            update_each_blocking(&fixture.state, &[(&id_a, &change_a), (&id_b, &change_b)])
                .unwrap();

        assert_eq!((result.updated_count, result.failed_count), (2, 0));
        // 更新できた曲（音楽フォルダの曲を移し直す対象を決める）
        assert_eq!(result.updated_track_ids, [id_a.clone(), id_b.clone()]);
        let tags_a = crate::metadata::read_file_tags(&a).unwrap();
        assert_eq!(tags_a.title.as_deref(), Some("曲A"));
        assert_eq!(
            (tags_a.track_number, tags_a.track_total),
            (Some(1), Some(2))
        );
        let tags_b = crate::metadata::read_file_tags(&b).unwrap();
        assert_eq!(tags_b.title.as_deref(), Some("曲B"));
        assert_eq!(tags_b.track_number, Some(2));
        assert_eq!(tags_b.genre, None);

        let track_a = fixture.track("a");
        assert_eq!(track_a.title.as_deref(), Some("曲A"));
        assert_eq!(track_a.track_number, Some(1));
        // 渡していない項目は変えない
        assert_eq!(track_a.artist.as_deref(), Some("元のアーティスト"));
        assert_eq!(track_a.genre.as_deref(), Some("Rock"));
        // 書き込み後のファイルのサイズを記録する（再スキャンで変更とみなさない）
        assert_eq!(
            track_a.file_size,
            std::fs::metadata(&a).unwrap().len() as i64
        );
        let track_b = fixture.track("b");
        assert_eq!(track_b.title.as_deref(), Some("曲B"));
        assert_eq!(track_b.genre, None);
    }

    /// ファイルへ書き込めなかったトラックは、DBも変えずに理由を返し、残りを続ける
    #[test]
    fn test_update_each_reports_tracks_that_cannot_be_written() {
        let fixture = Fixture::new();
        fixture.add_track("missing", false);
        fixture.add_track("a", true);
        let ids = [
            "missing".to_string(),
            "a".to_string(),
            "unknown".to_string(),
        ];
        let change = Metadata {
            title: Some("新しいタイトル".to_string()),
            ..Default::default()
        };
        let updates: Vec<(&String, &Metadata)> = ids.iter().map(|id| (id, &change)).collect();

        let result = update_each_blocking(&fixture.state, &updates).unwrap();

        assert_eq!((result.updated_count, result.failed_count), (1, 2));
        assert_eq!(result.updated_track_ids.len(), 1);
        assert!(result.errors[0].contains("missing.wav"));
        assert_eq!(fixture.track("a").title.as_deref(), Some("新しいタイトル"));
        assert_eq!(fixture.track("missing").title.as_deref(), Some("missing"));
    }

    /// ファイルから読み直した後は、アルバムアートのキャッシュを消す
    #[test]
    fn test_refresh_library_metadata_clears_album_art_cache() {
        let fixture = Fixture::new();
        let a = fixture.add_track("a", true);
        fixture.add_track("missing", false);
        // キャッシュに記録させる
        crate::album_art::picture_for_track(&fixture.state, "a", &a.to_string_lossy());
        let cached = || fixture.state.album_art_cache.lock().unwrap().get("a");
        assert!(cached().is_some());

        let result = refresh_library_metadata_blocking(&fixture.state).unwrap();

        assert_eq!((result.updated_count, result.skipped_count), (1, 1));
        assert!(cached().is_none());
    }

    /// 再スキャン・インポートの途中は、終わるまで待ってから読み直す
    #[test]
    fn test_refresh_library_metadata_waits_for_library_scan() {
        let fixture = Fixture::new();
        fixture.add_track("a", true);

        std::thread::scope(|scope| {
            let scan = fixture.state.lock_library_scan();
            let refresh = scope.spawn(|| refresh_library_metadata_blocking(&fixture.state));
            std::thread::sleep(std::time::Duration::from_millis(200));
            assert!(!refresh.is_finished(), "スキャンの途中に読み直している");

            drop(scan);
            let result = refresh.join().unwrap().unwrap();
            assert_eq!(result.updated_count, 1);
        });
    }
}
