//! メタデータ編集関連コマンド

use crate::metadata::{extract_metadata, update_file_metadata, validate_metadata};
use crate::models::Metadata;
use crate::state::AppState;
use crate::validation::{validate_string_length, validate_track_id};
use chrono::Utc;
use std::path::Path;
use tauri::State;

/// メタデータの内容と各フィールドの長さをまとめてバリデーション
fn validate_metadata_input(metadata: &Metadata) -> Result<(), String> {
    validate_metadata(metadata)?;
    validate_string_length(&metadata.title, "タイトル", 255)?;
    validate_string_length(&metadata.artist, "アーティスト", 255)?;
    validate_string_length(&metadata.album, "アルバム", 255)?;
    validate_string_length(&metadata.genre, "ジャンル", 100)?;
    validate_string_length(&metadata.album_artist, "アルバムアーティスト", 255)?;
    validate_string_length(&metadata.composer, "作曲者", 255)?;
    Ok(())
}

/// トラックのメタデータを更新（データベースのみ）
#[tauri::command]
pub async fn update_track_metadata(
    track_id: String,
    metadata: Metadata,
    state: State<'_, AppState>,
) -> Result<(), String> {
    validate_track_id(&track_id)?;
    validate_metadata_input(&metadata)?;

    state.with_db(|db| crate::repository::update_track_metadata(db, &track_id, &metadata))
}

/// トラックのメタデータを更新（ファイルとデータベース両方）
#[tauri::command]
pub async fn update_track_metadata_with_file(
    track_id: String,
    metadata: Metadata,
    state: State<'_, AppState>,
) -> Result<(), String> {
    validate_track_id(&track_id)?;
    validate_metadata_input(&metadata)?;

    state.with_db(|db| {
        // トラックのファイルパスを取得
        let file_path = crate::repository::find_file_path_by_track_id(db, &track_id)?;

        // ファイルのメタデータを更新
        update_file_metadata(Path::new(&file_path), &metadata)?;

        // データベースのメタデータを更新
        crate::repository::update_track_metadata(db, &track_id, &metadata)
    })
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

    validate_metadata_input(&metadata)?;

    state.with_db(|db| {
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

            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();

            tx.execute(&query, params_refs.as_slice())
                .map_err(|e| format!("メタデータの更新に失敗しました: {}", e))?;
        }

        // トランザクションをコミット
        tx.commit()
            .map_err(|e| format!("トランザクションのコミットに失敗しました: {}", e))?;

        Ok(())
    })
}

/// メタデータをバリデーション（フロントエンド用）
#[tauri::command]
pub async fn validate_metadata_command(metadata: Metadata) -> Result<(), String> {
    validate_metadata(&metadata)
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
    state.with_db(|db| {
        let mut updated_count = 0;
        let mut skipped_count = 0;
        let mut error_count = 0;
        let mut errors = Vec::new();

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
                            file_path, metadata.track_number, metadata.disc_number
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

            let processed = batch_idx * BATCH_SIZE + chunk.len();
            crate::logger::info(&format!(
                "メタデータ更新進行状況: {}/{} トラック処理完了",
                processed, total_tracks
            ));
        }

        crate::logger::info(&format!(
            "メタデータ更新完了: 更新={}, スキップ={}, エラー={}",
            updated_count, skipped_count, error_count
        ));

        Ok(RefreshMetadataResult {
            updated_count,
            skipped_count,
            error_count,
            errors,
        })
    })
}
