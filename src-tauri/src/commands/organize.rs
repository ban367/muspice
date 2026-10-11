//! ファイルの整理（タグに基づくリネーム・移動）のコマンド

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::events::LibraryChanged;
use crate::file_organize::{self, OrganizePlan, OrganizeStatus, Plan, PlannedMove, Template};
use crate::library::{get_file_modified_at, to_count};
use crate::library_folder::find_all_folders;
use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_track_id;
use serde::Serialize;
use specta::Type;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// 1回のトランザクションで書き換えるトラック数（インポートと同じ）
const BATCH_SIZE: usize = 50;

/// ファイルの整理の結果
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeResult {
    /// 移した曲の数
    pub moved_count: u32,
    /// 動かさなかった曲の数（ライブラリフォルダの外・見つからない・移動先がふさがっている）
    pub skipped_count: u32,
    /// 移せなかった曲の数
    pub failed_count: u32,
    /// 移せなかった理由と、歌詞ファイルを移せなかった理由（`場所: 理由`）
    pub errors: Vec<String>,
}

/// 入力を確かめて、書式を解釈する
fn validate_input(track_ids: &[String], template: &str) -> AppResult<Template> {
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "整理する曲が指定されていません".to_string(),
        ));
    }
    for track_id in track_ids {
        validate_track_id(track_id)?;
    }
    Template::parse(template)
}

/// 選んだ曲の、ファイルの整理の計画を立てる
fn build_plan(state: &AppState, track_ids: &[String], template: &Template) -> AppResult<Plan> {
    let (tracks, folders, registered_paths) = state.with_db(|db| {
        let mut by_id = crate::repository::find_tracks_by_ids(db, track_ids)?;
        // 選んだ順にする（同じIDの2つ目以降と、ライブラリにない曲は除く）
        let tracks: Vec<Track> = track_ids
            .iter()
            .filter_map(|track_id| by_id.remove(track_id))
            .collect();
        let folders: Vec<PathBuf> = find_all_folders(db)?
            .into_iter()
            .map(|folder| PathBuf::from(folder.path))
            .collect();
        let registered_paths = crate::repository::find_all_file_paths(db)?;
        Ok((tracks, folders, registered_paths))
    })?;

    // ファイルの確認はファイルシステムへのアクセスのため、DBロックの外で行う
    Ok(file_organize::plan(
        &tracks,
        &folders,
        template,
        &registered_paths,
    ))
}

/// 選んだ曲のファイルを、書式に従って整理する計画（今の場所 → 整理後の場所）を取得する
///
/// ファイルは動かさない。画面は、この一覧を見せてから`organize_files`を呼ぶ。
/// 書式の誤りは、入力の誤り（`VALIDATION`）として返す。
#[tauri::command]
#[specta::specta]
pub async fn plan_file_organize(
    track_ids: Vec<String>,
    template: String,
    app: AppHandle,
) -> AppResult<OrganizePlan> {
    let template = validate_input(&track_ids, &template)?;

    run_blocking(move || {
        let plan = build_plan(app.state::<AppState>().inner(), &track_ids, &template)?;
        Ok(OrganizePlan {
            entries: plan.entries,
        })
    })
    .await
}

/// 選んだ曲のファイルを、書式に従って、その曲のあるライブラリフォルダの中で移す
///
/// 計画は呼ばれた時点で立て直し、動かせる曲だけを移す（移動先がふさがっている曲などは動かさない）。
/// トラックのID・お気に入り・再生回数・プレイリストへの登録は、変えない。
/// 移せなかった曲は、結果の`errors`へ理由を入れて残りを続ける。
#[tauri::command]
#[specta::specta]
pub async fn organize_files(
    track_ids: Vec<String>,
    template: String,
    app: AppHandle,
) -> AppResult<OrganizeResult> {
    let template = validate_input(&track_ids, &template)?;

    run_blocking(move || {
        let state = app.state::<AppState>();
        let result = organize_blocking(state.inner(), &track_ids, &template)?;
        if result.moved_count > 0 {
            // フォルダの画像を使う曲は、フォルダが変わると画像も変わるため、アルバムアートを読み直させる
            if let Ok(mut cache) = state.album_art_cache.lock() {
                cache.clear();
            }
            if let Err(e) = LibraryChanged.emit(&app) {
                log::warn!("ライブラリの変更の通知に失敗しました: {}", e);
            }
        }
        Ok(result)
    })
    .await
}

/// `organize_files`の本体（同期処理）
fn organize_blocking(
    state: &AppState,
    track_ids: &[String],
    template: &Template,
) -> AppResult<OrganizeResult> {
    // 再スキャン（フォルダの監視による自動のものを含む）が、移している途中の曲を「見つからない曲」と
    // 「新しい曲」として扱わないよう、終わるまで待たせる
    let _scan = state.lock_library_scan();
    let plan = build_plan(state, track_ids, template)?;

    let skipped = plan
        .entries
        .iter()
        .filter(|entry| {
            !matches!(
                entry.status,
                OrganizeStatus::Move | OrganizeStatus::Unchanged
            )
        })
        .count();
    let mut result = OrganizeResult {
        skipped_count: to_count(skipped)?,
        ..OrganizeResult::default()
    };

    let mut moved: Vec<&PlannedMove> = Vec::new();
    for chunk in plan.moves.chunks(BATCH_SIZE) {
        // 1. ファイルを移す（DBロックの外）
        let mut on_disk: Vec<&PlannedMove> = Vec::new();
        for planned in chunk {
            match file_organize::move_files(planned) {
                Ok(warning) => {
                    if let Some(warning) = warning {
                        result
                            .errors
                            .push(format!("{}: {}", planned.to.display(), warning));
                    }
                    on_disk.push(planned);
                }
                Err(e) => fail(&mut result, planned, &e),
            }
        }
        if on_disk.is_empty() {
            continue;
        }

        // 2. 移した曲の場所を、まとめて書き換える。書き換えられなかった曲は、ファイルを元の場所へ戻す
        match record_moves(state, &on_disk) {
            Ok(outcomes) => {
                for (planned, outcome) in on_disk.into_iter().zip(outcomes) {
                    match outcome {
                        Ok(()) => moved.push(planned),
                        Err(e) => {
                            file_organize::undo_move(planned);
                            fail(&mut result, planned, &e);
                        }
                    }
                }
            }
            Err(e) => {
                for planned in on_disk.into_iter().rev() {
                    file_organize::undo_move(planned);
                    fail(&mut result, planned, &e);
                }
            }
        }
    }

    let moved: Vec<PlannedMove> = moved.into_iter().cloned().collect();
    file_organize::tidy_folders(&moved);
    result.moved_count = to_count(moved.len())?;

    log::info!(
        "ファイルを整理しました: 移動={}, 動かさない={}, 失敗={}",
        result.moved_count,
        result.skipped_count,
        result.failed_count
    );
    Ok(result)
}

/// 移せなかった曲を、結果に記録する
fn fail(result: &mut OrganizeResult, planned: &PlannedMove, error: &AppError) {
    result.failed_count += 1;
    result
        .errors
        .push(format!("{}: {}", planned.from.display(), error));
}

/// 移した曲の場所を、データベースに書き込む
///
/// @returns 曲ごとの結果（渡した順）。トランザクションごと失敗した場合は、エラー
fn record_moves(state: &AppState, moved: &[&PlannedMove]) -> AppResult<Vec<AppResult<()>>> {
    state.with_db(|db| {
        let tx = db.transaction().map_err(|e| {
            AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
        })?;
        let outcomes = moved
            .iter()
            .map(|planned| {
                let (Some(path), Some(name)) = (
                    planned.to.to_str(),
                    planned.to.file_name().and_then(|name| name.to_str()),
                ) else {
                    return Err(AppError::Io("移動先のパスを扱えません".to_string()));
                };
                crate::repository::relink_track(
                    &tx,
                    &planned.track_id,
                    path,
                    name,
                    get_file_modified_at(&planned.to),
                )
            })
            .collect();
        tx.commit().map_err(|e| {
            AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
        })?;
        Ok(outcomes)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;
    use crate::models::ReplayGain;
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;

    const TEMPLATE: &str = "%albumartist%/%album%/%track% %title%";

    fn test_state() -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        AppState::new(conn)
    }

    /// 曲のファイルを作り、ライブラリに登録する
    fn add_track(state: &AppState, path: &Path, title: &str, number: i32) -> String {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"data").unwrap();
        let track = Track {
            id: uuid::Uuid::new_v4().to_string(),
            file_path: path.to_str().unwrap().to_string(),
            file_name: path.file_name().unwrap().to_str().unwrap().to_string(),
            title: Some(title.to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            album_artist: None,
            genre: None,
            year: None,
            track_number: Some(number),
            disc_number: None,
            duration: Some(200),
            file_size: 4,
            format: "mp3".to_string(),
            bitrate: None,
            sample_rate: None,
            is_favorite: false,
            rating: 0,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            replay_gain: ReplayGain::default(),
            sort_tags: Default::default(),
            is_missing: false,
        };
        state
            .with_db(|db| crate::repository::insert_track(db, &track, Some(1)))
            .unwrap();
        track.id
    }

    fn register_library(state: &AppState, library: &Path) {
        fs::create_dir_all(library).unwrap();
        state
            .with_db(|db| crate::library_folder::register_folder(db, library.to_str().unwrap()))
            .unwrap();
    }

    fn find(state: &AppState, track_id: &str) -> Track {
        state
            .with_db(|db| crate::repository::find_track_by_id(db, track_id))
            .unwrap()
    }

    fn organize(state: &AppState, track_ids: &[String], template: &str) -> OrganizeResult {
        organize_blocking(state, track_ids, &Template::parse(template).unwrap()).unwrap()
    }

    #[test]
    fn test_validate_input() {
        let id = uuid::Uuid::new_v4().to_string();
        assert!(validate_input(std::slice::from_ref(&id), TEMPLATE).is_ok());
        assert!(matches!(
            validate_input(&[], TEMPLATE),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            validate_input(&["../x".to_string()], TEMPLATE),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            validate_input(&[id], "%unknown%"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn test_organize_moves_files_and_keeps_track_ids() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let state = test_state();
        register_library(&state, &library);
        let first = add_track(&state, &library.join("incoming/a.mp3"), "First", 1);
        let second = add_track(&state, &library.join("incoming/b.mp3"), "Second", 2);
        state
            .with_db(|db| {
                crate::repository::set_tracks_favorite(db, std::slice::from_ref(&first), true)
            })
            .unwrap();

        let ids = [first.clone(), second.clone()];
        let plan = build_plan(&state, &ids, &Template::parse(TEMPLATE).unwrap()).unwrap();
        assert_eq!(plan.moves.len(), 2);
        // 計画を立てただけでは、動かさない
        assert!(library.join("incoming/a.mp3").is_file());

        let result = organize(&state, &ids, TEMPLATE);

        assert_eq!(
            result,
            OrganizeResult {
                moved_count: 2,
                ..OrganizeResult::default()
            }
        );
        let moved_to = library.join("Artist/Album/01 First.mp3");
        assert!(moved_to.is_file());
        assert!(library.join("Artist/Album/02 Second.mp3").is_file());
        assert!(!library.join("incoming").exists());
        // トラックのIDとお気に入りはそのままで、場所だけが変わる
        let track = find(&state, &first);
        assert_eq!(track.file_path, moved_to.to_str().unwrap());
        assert_eq!(track.file_name, "01 First.mp3");
        assert!(track.is_favorite);
        assert!(!track.is_missing);

        // もう一度行っても、動かすものはない
        assert_eq!(organize(&state, &ids, TEMPLATE), OrganizeResult::default());
    }

    #[test]
    fn test_organize_reports_skipped_tracks() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let state = test_state();
        register_library(&state, &library);
        let blocked = add_track(&state, &library.join("incoming/a.mp3"), "First", 1);
        let outside = add_track(&state, &dir.path().join("Other/b.mp3"), "Second", 2);
        // 移動先に、ライブラリにないファイルがある
        let existing = library.join("Artist/Album/01 First.mp3");
        fs::create_dir_all(existing.parent().unwrap()).unwrap();
        fs::write(&existing, b"other").unwrap();

        let result = organize(&state, &[blocked.clone(), outside.clone()], TEMPLATE);

        assert_eq!(
            result,
            OrganizeResult {
                skipped_count: 2,
                ..OrganizeResult::default()
            }
        );
        assert_eq!(fs::read(&existing).unwrap(), b"other");
        assert_eq!(
            find(&state, &blocked).file_path,
            library.join("incoming/a.mp3").to_str().unwrap()
        );
        assert!(dir.path().join("Other/b.mp3").is_file());
    }

    #[test]
    fn test_organize_restores_files_when_the_library_cannot_be_updated() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let state = test_state();
        register_library(&state, &library);
        let source = library.join("incoming/a.mp3");
        let track_id = add_track(&state, &source, "First", 1);
        let plan = build_plan(
            &state,
            std::slice::from_ref(&track_id),
            &Template::parse(TEMPLATE).unwrap(),
        )
        .unwrap();
        let planned = &plan.moves[0];
        file_organize::move_files(planned).unwrap();
        // ファイルを移した後に、曲がライブラリから外された
        state
            .with_db(|db| crate::library::delete_tracks(db, std::slice::from_ref(&track_id)))
            .unwrap();

        let outcomes = record_moves(&state, &[planned]).unwrap();

        assert!(matches!(outcomes[0], Err(AppError::NotFound(_))));
        file_organize::undo_move(planned);
        assert!(source.is_file());
    }
}
