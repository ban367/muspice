use crate::library::{
    delete_tracks, delete_tracks_with_files, get_default_title, get_file_format, get_file_size,
    is_duplicate_file, scan_directory, DeleteResult, DuplicateAction, ImportResult,
};
use crate::metadata::{
    extract_album_art, extract_bitrate, extract_duration, extract_metadata, extract_sample_rate,
    update_file_metadata, validate_metadata, AlbumArt,
};
use crate::models::{AlbumGroup, ArtistGroup, GenreGroup, Metadata, Track};
use crate::state::AppState;
use crate::validation::{
    sanitize_search_query, validate_file_path, validate_playlist_id, validate_playlist_name,
    validate_string_length, validate_track_id,
};
use chrono::Utc;
use serde::Deserialize;
use std::path::Path;
use tauri::State;
use uuid::Uuid;

/// フォルダから音楽ファイルをインポート（バッチ処理最適化版）
#[tauri::command]
pub async fn import_folder(
    folder_path: String,
    duplicate_action: DuplicateAction,
    state: State<'_, AppState>,
) -> Result<ImportResult, String> {
    // ファイルパスをバリデーション
    validate_file_path(&folder_path)?;

    let path = Path::new(&folder_path);

    // ディレクトリをスキャン
    let audio_files = scan_directory(path)?;

    let mut imported_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;
    let mut errors = Vec::new();

    let mut db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // バッチサイズ（一度にコミットするファイル数）
    const BATCH_SIZE: usize = 50;
    let total_files = audio_files.len();

    // バッチ処理でインポート
    for (batch_idx, chunk) in audio_files.chunks(BATCH_SIZE).enumerate() {
        // トランザクション開始
        let tx = db
            .transaction()
            .map_err(|e| format!("トランザクションの開始に失敗しました: {}", e))?;

        for file_path in chunk {
            let file_path_str = file_path
                .to_str()
                .ok_or_else(|| "ファイルパスの変換に失敗しました".to_string())?;

            // 重複チェック
            let is_duplicate = match is_duplicate_file(&tx, file_path_str) {
                Ok(dup) => dup,
                Err(e) => {
                    errors.push(format!("{}: {}", file_path_str, e));
                    error_count += 1;
                    continue;
                }
            };

            if is_duplicate {
                match duplicate_action {
                    DuplicateAction::Skip => {
                        skipped_count += 1;
                        continue;
                    }
                    DuplicateAction::Replace => {
                        // 既存のトラックを更新
                        match process_and_update_track_in_tx(&tx, file_path) {
                            Ok(_) => imported_count += 1,
                            Err(e) => {
                                errors.push(format!("{}: {}", file_path_str, e));
                                error_count += 1;
                            }
                        }
                    }
                }
            } else {
                // 新しいトラックを追加
                match process_and_save_track_in_tx(&tx, file_path) {
                    Ok(_) => imported_count += 1,
                    Err(e) => {
                        errors.push(format!("{}: {}", file_path_str, e));
                        error_count += 1;
                    }
                }
            }
        }

        // バッチをコミット
        tx.commit()
            .map_err(|e| format!("トランザクションのコミットに失敗しました: {}", e))?;

        // 進行状況をログ出力（将来的にはイベントで通知可能）
        let processed = (batch_idx + 1) * BATCH_SIZE.min(total_files - batch_idx * BATCH_SIZE);
        log::info!(
            "インポート進行状況: {}/{} ファイル処理完了",
            processed,
            total_files
        );
    }

    Ok(ImportResult {
        imported_count,
        skipped_count,
        error_count,
        errors,
    })
}

/// トラックを処理してトランザクション内で保存
fn process_and_save_track_in_tx(
    tx: &rusqlite::Transaction,
    file_path: &Path,
) -> Result<(), String> {
    let track = create_track_from_file(file_path)?;
    tx.execute(
        "INSERT INTO tracks (
            id, file_path, file_name, title, artist, album, genre, year,
            track_number, disc_number, duration, file_size, format, bitrate, sample_rate, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        rusqlite::params![
            track.id,
            track.file_path,
            track.file_name,
            track.title,
            track.artist,
            track.album,
            track.genre,
            track.year,
            track.track_number,
            track.disc_number,
            track.duration,
            track.file_size,
            track.format,
            track.bitrate,
            track.sample_rate,
            track.created_at,
            track.updated_at,
        ],
    )
    .map_err(|e| format!("トラックの保存に失敗しました: {}", e))?;
    Ok(())
}

/// トラックを処理してトランザクション内で更新
fn process_and_update_track_in_tx(
    tx: &rusqlite::Transaction,
    file_path: &Path,
) -> Result<(), String> {
    let track = create_track_from_file(file_path)?;
    tx.execute(
        "UPDATE tracks SET
            file_name = ?2, title = ?3, artist = ?4, album = ?5, genre = ?6, year = ?7,
            track_number = ?8, disc_number = ?9, duration = ?10, file_size = ?11, format = ?12, bitrate = ?13, sample_rate = ?14,
            updated_at = ?15
        WHERE file_path = ?1",
        rusqlite::params![
            track.file_path,
            track.file_name,
            track.title,
            track.artist,
            track.album,
            track.genre,
            track.year,
            track.track_number,
            track.disc_number,
            track.duration,
            track.file_size,
            track.format,
            track.bitrate,
            track.sample_rate,
            track.updated_at,
        ],
    )
    .map_err(|e| format!("トラックの更新に失敗しました: {}", e))?;
    Ok(())
}

/// ファイルからトラック情報を作成
fn create_track_from_file(file_path: &Path) -> Result<Track, String> {
    let file_name = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "ファイル名の取得に失敗しました".to_string())?
        .to_string();

    let file_path_str = file_path
        .to_str()
        .ok_or_else(|| "ファイルパスの変換に失敗しました".to_string())?
        .to_string();

    // メタデータを抽出
    let metadata = extract_metadata(file_path)?;

    // タイトルがない場合はファイル名をデフォルトとして使用
    let title = metadata
        .title
        .or_else(|| Some(get_default_title(file_path)));

    // その他の情報を取得
    let duration = extract_duration(file_path)?;
    let bitrate = extract_bitrate(file_path)?;
    let sample_rate = extract_sample_rate(file_path)?;
    let file_size = get_file_size(file_path)?;
    let format = get_file_format(file_path);

    let now = Utc::now().to_rfc3339();

    Ok(Track {
        id: Uuid::new_v4().to_string(),
        file_path: file_path_str,
        file_name,
        title,
        artist: metadata.artist,
        album: metadata.album,
        genre: metadata.genre,
        year: metadata.year,
        track_number: metadata.track_number,
        disc_number: metadata.disc_number,
        duration,
        file_size,
        format,
        bitrate,
        sample_rate,
        is_favorite: false,
        rating: 0,
        play_count: 0,
        last_played_at: None,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// すべてのトラックを取得
#[tauri::command]
pub async fn get_all_tracks(state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                    created_at, updated_at
             FROM tracks
             ORDER BY created_at DESC",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                track_number: row.get(8)?,
                disc_number: row.get(9)?,
                duration: row.get(10)?,
                file_size: row.get(11)?,
                format: row.get(12)?,
                bitrate: row.get(13)?,
                sample_rate: row.get(14)?,
                is_favorite: row.get::<_, i32>(15)? != 0,
                rating: row.get(16)?,
                play_count: row.get(17)?,
                last_played_at: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(tracks)
}

/// トラックを検索（FTS5全文検索を使用した高速版）
#[tauri::command]
pub async fn search_tracks(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    // 検索クエリをサニタイズ
    let sanitized_query = sanitize_search_query(&query);

    if sanitized_query.is_empty() {
        return Ok(Vec::new());
    }

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // FTS5テーブルが存在するか確認
    let fts_exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tracks_fts'",
            [],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )
        .unwrap_or(false);

    if fts_exists {
        // FTS5を使用した高速検索
        let mut stmt = db
            .prepare(
                "SELECT t.id, t.file_path, t.file_name, t.title, t.artist, t.album, t.genre, t.year,
                        t.track_number, t.disc_number, t.duration, t.file_size, t.format, t.bitrate, t.sample_rate,
                        COALESCE(t.is_favorite, 0), COALESCE(t.rating, 0), COALESCE(t.play_count, 0), t.last_played_at,
                        t.created_at, t.updated_at
                 FROM tracks t
                 INNER JOIN tracks_fts fts ON t.rowid = fts.rowid
                 WHERE tracks_fts MATCH ?1
                 ORDER BY rank
                 LIMIT 1000",
            )
            .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

        let tracks = stmt
            .query_map([&sanitized_query], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    file_name: row.get(2)?,
                    title: row.get(3)?,
                    artist: row.get(4)?,
                    album: row.get(5)?,
                    genre: row.get(6)?,
                    year: row.get(7)?,
                    track_number: row.get(8)?,
                    disc_number: row.get(9)?,
                    duration: row.get(10)?,
                    file_size: row.get(11)?,
                    format: row.get(12)?,
                    bitrate: row.get(13)?,
                    sample_rate: row.get(14)?,
                    is_favorite: row.get(15)?,
                    rating: row.get(16)?,
                    play_count: row.get(17)?,
                    last_played_at: row.get(18)?,
                    created_at: row.get(19)?,
                    updated_at: row.get(20)?,
                })
            })
            .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

        Ok(tracks)
    } else {
        // フォールバック: 通常のLIKE検索
        let search_pattern = format!("%{}%", sanitized_query);

        let mut stmt = db
            .prepare(
                "SELECT id, file_path, file_name, title, artist, album, genre, year,
                        track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                        COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                        created_at, updated_at
                 FROM tracks
                 WHERE title LIKE ?1 OR artist LIKE ?1 OR album LIKE ?1 OR genre LIKE ?1
                 ORDER BY 
                    CASE 
                        WHEN title LIKE ?1 THEN 1
                        WHEN artist LIKE ?1 THEN 2
                        WHEN album LIKE ?1 THEN 3
                        ELSE 4
                    END,
                    created_at DESC
                 LIMIT 1000",
            )
            .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

        let tracks = stmt
            .query_map([&search_pattern], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    file_name: row.get(2)?,
                    title: row.get(3)?,
                    artist: row.get(4)?,
                    album: row.get(5)?,
                    genre: row.get(6)?,
                    year: row.get(7)?,
                    track_number: row.get(8)?,
                    disc_number: row.get(9)?,
                    duration: row.get(10)?,
                    file_size: row.get(11)?,
                    format: row.get(12)?,
                    bitrate: row.get(13)?,
                    sample_rate: row.get(14)?,
                    is_favorite: row.get(15)?,
                    rating: row.get(16)?,
                    play_count: row.get(17)?,
                    last_played_at: row.get(18)?,
                    created_at: row.get(19)?,
                    updated_at: row.get(20)?,
                })
            })
            .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

        Ok(tracks)
    }
}

/// フィルタオプション
#[derive(Debug, Deserialize)]
pub struct FilterOptions {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
}

/// トラックをフィルタリング
#[tauri::command]
pub async fn filter_tracks(
    filters: FilterOptions,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let mut query = String::from(
        "SELECT id, file_path, file_name, title, artist, album, genre, year,
                track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                created_at, updated_at
         FROM tracks WHERE 1=1",
    );

    let mut params: Vec<String> = Vec::new();

    if let Some(artist) = filters.artist {
        query.push_str(" AND artist = ?");
        params.push(artist);
    }

    if let Some(album) = filters.album {
        query.push_str(" AND album = ?");
        params.push(album);
    }

    if let Some(genre) = filters.genre {
        query.push_str(" AND genre = ?");
        params.push(genre);
    }

    query.push_str(" ORDER BY created_at DESC LIMIT 1000");

    let mut stmt = db
        .prepare(&query)
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let params_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let tracks = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                track_number: row.get(8)?,
                disc_number: row.get(9)?,
                duration: row.get(10)?,
                file_size: row.get(11)?,
                format: row.get(12)?,
                bitrate: row.get(13)?,
                sample_rate: row.get(14)?,
                is_favorite: row.get(15)?,
                rating: row.get(16)?,
                play_count: row.get(17)?,
                last_played_at: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(tracks)
}

/// ユニークなアーティスト一覧を取得
#[tauri::command]
pub async fn get_unique_artists(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let mut stmt = db
        .prepare("SELECT DISTINCT artist FROM tracks WHERE artist IS NOT NULL ORDER BY artist")
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let artists = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(artists)
}

/// ユニークなアルバム一覧を取得
#[tauri::command]
pub async fn get_unique_albums(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let mut stmt = db
        .prepare("SELECT DISTINCT album FROM tracks WHERE album IS NOT NULL ORDER BY album")
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let albums = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(albums)
}

/// ユニークなジャンル一覧を取得
#[tauri::command]
pub async fn get_unique_genres(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let mut stmt = db
        .prepare("SELECT DISTINCT genre FROM tracks WHERE genre IS NOT NULL ORDER BY genre")
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let genres = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(genres)
}

/// トラックのメタデータを更新（データベースのみ）
#[tauri::command]
pub async fn update_track_metadata(
    track_id: String,
    metadata: Metadata,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    // メタデータをバリデーション
    validate_metadata(&metadata)?;

    // メタデータフィールドの長さをバリデーション
    validate_string_length(&metadata.title, "タイトル", 255)?;
    validate_string_length(&metadata.artist, "アーティスト", 255)?;
    validate_string_length(&metadata.album, "アルバム", 255)?;
    validate_string_length(&metadata.genre, "ジャンル", 100)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // トラックの存在確認
    let mut stmt = db
        .prepare("SELECT id FROM tracks WHERE id = ?1")
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let exists = stmt
        .exists([&track_id])
        .map_err(|e| format!("トラックの確認に失敗しました: {}", e))?;

    if !exists {
        return Err("指定されたトラックが見つかりません".to_string());
    }

    // メタデータを更新
    let now = Utc::now().to_rfc3339();

    db.execute(
        "UPDATE tracks SET 
            title = ?1, 
            artist = ?2, 
            album = ?3, 
            genre = ?4, 
            year = ?5, 
            updated_at = ?6
         WHERE id = ?7",
        rusqlite::params![
            metadata.title,
            metadata.artist,
            metadata.album,
            metadata.genre,
            metadata.year,
            now,
            track_id,
        ],
    )
    .map_err(|e| format!("メタデータの更新に失敗しました: {}", e))?;

    Ok(())
}

/// トラックのメタデータを更新（ファイルとデータベース両方）
#[tauri::command]
pub async fn update_track_metadata_with_file(
    track_id: String,
    metadata: Metadata,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    // メタデータをバリデーション
    validate_metadata(&metadata)?;

    // メタデータフィールドの長さをバリデーション
    validate_string_length(&metadata.title, "タイトル", 255)?;
    validate_string_length(&metadata.artist, "アーティスト", 255)?;
    validate_string_length(&metadata.album, "アルバム", 255)?;
    validate_string_length(&metadata.genre, "ジャンル", 100)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // トラックのファイルパスを取得
    let mut stmt = db
        .prepare("SELECT file_path FROM tracks WHERE id = ?1")
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let file_path: String = stmt
        .query_row([&track_id], |row| row.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                "指定されたトラックが見つかりません".to_string()
            }
            _ => format!("トラックの取得に失敗しました: {}", e),
        })?;

    // ファイルのメタデータを更新
    let path = Path::new(&file_path);
    update_file_metadata(path, &metadata)?;

    // データベースのメタデータを更新
    let now = Utc::now().to_rfc3339();

    db.execute(
        "UPDATE tracks SET 
            title = ?1, 
            artist = ?2, 
            album = ?3, 
            genre = ?4, 
            year = ?5, 
            updated_at = ?6
         WHERE id = ?7",
        rusqlite::params![
            metadata.title,
            metadata.artist,
            metadata.album,
            metadata.genre,
            metadata.year,
            now,
            track_id,
        ],
    )
    .map_err(|e| format!("データベースの更新に失敗しました: {}", e))?;

    Ok(())
}

/// 複数トラックのメタデータを一括更新（データベースのみ）
#[tauri::command]
pub async fn update_multiple_tracks_metadata(
    track_ids: Vec<String>,
    metadata: Metadata,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if track_ids.is_empty() {
        return Err("トラックIDが指定されていません".to_string());
    }

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    // メタデータをバリデーション
    validate_metadata(&metadata)?;

    // メタデータフィールドの長さをバリデーション
    validate_string_length(&metadata.title, "タイトル", 255)?;
    validate_string_length(&metadata.artist, "アーティスト", 255)?;
    validate_string_length(&metadata.album, "アルバム", 255)?;
    validate_string_length(&metadata.genre, "ジャンル", 100)?;

    let mut db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let now = Utc::now().to_rfc3339();

    // トランザクションを開始
    let tx = db
        .transaction()
        .map_err(|e| format!("トランザクションの開始に失敗しました: {}", e))?;

    for track_id in track_ids {
        // トラックの存在確認
        let mut stmt = tx
            .prepare("SELECT id FROM tracks WHERE id = ?1")
            .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

        let exists = stmt
            .exists([&track_id])
            .map_err(|e| format!("トラックの確認に失敗しました: {}", e))?;

        if !exists {
            return Err(format!("トラックが見つかりません: {}", track_id));
        }

        // メタデータを更新（Noneでないフィールドのみ）
        let mut update_parts = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if metadata.title.is_some() {
            update_parts.push("title = ?");
            params.push(Box::new(metadata.title.clone()));
        }

        if metadata.artist.is_some() {
            update_parts.push("artist = ?");
            params.push(Box::new(metadata.artist.clone()));
        }

        if metadata.album.is_some() {
            update_parts.push("album = ?");
            params.push(Box::new(metadata.album.clone()));
        }

        if metadata.genre.is_some() {
            update_parts.push("genre = ?");
            params.push(Box::new(metadata.genre.clone()));
        }

        if metadata.year.is_some() {
            update_parts.push("year = ?");
            params.push(Box::new(metadata.year));
        }

        if update_parts.is_empty() {
            continue; // 更新するフィールドがない場合はスキップ
        }

        update_parts.push("updated_at = ?");
        params.push(Box::new(now.clone()));

        let query = format!("UPDATE tracks SET {} WHERE id = ?", update_parts.join(", "));

        params.push(Box::new(track_id.clone()));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        tx.execute(&query, params_refs.as_slice())
            .map_err(|e| format!("メタデータの更新に失敗しました: {}", e))?;
    }

    // トランザクションをコミット
    tx.commit()
        .map_err(|e| format!("トランザクションのコミットに失敗しました: {}", e))?;

    Ok(())
}

/// メタデータをバリデーション（フロントエンド用）
#[tauri::command]
pub async fn validate_metadata_command(metadata: Metadata) -> Result<(), String> {
    validate_metadata(&metadata)
}

// ========== プレイリスト管理コマンド ==========

/// プレイリストを作成
#[tauri::command]
pub async fn create_playlist(
    name: String,
    state: State<'_, AppState>,
) -> Result<crate::models::Playlist, String> {
    // プレイリスト名をバリデーション
    validate_playlist_name(&name)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::playlist::create_playlist(&db, &name)
        .map_err(|e| format!("プレイリストの作成に失敗しました: {}", e))
}

/// すべてのプレイリストを取得
#[tauri::command]
pub async fn get_playlists(
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::Playlist>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::playlist::get_all_playlists(&db)
        .map_err(|e| format!("プレイリストの取得に失敗しました: {}", e))
}

/// プレイリストにトラックを追加
#[tauri::command]
pub async fn add_track_to_playlist(
    playlist_id: String,
    track_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // IDをバリデーション
    validate_playlist_id(&playlist_id)?;
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::playlist::add_track_to_playlist(&db, &playlist_id, &track_id).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            "プレイリストまたはトラックが見つかりません".to_string()
        }
        _ => format!("トラックの追加に失敗しました: {}", e),
    })
}

/// プレイリストからトラックを削除
#[tauri::command]
pub async fn remove_track_from_playlist(
    playlist_id: String,
    track_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // IDをバリデーション
    validate_playlist_id(&playlist_id)?;
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::playlist::remove_track_from_playlist(&db, &playlist_id, &track_id).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            "プレイリストまたはトラックが見つかりません".to_string()
        }
        _ => format!("トラックの削除に失敗しました: {}", e),
    })
}

/// プレイリストの名前を変更
#[tauri::command]
pub async fn rename_playlist(
    playlist_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    // 名前をバリデーション
    let name = name.trim().to_string();
    crate::validation::validate_playlist_name(&name)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::playlist::rename_playlist(&db, &playlist_id, &name).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => "プレイリストが見つかりません".to_string(),
        _ => format!("プレイリスト名の変更に失敗しました: {}", e),
    })
}

/// プレイリストを削除
#[tauri::command]
pub async fn delete_playlist(
    playlist_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::playlist::delete_playlist(&db, &playlist_id).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => "プレイリストが見つかりません".to_string(),
        _ => format!("プレイリストの削除に失敗しました: {}", e),
    })
}

/// プレイリスト内のトラックを並び替え
#[tauri::command]
pub async fn reorder_playlist_tracks(
    playlist_id: String,
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::playlist::reorder_playlist_tracks(&db, &playlist_id, &track_ids).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => "プレイリストが見つかりません".to_string(),
        _ => format!("トラックの並び替えに失敗しました: {}", e),
    })
}

// ========== アルバムアートコマンド ==========

/// トラックのアルバムアートを取得
#[tauri::command]
pub async fn get_album_art(
    track_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AlbumArt>, String> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // トラックのファイルパスを取得
    let mut stmt = db
        .prepare("SELECT file_path FROM tracks WHERE id = ?1")
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let file_path: String = stmt
        .query_row([&track_id], |row| row.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                "指定されたトラックが見つかりません".to_string()
            }
            _ => format!("トラックの取得に失敗しました: {}", e),
        })?;

    // アルバムアートを抽出
    let path = Path::new(&file_path);
    extract_album_art(path)
}

// ========== 音楽再生コマンド ==========

/// トラックのファイルパスを取得
#[tauri::command]
pub async fn get_track_file_path(
    track_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let mut stmt = db
        .prepare("SELECT file_path FROM tracks WHERE id = ?1")
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let file_path: String = stmt
        .query_row([&track_id], |row| row.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                "指定されたトラックが見つかりません".to_string()
            }
            _ => format!("トラックの取得に失敗しました: {}", e),
        })?;

    Ok(file_path)
}

/// 現在再生中のトラックIDを設定
#[tauri::command]
pub async fn set_current_track(
    track_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut current_track = state
        .current_track_id
        .lock()
        .map_err(|e| format!("ステートロックの取得に失敗しました: {}", e))?;

    *current_track = track_id;

    Ok(())
}

/// 現在再生中のトラック情報を取得
#[tauri::command]
pub async fn get_current_track(state: State<'_, AppState>) -> Result<Option<Track>, String> {
    let current_track_id = state
        .current_track_id
        .lock()
        .map_err(|e| format!("ステートロックの取得に失敗しました: {}", e))?;

    if let Some(track_id) = current_track_id.as_ref() {
        let db = state
            .db
            .lock()
            .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

        let mut stmt = db
            .prepare(
                "SELECT id, file_path, file_name, title, artist, album, genre, year,
                        track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                        COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                        created_at, updated_at
                 FROM tracks WHERE id = ?1",
            )
            .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

        let track = stmt
            .query_row([track_id], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    file_name: row.get(2)?,
                    title: row.get(3)?,
                    artist: row.get(4)?,
                    album: row.get(5)?,
                    genre: row.get(6)?,
                    year: row.get(7)?,
                    track_number: row.get(8)?,
                    disc_number: row.get(9)?,
                    duration: row.get(10)?,
                    file_size: row.get(11)?,
                    format: row.get(12)?,
                    bitrate: row.get(13)?,
                    sample_rate: row.get(14)?,
                    is_favorite: row.get(15)?,
                    rating: row.get(16)?,
                    play_count: row.get(17)?,
                    last_played_at: row.get(18)?,
                    created_at: row.get(19)?,
                    updated_at: row.get(20)?,
                })
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    "指定されたトラックが見つかりません".to_string()
                }
                _ => format!("トラックの取得に失敗しました: {}", e),
            })?;

        Ok(Some(track))
    } else {
        Ok(None)
    }
}

// ========== お気に入り/レーティング/再生統計コマンド ==========

/// お気に入りを切り替え
#[tauri::command]
pub async fn toggle_favorite(track_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // 現在のお気に入り状態を取得
    let current: i32 = db
        .query_row(
            "SELECT COALESCE(is_favorite, 0) FROM tracks WHERE id = ?1",
            [&track_id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "トラックが見つかりません".to_string(),
            _ => format!("お気に入り状態の取得に失敗しました: {}", e),
        })?;

    let new_value = if current == 0 { 1 } else { 0 };

    db.execute(
        "UPDATE tracks SET is_favorite = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![new_value, track_id],
    )
    .map_err(|e| format!("お気に入りの更新に失敗しました: {}", e))?;

    Ok(new_value == 1)
}

/// レーティングを設定
#[tauri::command]
pub async fn set_rating(
    track_id: String,
    rating: i32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    validate_track_id(&track_id)?;

    if !(0..=5).contains(&rating) {
        return Err("レーティングは0から5の間で指定してください".to_string());
    }

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    db.execute(
        "UPDATE tracks SET rating = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![rating, track_id],
    )
    .map_err(|e| format!("レーティングの更新に失敗しました: {}", e))?;

    Ok(())
}

/// 再生回数をインクリメント
#[tauri::command]
pub async fn increment_play_count(
    track_id: String,
    state: State<'_, AppState>,
) -> Result<i32, String> {
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // 再生回数をインクリメントし、最終再生日時を更新
    db.execute(
        "UPDATE tracks SET 
            play_count = COALESCE(play_count, 0) + 1, 
            last_played_at = datetime('now'),
            updated_at = datetime('now')
         WHERE id = ?1",
        [&track_id],
    )
    .map_err(|e| format!("再生回数の更新に失敗しました: {}", e))?;

    // 再生履歴に追加
    db.execute(
        "INSERT INTO play_history (track_id, played_at) VALUES (?1, datetime('now'))",
        [&track_id],
    )
    .map_err(|e| format!("再生履歴の追加に失敗しました: {}", e))?;

    // 新しい再生回数を取得
    let new_count: i32 = db
        .query_row(
            "SELECT play_count FROM tracks WHERE id = ?1",
            [&track_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("再生回数の取得に失敗しました: {}", e))?;

    Ok(new_count)
}

/// お気に入りトラックを取得
#[tauri::command]
pub async fn get_favorite_tracks(state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                    created_at, updated_at
             FROM tracks
             WHERE is_favorite = 1
             ORDER BY updated_at DESC",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                track_number: row.get(8)?,
                disc_number: row.get(9)?,
                duration: row.get(10)?,
                file_size: row.get(11)?,
                format: row.get(12)?,
                bitrate: row.get(13)?,
                sample_rate: row.get(14)?,
                is_favorite: row.get::<_, i32>(15)? != 0,
                rating: row.get(16)?,
                play_count: row.get(17)?,
                last_played_at: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(tracks)
}

/// よく聴くトラック（再生回数順）を取得
#[tauri::command]
pub async fn get_most_played_tracks(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let limit = limit.unwrap_or(50);

    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                    created_at, updated_at
             FROM tracks
             WHERE play_count > 0
             ORDER BY play_count DESC, last_played_at DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([limit], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                track_number: row.get(8)?,
                disc_number: row.get(9)?,
                duration: row.get(10)?,
                file_size: row.get(11)?,
                format: row.get(12)?,
                bitrate: row.get(13)?,
                sample_rate: row.get(14)?,
                is_favorite: row.get::<_, i32>(15)? != 0,
                rating: row.get(16)?,
                play_count: row.get(17)?,
                last_played_at: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(tracks)
}

/// 最近再生したトラックを取得
#[tauri::command]
pub async fn get_recently_played_tracks(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let limit = limit.unwrap_or(50);

    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                    created_at, updated_at
             FROM tracks
             WHERE last_played_at IS NOT NULL
             ORDER BY last_played_at DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([limit], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                track_number: row.get(8)?,
                disc_number: row.get(9)?,
                duration: row.get(10)?,
                file_size: row.get(11)?,
                format: row.get(12)?,
                bitrate: row.get(13)?,
                sample_rate: row.get(14)?,
                is_favorite: row.get::<_, i32>(15)? != 0,
                rating: row.get(16)?,
                play_count: row.get(17)?,
                last_played_at: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(tracks)
}

/// ファイルの場所をシステムのファイルマネージャーで開く
#[tauri::command]
pub async fn show_in_folder(path: String) -> Result<(), String> {
    use std::process::Command;

    let file_path = Path::new(&path);

    if !file_path.exists() {
        return Err("ファイルが見つかりません".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", file_path.to_str().unwrap()])
            .spawn()
            .map_err(|e| format!("ファイルマネージャーを開けませんでした: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .args(["/select,", file_path.to_str().unwrap()])
            .spawn()
            .map_err(|e| format!("ファイルマネージャーを開けませんでした: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // 親ディレクトリを開く
        if let Some(parent) = file_path.parent() {
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("ファイルマネージャーを開けませんでした: {}", e))?;
        }
    }

    Ok(())
}

// ========== グループ化データ取得コマンド ==========

/// アルバムごとにグループ化されたトラックを取得
#[tauri::command]
pub async fn get_albums_grouped(state: State<'_, AppState>) -> Result<Vec<AlbumGroup>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // アルバムごとにトラックを取得
    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                    created_at, updated_at
             FROM tracks
             WHERE album IS NOT NULL AND album != ''
             ORDER BY album, COALESCE(disc_number, 1), COALESCE(track_number, 0), COALESCE(title, file_name)",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                track_number: row.get(8)?,
                disc_number: row.get(9)?,
                duration: row.get(10)?,
                file_size: row.get(11)?,
                format: row.get(12)?,
                bitrate: row.get(13)?,
                sample_rate: row.get(14)?,
                is_favorite: row.get::<_, i32>(15)? != 0,
                rating: row.get(16)?,
                play_count: row.get(17)?,
                last_played_at: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    // アルバムごとにグループ化
    use std::collections::HashMap;
    let mut album_map: HashMap<String, Vec<Track>> = HashMap::new();

    for track in tracks {
        if let Some(album) = &track.album {
            album_map.entry(album.clone()).or_default().push(track);
        }
    }

    // AlbumGroupに変換
    let mut album_groups: Vec<AlbumGroup> = album_map
        .into_iter()
        .map(|(name, tracks)| {
            let track_count = tracks.len() as i32;
            let total_duration = tracks.iter().filter_map(|t| t.duration).sum();
            let artist = tracks.first().and_then(|t| t.artist.clone());
            let representative_track_id = tracks.first().map(|t| t.id.clone()).unwrap_or_default();

            AlbumGroup {
                name,
                artist,
                track_count,
                total_duration,
                representative_track_id,
                tracks,
            }
        })
        .collect();

    // アルバム名でソート
    album_groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(album_groups)
}

/// アーティストごとにグループ化されたトラックを取得
#[tauri::command]
pub async fn get_artists_grouped(state: State<'_, AppState>) -> Result<Vec<ArtistGroup>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // アーティストごとにトラックを取得
    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                    created_at, updated_at
             FROM tracks
             WHERE artist IS NOT NULL AND artist != ''
             ORDER BY artist, album, COALESCE(disc_number, 1), COALESCE(track_number, 0), COALESCE(title, file_name)",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                track_number: row.get(8)?,
                disc_number: row.get(9)?,
                duration: row.get(10)?,
                file_size: row.get(11)?,
                format: row.get(12)?,
                bitrate: row.get(13)?,
                sample_rate: row.get(14)?,
                is_favorite: row.get::<_, i32>(15)? != 0,
                rating: row.get(16)?,
                play_count: row.get(17)?,
                last_played_at: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    // アーティスト -> アルバム -> トラックの階層構造を構築
    use std::collections::HashMap;
    let mut artist_map: HashMap<String, HashMap<String, Vec<Track>>> = HashMap::new();

    for track in tracks {
        if let Some(artist) = &track.artist {
            let album_name = track
                .album
                .clone()
                .unwrap_or_else(|| "不明なアルバム".to_string());
            artist_map
                .entry(artist.clone())
                .or_default()
                .entry(album_name)
                .or_default()
                .push(track);
        }
    }

    // ArtistGroupに変換
    let mut artist_groups: Vec<ArtistGroup> = artist_map
        .into_iter()
        .map(|(name, album_map)| {
            let albums: Vec<AlbumGroup> = album_map
                .into_iter()
                .map(|(album_name, tracks)| {
                    let track_count = tracks.len() as i32;
                    let total_duration = tracks.iter().filter_map(|t| t.duration).sum();
                    let representative_track_id =
                        tracks.first().map(|t| t.id.clone()).unwrap_or_default();

                    AlbumGroup {
                        name: album_name,
                        artist: Some(name.clone()),
                        track_count,
                        total_duration,
                        representative_track_id,
                        tracks,
                    }
                })
                .collect();

            let album_count = albums.len() as i32;
            let track_count: i32 = albums.iter().map(|a| a.track_count).sum();
            let total_duration: i32 = albums.iter().map(|a| a.total_duration).sum();
            let representative_track_id = albums
                .first()
                .map(|a| a.representative_track_id.clone())
                .unwrap_or_default();

            ArtistGroup {
                name,
                album_count,
                track_count,
                total_duration,
                representative_track_id,
                albums,
            }
        })
        .collect();

    // アーティスト名でソート
    artist_groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(artist_groups)
}

/// ジャンルごとにグループ化されたトラックを取得
#[tauri::command]
pub async fn get_genres_grouped(state: State<'_, AppState>) -> Result<Vec<GenreGroup>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // ジャンルごとにトラックを取得
    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
                    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
                    created_at, updated_at
             FROM tracks
             WHERE genre IS NOT NULL AND genre != ''
             ORDER BY genre, artist, album, COALESCE(disc_number, 1), COALESCE(track_number, 0), COALESCE(title, file_name)",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                track_number: row.get(8)?,
                disc_number: row.get(9)?,
                duration: row.get(10)?,
                file_size: row.get(11)?,
                format: row.get(12)?,
                bitrate: row.get(13)?,
                sample_rate: row.get(14)?,
                is_favorite: row.get::<_, i32>(15)? != 0,
                rating: row.get(16)?,
                play_count: row.get(17)?,
                last_played_at: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    // ジャンルごとにグループ化
    use std::collections::HashMap;
    let mut genre_map: HashMap<String, Vec<Track>> = HashMap::new();

    for track in tracks {
        if let Some(genre) = &track.genre {
            genre_map.entry(genre.clone()).or_default().push(track);
        }
    }

    // GenreGroupに変換
    let mut genre_groups: Vec<GenreGroup> = genre_map
        .into_iter()
        .map(|(name, tracks)| {
            let track_count = tracks.len() as i32;
            let total_duration = tracks.iter().filter_map(|t| t.duration).sum();
            let representative_track_id = tracks.first().map(|t| t.id.clone()).unwrap_or_default();

            GenreGroup {
                name,
                track_count,
                total_duration,
                representative_track_id,
                tracks,
            }
        })
        .collect();

    // ジャンル名でソート
    genre_groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(genre_groups)
}

// ========== トラック削除コマンド ==========

/// トラックをライブラリから削除（データベースのみ）
/// ファイルは削除せず、データベースからのみ削除
#[tauri::command]
pub async fn delete_tracks_command(
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    if track_ids.is_empty() {
        return Err("削除するトラックが指定されていません".to_string());
    }

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    delete_tracks(&db, &track_ids)
}

/// トラックをライブラリとファイルシステムから削除
/// データベースとファイル両方を削除
#[tauri::command]
pub async fn delete_tracks_with_files_command(
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<DeleteResult, String> {
    if track_ids.is_empty() {
        return Err("削除するトラックが指定されていません".to_string());
    }

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    delete_tracks_with_files(&db, &track_ids)
}

/// メタデータ更新の結果
#[derive(Debug, Clone, serde::Serialize)]
pub struct RefreshMetadataResult {
    pub updated_count: i32,
    pub skipped_count: i32,
    pub error_count: i32,
    pub errors: Vec<String>,
}

/// ライブラリ全体のメタデータを更新
/// ファイルからtrack_numberとdisc_numberを再読み込み
#[tauri::command]
pub async fn refresh_library_metadata(
    state: State<'_, AppState>,
) -> Result<RefreshMetadataResult, String> {
    let mut updated_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;
    let mut errors = Vec::new();

    let mut db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // 全トラックのファイルパスを取得
    let tracks: Vec<(String, String)> = {
        let mut stmt = db
            .prepare("SELECT id, file_path FROM tracks")
            .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

        let result: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;
        result
    };

    let total_tracks = tracks.len();
    crate::logger::info(&format!("メタデータ更新を開始: {} トラック", total_tracks));

    // バッチ処理で更新
    const BATCH_SIZE: usize = 50;
    for (batch_idx, chunk) in tracks.chunks(BATCH_SIZE).enumerate() {
        let tx = db
            .transaction()
            .map_err(|e| format!("トランザクションの開始に失敗しました: {}", e))?;

        for (track_id, file_path) in chunk {
            let path = Path::new(file_path);

            // ファイルが存在しない場合はスキップ
            if !path.exists() {
                skipped_count += 1;
                continue;
            }

            // メタデータを抽出
            match extract_metadata(path) {
                Ok(metadata) => {
                    // ログ: 抽出されたトラック番号とディスク番号
                    crate::logger::info(&format!(
                        "メタデータ抽出: {} - track={:?}, disc={:?}",
                        file_path,
                        metadata.track_number,
                        metadata.disc_number
                    ));

                    // track_numberとdisc_numberを更新
                    let now = Utc::now().to_rfc3339();
                    match tx.execute(
                        "UPDATE tracks SET track_number = ?1, disc_number = ?2, updated_at = ?3 WHERE id = ?4",
                        rusqlite::params![
                            metadata.track_number,
                            metadata.disc_number,
                            now,
                            track_id,
                        ],
                    ) {
                        Ok(_) => updated_count += 1,
                        Err(e) => {
                            errors.push(format!("{}: DB更新失敗 - {}", file_path, e));
                            error_count += 1;
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("{}: {}", file_path, e));
                    error_count += 1;
                }
            }
        }

        tx.commit()
            .map_err(|e| format!("トランザクションのコミットに失敗しました: {}", e))?;

        let processed = (batch_idx + 1) * BATCH_SIZE.min(total_tracks - batch_idx * BATCH_SIZE);
        crate::logger::info(&format!(
            "メタデータ更新進行状況: {}/{} トラック処理完了",
            processed,
            total_tracks
        ));
    }

    crate::logger::info(&format!(
        "メタデータ更新完了: 更新={}, スキップ={}, エラー={}",
        updated_count,
        skipped_count,
        error_count
    ));

    Ok(RefreshMetadataResult {
        updated_count,
        skipped_count,
        error_count,
        errors,
    })
}
