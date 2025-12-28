use crate::library::{
    get_default_title, get_file_format, get_file_size, is_duplicate_file, scan_directory,
    DuplicateAction, ImportResult,
};
use crate::metadata::{
    extract_bitrate, extract_duration, extract_metadata, extract_sample_rate, update_file_metadata,
    validate_metadata,
};
use crate::models::{Metadata, Track};
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
            duration, file_size, format, bitrate, sample_rate, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            track.id,
            track.file_path,
            track.file_name,
            track.title,
            track.artist,
            track.album,
            track.genre,
            track.year,
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
            duration = ?8, file_size = ?9, format = ?10, bitrate = ?11, sample_rate = ?12,
            updated_at = ?13
        WHERE file_path = ?1",
        rusqlite::params![
            track.file_path,
            track.file_name,
            track.title,
            track.artist,
            track.album,
            track.genre,
            track.year,
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
        duration,
        file_size,
        format,
        bitrate,
        sample_rate,
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
                    duration, file_size, format, bitrate, sample_rate, created_at, updated_at
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
                duration: row.get(8)?,
                file_size: row.get(9)?,
                format: row.get(10)?,
                bitrate: row.get(11)?,
                sample_rate: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
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
                        t.duration, t.file_size, t.format, t.bitrate, t.sample_rate, t.created_at, t.updated_at
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
                    duration: row.get(8)?,
                    file_size: row.get(9)?,
                    format: row.get(10)?,
                    bitrate: row.get(11)?,
                    sample_rate: row.get(12)?,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
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
                        duration, file_size, format, bitrate, sample_rate, created_at, updated_at
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
                    duration: row.get(8)?,
                    file_size: row.get(9)?,
                    format: row.get(10)?,
                    bitrate: row.get(11)?,
                    sample_rate: row.get(12)?,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
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
                duration, file_size, format, bitrate, sample_rate, created_at, updated_at
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
                duration: row.get(8)?,
                file_size: row.get(9)?,
                format: row.get(10)?,
                bitrate: row.get(11)?,
                sample_rate: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
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
                        duration, file_size, format, bitrate, sample_rate, created_at, updated_at
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
                    duration: row.get(8)?,
                    file_size: row.get(9)?,
                    format: row.get(10)?,
                    bitrate: row.get(11)?,
                    sample_rate: row.get(12)?,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
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
