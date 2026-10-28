//! ファイルの整理（タグに基づくリネーム・移動）と、音楽フォルダへ集めるコマンド

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::events::{LibraryChanged, OrganizeProgress};
use crate::file_organize::{
    self, CopiedMove, OrganizePlan, OrganizeStatus, Plan, PlannedMove, Template,
};
use crate::library::{get_file_modified_at, to_count};
use crate::library_folder::{find_all_folders, find_music_folder};
use crate::models::Track;
use crate::settings::SettingsState;
use crate::state::AppState;
use crate::validation::validate_track_id;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 1回のトランザクションで書き換えるトラック数（インポートと同じ）
const BATCH_SIZE: usize = 50;

/// 移した後に残す、音楽フォルダのドライブの空き容量（バイト。取り込みのコピーと同じ）
const SPACE_MARGIN_BYTES: u64 = 64 * 1024 * 1024;

/// ボリュームをまたいで移す時の、コピーの読み書きの単位
const COPY_BUFFER_SIZE: usize = 1024 * 1024;

/// 整理の移動先
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OrganizeDestination {
    /// 曲のあるライブラリフォルダの中（書式は、画面で選んだもの）
    LibraryFolder,
    /// 音楽フォルダへ集める（書式は、設定の音楽フォルダの書式。画面の書式は使わない）
    MusicFolder,
}

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
    /// 移せなかった理由と、歌詞ファイルを移せなかった・元のファイルを消せなかった理由（`場所: 理由`）
    pub errors: Vec<String>,
    /// 途中で中止したか（中止するまでに移した曲は、移したまま）
    pub cancelled: bool,
}

/// 整理の実行の状態（Tauriの状態として管理する。同時に実行できる整理は1つ）
#[derive(Default)]
pub struct OrganizeState {
    running: AtomicBool,
    cancel: AtomicBool,
}

impl OrganizeState {
    /// 整理を始める。ほかの整理が実行中ならNone（返したガードを破棄すると、実行中の印が外れる）
    fn begin(&self) -> Option<OrganizeGuard<'_>> {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        self.cancel.store(false, Ordering::SeqCst);
        Some(OrganizeGuard { state: self })
    }
}

struct OrganizeGuard<'a> {
    state: &'a OrganizeState,
}

impl Drop for OrganizeGuard<'_> {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::SeqCst);
    }
}

/// 移す先（書式と、音楽フォルダへ集める時は音楽フォルダ）
struct Destination {
    template: Template,
    music_folder: Option<PathBuf>,
}

/// 入力を確かめて、移す先を決める
///
/// 音楽フォルダへ集める時は、設定の音楽フォルダの書式を使う。音楽フォルダが未設定・見つからない場合は
/// `NOT_FOUND`。
fn resolve_destination(
    app: &AppHandle,
    track_ids: &[String],
    template: &str,
    destination: OrganizeDestination,
) -> AppResult<Destination> {
    validate_track_ids(track_ids)?;
    match destination {
        OrganizeDestination::LibraryFolder => Ok(Destination {
            template: Template::parse(template)?,
            music_folder: None,
        }),
        OrganizeDestination::MusicFolder => {
            let folder = app
                .state::<AppState>()
                .with_db(|db| find_music_folder(db))?
                .ok_or_else(|| {
                    AppError::NotFound("音楽フォルダが設定されていません".to_string())
                })?;
            let folder = PathBuf::from(folder.path);
            if !folder.is_dir() {
                return Err(AppError::NotFound(format!(
                    "音楽フォルダが見つかりません: {}",
                    folder.display()
                )));
            }
            let settings = app.state::<SettingsState>().get()?;
            Ok(Destination {
                template: Template::parse(&settings.music_folder_template)?,
                music_folder: Some(folder),
            })
        }
    }
}

/// 整理する曲のIDを確かめる
fn validate_track_ids(track_ids: &[String]) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "整理する曲が指定されていません".to_string(),
        ));
    }
    track_ids
        .iter()
        .try_for_each(|track_id| validate_track_id(track_id))
}

/// 選んだ曲の、ファイルの整理の計画を立てる
fn build_plan(
    state: &AppState,
    track_ids: &[String],
    destination: &Destination,
) -> AppResult<Plan> {
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
        &destination.template,
        &registered_paths,
        destination.music_folder.as_deref(),
    ))
}

/// ボリュームをまたいで移す曲の数と、要る容量（バイト）
fn copy_requirements(plan: &Plan) -> (usize, u64) {
    plan.moves
        .iter()
        .filter(|planned| planned.copy)
        .fold((0, 0), |(count, bytes), planned| {
            (count + 1, bytes + planned.size())
        })
}

/// バイト数を、画面に渡す数にする
fn to_bytes(bytes: u64) -> i64 {
    i64::try_from(bytes).unwrap_or(i64::MAX)
}

/// 選んだ曲のファイルを、書式に従って整理する計画（今の場所 → 整理後の場所）を取得する
///
/// ファイルは動かさない。画面は、この一覧を見せてから`organize_files`を呼ぶ。
/// 書式の誤りは、入力の誤り（`VALIDATION`）として返す。`musicFolder`では`template`を使わず、設定の
/// 音楽フォルダの書式を使う（音楽フォルダが未設定・見つからない場合は`NOT_FOUND`）。
#[tauri::command]
#[specta::specta]
pub async fn plan_file_organize(
    track_ids: Vec<String>,
    template: String,
    destination: OrganizeDestination,
    app: AppHandle,
) -> AppResult<OrganizePlan> {
    run_blocking(move || {
        let destination = resolve_destination(&app, &track_ids, &template, destination)?;
        let plan = build_plan(app.state::<AppState>().inner(), &track_ids, &destination)?;
        let (copy_count, copy_bytes) = copy_requirements(&plan);
        Ok(OrganizePlan {
            entries: plan.entries,
            copy_count: to_count(copy_count)?,
            copy_bytes: to_bytes(copy_bytes),
            free_bytes: destination
                .music_folder
                .as_deref()
                .and_then(|folder| fs4::available_space(folder).ok())
                .map(to_bytes),
        })
    })
    .await
}

/// 選んだ曲のファイルを、書式に従って、その曲のあるライブラリフォルダの中で移すか、音楽フォルダへ集める
///
/// 計画は呼ばれた時点で立て直し、動かせる曲だけを移す（移動先がふさがっている曲などは動かさない）。
/// トラックのID・お気に入り・再生回数・プレイリストへの登録は、変えない。
/// 移せなかった曲は、結果の`errors`へ理由を入れて残りを続ける。曲ごとに`OrganizeProgress`を送り、
/// `cancel_file_organize`で、次の曲の前で中止する。ほかの整理が実行中なら`LOCK`。
#[tauri::command]
#[specta::specta]
pub async fn organize_files(
    track_ids: Vec<String>,
    template: String,
    destination: OrganizeDestination,
    app: AppHandle,
) -> AppResult<OrganizeResult> {
    run_blocking(move || {
        let destination = resolve_destination(&app, &track_ids, &template, destination)?;
        let organize = app.state::<OrganizeState>();
        let _running = organize
            .begin()
            .ok_or_else(|| AppError::Lock("ファイルの整理を実行中です".to_string()))?;
        let state = app.state::<AppState>();
        let result = organize_blocking(
            state.inner(),
            &track_ids,
            &destination,
            &Execution {
                cancel: &organize.cancel,
                on_progress: &|progress| {
                    if let Err(e) = progress.emit(&app) {
                        log::warn!("Failed to emit organize progress event: {}", e);
                    }
                },
                discard: &crate::library::discard_track_file,
            },
        )?;
        if result.moved_count > 0 {
            // フォルダの画像を使う曲は、フォルダが変わると画像も変わるため、アルバムアートを読み直させる
            if let Ok(mut cache) = state.album_art_cache.lock() {
                cache.clear();
            }
            if let Err(e) = LibraryChanged.emit(&app) {
                log::warn!("Failed to emit library change event: {}", e);
            }
        }
        Ok(result)
    })
    .await
}

/// 実行中の整理を中止する（実行中でなければ何もしない）
///
/// 移している途中の曲は、移し終えてから止まる。
#[tauri::command]
#[specta::specta]
pub async fn cancel_file_organize(organize: State<'_, OrganizeState>) -> AppResult<()> {
    if organize.running.load(Ordering::SeqCst) {
        organize.cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// タグを保存した後に、音楽フォルダの曲を移し直した結果（ADR-071）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeepOrganizedResult {
    /// 書式どおりの場所へ移した曲の数
    pub moved_count: u32,
    /// 移さなかった曲の数（移動先に同じ名前のファイルがある・選んだ曲の中で同じ移動先になる・移せなかった）
    pub not_moved_count: u32,
    /// 移せなかった理由（`場所: 理由`）
    pub errors: Vec<String>,
}

/// タグを保存した曲のうち、音楽フォルダの中の曲を、音楽フォルダの書式どおりの場所へ移し直す
/// （同期処理。設定「音楽フォルダを整理された状態に保つ」。ADR-071）
///
/// 設定がオフ・音楽フォルダが未設定か見つからない・音楽フォルダの中の曲がない場合は、何もしない（None）。
/// 移すのはファイルの整理（音楽フォルダへ集める）と同じ処理で、名前が衝突する曲は動かさない。
/// タグの保存は済んでいるため、移せなかった場合もエラーにせず、結果で知らせる。1曲でも移すと
/// `LibraryChanged`を送る。
pub(super) fn keep_music_folder_organized(
    app: &AppHandle,
    track_ids: &[String],
) -> Option<KeepOrganizedResult> {
    let settings = match app.state::<SettingsState>().get() {
        Ok(settings) if settings.keep_music_folder_organized => settings,
        Ok(_) => return None,
        Err(e) => {
            log::error!("Failed to read music folder organize settings: {}", e);
            return None;
        }
    };
    let state = app.state::<AppState>();
    let music_folder = match state.with_db(|db| find_music_folder(db)) {
        Ok(Some(folder)) => PathBuf::from(folder.path),
        Ok(None) => return None,
        Err(e) => {
            log::error!("Failed to read music folder: {}", e);
            return None;
        }
    };
    if !music_folder.is_dir() {
        return None;
    }
    let template = match Template::parse(&settings.music_folder_template) {
        Ok(template) => template,
        Err(e) => {
            log::error!("Failed to parse music folder template: {}", e);
            return None;
        }
    };

    let result = organize_saved_tracks(state.inner(), track_ids, music_folder, template);
    if let Some(KeepOrganizedResult { moved_count, .. }) = &result
        && *moved_count > 0
    {
        // フォルダの画像を使う曲は、フォルダが変わると画像も変わるため、アルバムアートを読み直させる
        if let Ok(mut cache) = state.album_art_cache.lock() {
            cache.clear();
        }
        if let Err(e) = LibraryChanged.emit(app) {
            log::warn!("Failed to emit library change event: {}", e);
        }
    }
    result
}

/// `keep_music_folder_organized`の本体: 音楽フォルダの中の曲だけを、音楽フォルダの書式どおりの場所へ移す
fn organize_saved_tracks(
    state: &AppState,
    track_ids: &[String],
    music_folder: PathBuf,
    template: Template,
) -> Option<KeepOrganizedResult> {
    // 音楽フォルダの外の曲は、動かさない（集めるのは、利用者がファイルの整理で行う）
    let paths = match state.with_db(|db| crate::repository::find_tracks_by_ids(db, track_ids)) {
        Ok(tracks) => tracks,
        Err(e) => {
            log::error!("Failed to read tracks to reorganize: {}", e);
            return None;
        }
    };
    let inside: Vec<String> = track_ids
        .iter()
        .filter(|track_id| {
            paths.get(*track_id).is_some_and(|track| {
                let path = Path::new(&track.file_path);
                path.starts_with(&music_folder) && path != music_folder
            })
        })
        .cloned()
        .collect();
    if inside.is_empty() {
        return None;
    }

    let destination = Destination {
        template,
        music_folder: Some(music_folder),
    };
    let organized = organize_blocking(
        state,
        &inside,
        &destination,
        &Execution {
            cancel: &AtomicBool::new(false),
            on_progress: &|_| {},
            discard: &crate::library::discard_track_file,
        },
    );
    Some(match organized {
        Ok(result) => KeepOrganizedResult {
            moved_count: result.moved_count,
            not_moved_count: result.skipped_count + result.failed_count,
            errors: result.errors,
        },
        Err(e) => {
            log::error!("Failed to reorganize tracks in the music folder: {}", e);
            KeepOrganizedResult {
                moved_count: 0,
                not_moved_count: to_count(inside.len()).unwrap_or(u32::MAX),
                errors: vec![e.to_string()],
            }
        }
    })
}

/// 実行の進め方（中止・進み具合の通知・元のファイルの消し方）
struct Execution<'a> {
    /// trueになったら、次の曲の前で中止する
    cancel: &'a AtomicBool,
    on_progress: &'a dyn Fn(OrganizeProgress),
    /// ボリュームをまたいで移した曲の、元のファイルを消す（普段は`library::discard_track_file`）
    discard: &'a file_organize::Discard<'a>,
}

/// 移した（コピーした）曲（コピーした場合は、その結果）
type OnDisk<'a> = (&'a PlannedMove, Option<CopiedMove>);

/// `organize_files`の本体（同期処理）
fn organize_blocking(
    state: &AppState,
    track_ids: &[String],
    destination: &Destination,
    execution: &Execution,
) -> AppResult<OrganizeResult> {
    // 再スキャン（フォルダの監視による自動のものを含む）が、移している途中の曲を「見つからない曲」と
    // 「新しい曲」として扱わないよう、終わるまで待たせる
    let _scan = state.lock_library_scan();
    let plan = build_plan(state, track_ids, destination)?;
    execute_plan(state, &plan, destination.music_folder.as_deref(), execution)
}

/// 計画どおりに移す（同期処理。呼び出し側が`lock_library_scan`を持つ）
fn execute_plan(
    state: &AppState,
    plan: &Plan,
    music_folder: Option<&Path>,
    execution: &Execution,
) -> AppResult<OrganizeResult> {
    // ボリュームをまたぐ曲は、コピーする前に、空き容量を確かめる（足りなければ、何も移さない）
    let (_, required_bytes) = copy_requirements(plan);
    if let Some(music_folder) = music_folder
        && required_bytes > 0
    {
        let free_bytes = fs4::available_space(music_folder).map_err(|e| {
            AppError::Io(format!(
                "音楽フォルダのドライブの空き容量を取得できません: {}",
                e
            ))
        })?;
        if required_bytes.saturating_add(SPACE_MARGIN_BYTES) > free_bytes {
            return Err(AppError::Validation(format!(
                "音楽フォルダのドライブの空き容量が足りません（必要: {}バイト、空き: {}バイト）",
                required_bytes, free_bytes
            )));
        }
    }

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

    let total = to_count(plan.moves.len())?;
    let mut processed: u32 = 0;
    let mut buffer = vec![0u8; COPY_BUFFER_SIZE];
    let mut moved: Vec<&PlannedMove> = Vec::new();
    for chunk in plan.moves.chunks(BATCH_SIZE) {
        // 1. ファイルを移す・コピーする（DBロックの外）
        let mut on_disk: Vec<OnDisk> = Vec::new();
        for planned in chunk {
            if execution.cancel.load(Ordering::SeqCst) {
                result.cancelled = true;
                break;
            }
            processed += 1;
            (execution.on_progress)(OrganizeProgress {
                current: processed,
                total,
                current_file: planned
                    .from
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            });
            // 同じ曲のタグを書き換えている処理が終わるまで待つ（書いている途中のファイルを移さない）
            let file = state.lock_track_file(&planned.track_id);
            let outcome = if planned.copy {
                file_organize::copy_for_move(planned, &mut buffer)
                    .map(|copied| (copied.warning.clone(), Some(copied)))
            } else {
                file_organize::move_files(planned).map(|warning| (warning, None))
            };
            drop(file);
            match outcome {
                Ok((warning, copied)) => {
                    if let Some(warning) = warning {
                        result
                            .errors
                            .push(format!("{}: {}", planned.to.display(), warning));
                    }
                    on_disk.push((planned, copied));
                }
                Err(e) => fail(&mut result, planned, &e),
            }
        }

        // 2. 移した曲の場所を、まとめて書き換える。書き換えられなかった曲は、移す前に戻す
        //    （移した曲は元の場所へ戻し、コピーした曲はコピーを消す）
        if !on_disk.is_empty() {
            let undo = |planned: &PlannedMove, copied: &Option<CopiedMove>| match copied {
                Some(copied) => file_organize::remove_copied(planned, copied),
                None => file_organize::undo_move(planned),
            };
            let paths: Vec<&PlannedMove> = on_disk.iter().map(|(planned, _)| *planned).collect();
            match record_moves(state, &paths) {
                Ok(outcomes) => {
                    for ((planned, copied), outcome) in on_disk.into_iter().zip(outcomes) {
                        match outcome {
                            Ok(()) => {
                                // 3. コピーした曲は、場所を書き換えてから元を消す
                                if let Some(copied) = &copied
                                    && let Some(warning) = file_organize::discard_originals(
                                        planned,
                                        copied,
                                        execution.discard,
                                    )
                                {
                                    result.errors.push(format!(
                                        "{}: {}",
                                        planned.from.display(),
                                        warning
                                    ));
                                }
                                moved.push(planned);
                            }
                            Err(e) => {
                                undo(planned, &copied);
                                fail(&mut result, planned, &e);
                            }
                        }
                    }
                }
                Err(e) => {
                    for (planned, copied) in on_disk.into_iter().rev() {
                        undo(planned, &copied);
                        fail(&mut result, planned, &e);
                    }
                }
            }
        }
        if result.cancelled {
            break;
        }
    }

    let moved: Vec<PlannedMove> = moved.into_iter().cloned().collect();
    file_organize::tidy_folders(&moved, execution.discard);
    result.moved_count = to_count(moved.len())?;

    log::info!(
        "Organized files: moved={}, skipped={}, failed={}, cancelled={}",
        result.moved_count,
        result.skipped_count,
        result.failed_count,
        result.cancelled
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
    use std::cell::RefCell;
    use std::fs;

    const TEMPLATE: &str = "%albumartist%/%album%/%track% %title%";

    fn test_state() -> AppState {
        let conn = crate::db::open_in_memory();
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
            .with_db(|db| {
                crate::library_folder::register_folder(db, library.to_str().unwrap(), &[])
            })
            .unwrap();
    }

    fn find(state: &AppState, track_id: &str) -> Track {
        state
            .with_db(|db| crate::repository::find_track_by_id(db, track_id))
            .unwrap()
    }

    /// 曲のあるライブラリフォルダの中で整理する
    fn library_destination(template: &str) -> Destination {
        Destination {
            template: Template::parse(template).unwrap(),
            music_folder: None,
        }
    }

    /// 音楽フォルダへ集める（ライブラリフォルダとして登録する）
    fn music_destination(state: &AppState, music: &Path) -> Destination {
        register_library(state, music);
        Destination {
            template: Template::parse(TEMPLATE).unwrap(),
            music_folder: Some(music.to_path_buf()),
        }
    }

    /// テストの`discard`（ゴミ箱を使わず、ファイルを消す）
    fn remove_file(path: &Path) -> AppResult<Option<PathBuf>> {
        fs::remove_file(path)
            .map(|()| None)
            .map_err(|e| AppError::Io(e.to_string()))
    }

    /// 中止しない・進み具合を捨てる・元を消す実行
    fn run(state: &AppState, track_ids: &[String], destination: &Destination) -> OrganizeResult {
        organize_blocking(
            state,
            track_ids,
            destination,
            &Execution {
                cancel: &AtomicBool::new(false),
                on_progress: &|_| {},
                discard: &remove_file,
            },
        )
        .unwrap()
    }

    fn organize(state: &AppState, track_ids: &[String], template: &str) -> OrganizeResult {
        run(state, track_ids, &library_destination(template))
    }

    #[test]
    fn test_validate_track_ids() {
        let id = uuid::Uuid::new_v4().to_string();
        assert!(validate_track_ids(std::slice::from_ref(&id)).is_ok());
        assert!(matches!(
            validate_track_ids(&[]),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            validate_track_ids(&["../x".to_string()]),
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
        let plan = build_plan(&state, &ids, &library_destination(TEMPLATE)).unwrap();
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
            &library_destination(TEMPLATE),
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

    /// 音楽フォルダへ集めると、ほかのライブラリフォルダ・ライブラリフォルダの外の曲も移す
    /// （IDとお気に入りは保ち、空になったフォルダは消すが、ライブラリフォルダの登録は残す）
    #[test]
    fn test_collecting_moves_tracks_into_the_music_folder() {
        let dir = TempDir::new();
        let old = dir.path().join("Old Library");
        let state = test_state();
        register_library(&state, &old);
        let destination = music_destination(&state, &dir.path().join("Music"));
        let music = destination.music_folder.clone().unwrap();
        let first = add_track(&state, &old.join("rip/a.mp3"), "First", 1);
        let second = add_track(&state, &dir.path().join("Downloads/b.mp3"), "Second", 2);
        state
            .with_db(|db| {
                crate::repository::set_tracks_favorite(db, std::slice::from_ref(&first), true)
            })
            .unwrap();

        let result = run(&state, &[first.clone(), second.clone()], &destination);

        assert_eq!(
            result,
            OrganizeResult {
                moved_count: 2,
                ..OrganizeResult::default()
            }
        );
        let track = find(&state, &first);
        assert_eq!(
            Path::new(&track.file_path),
            music.join("Artist/Album/01 First.mp3")
        );
        assert!(track.is_favorite);
        assert!(music.join("Artist/Album/02 Second.mp3").is_file());
        // ライブラリフォルダの空になったフォルダは消し、フォルダ自体と登録は残す
        assert!(!old.join("rip").exists());
        assert!(old.is_dir());
        let folders = state
            .with_db(|db| crate::library_folder::find_all_folders(db))
            .unwrap();
        assert_eq!(folders.len(), 2);
        // ライブラリフォルダの外の、元のフォルダは消さない
        assert!(dir.path().join("Downloads").is_dir());
    }

    /// ボリュームをまたぐ曲は、コピーして場所を書き換えてから、元を消す
    #[test]
    fn test_collecting_across_volumes_copies_then_discards_the_originals() {
        let dir = TempDir::new();
        let state = test_state();
        let destination = music_destination(&state, &dir.path().join("Music"));
        let music = destination.music_folder.clone().unwrap();
        let source = dir.path().join("External/rip/a.mp3");
        let track_id = add_track(&state, &source, "First", 1);
        let mut plan = build_plan(&state, std::slice::from_ref(&track_id), &destination).unwrap();
        // ボリュームをまたぐ場合の処理を、同じボリュームで確かめる
        plan.moves[0].copy = true;
        let discarded = RefCell::new(Vec::new());
        let progress = RefCell::new(Vec::new());

        let result = execute_plan(
            &state,
            &plan,
            Some(&music),
            &Execution {
                cancel: &AtomicBool::new(false),
                on_progress: &|event| {
                    progress
                        .borrow_mut()
                        .push((event.current, event.total, event.current_file));
                },
                discard: &|path| {
                    discarded.borrow_mut().push(path.to_path_buf());
                    remove_file(path)
                },
            },
        )
        .unwrap();

        assert_eq!(result.moved_count, 1);
        let target = music.join("Artist/Album/01 First.mp3");
        assert_eq!(fs::read(&target).unwrap(), b"data");
        assert_eq!(
            Path::new(&find(&state, &track_id).file_path),
            target.as_path()
        );
        assert_eq!(*discarded.borrow(), std::slice::from_ref(&source));
        assert!(!source.exists());
        assert_eq!(*progress.borrow(), [(1, 1, "a.mp3".to_string())]);
    }

    /// 場所を書き換えられなかった曲は、コピーを消し、元を残す
    #[test]
    fn test_collecting_removes_copies_when_the_library_cannot_be_updated() {
        let dir = TempDir::new();
        let state = test_state();
        let destination = music_destination(&state, &dir.path().join("Music"));
        let music = destination.music_folder.clone().unwrap();
        let source = dir.path().join("External/a.mp3");
        let track_id = add_track(&state, &source, "First", 1);
        let mut plan = build_plan(&state, std::slice::from_ref(&track_id), &destination).unwrap();
        plan.moves[0].copy = true;
        // 計画を立てた後に、曲がライブラリから外された
        state
            .with_db(|db| crate::library::delete_tracks(db, std::slice::from_ref(&track_id)))
            .unwrap();

        let result = execute_plan(
            &state,
            &plan,
            Some(&music),
            &Execution {
                cancel: &AtomicBool::new(false),
                on_progress: &|_| {},
                discard: &|_| panic!("元は消さない"),
            },
        )
        .unwrap();

        assert_eq!((result.moved_count, result.failed_count), (0, 1));
        assert!(!music.join("Artist/Album/01 First.mp3").exists());
        assert!(source.is_file());
    }

    /// 中止すると、次の曲の前で止まる（移した曲は、移したまま）
    #[test]
    fn test_cancelling_stops_before_the_next_track() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let state = test_state();
        register_library(&state, &library);
        let ids = [
            add_track(&state, &library.join("in/a.mp3"), "First", 1),
            add_track(&state, &library.join("in/b.mp3"), "Second", 2),
        ];
        let cancel = AtomicBool::new(false);

        let result = organize_blocking(
            &state,
            &ids,
            &library_destination(TEMPLATE),
            &Execution {
                cancel: &cancel,
                // 1曲目を移し始めたところで、中止された
                on_progress: &|_| cancel.store(true, Ordering::SeqCst),
                discard: &remove_file,
            },
        )
        .unwrap();

        assert!(result.cancelled);
        assert_eq!(result.moved_count, 1);
        assert!(library.join("Artist/Album/01 First.mp3").is_file());
        assert!(library.join("in/b.mp3").is_file());
    }

    /// タグを保存した曲のうち、音楽フォルダの中の曲だけを、書式どおりの場所へ移し直す
    #[test]
    fn test_saved_tracks_inside_the_music_folder_are_organized() {
        let dir = TempDir::new();
        let state = test_state();
        let music = dir.path().join("Music");
        register_library(&state, &music);
        let inside = add_track(&state, &music.join("Old Name/a.mp3"), "First", 1);
        let outside = add_track(&state, &dir.path().join("Downloads/b.mp3"), "Second", 2);

        let result = organize_saved_tracks(
            &state,
            &[inside.clone(), outside.clone()],
            music.clone(),
            Template::parse(TEMPLATE).unwrap(),
        );

        assert_eq!(
            result,
            Some(KeepOrganizedResult {
                moved_count: 1,
                not_moved_count: 0,
                errors: Vec::new()
            })
        );
        assert_eq!(
            Path::new(&find(&state, &inside).file_path),
            music.join("Artist/Album/01 First.mp3")
        );
        assert!(!music.join("Old Name").exists());
        // 音楽フォルダの外の曲は、動かさない
        assert!(dir.path().join("Downloads/b.mp3").is_file());
        // 音楽フォルダの中の曲がなければ、何もしない
        assert_eq!(
            organize_saved_tracks(
                &state,
                std::slice::from_ref(&outside),
                music.clone(),
                Template::parse(TEMPLATE).unwrap(),
            ),
            None
        );
    }

    /// 名前が衝突する曲は動かさず、数を知らせる
    #[test]
    fn test_saved_tracks_with_conflicting_names_are_not_moved() {
        let dir = TempDir::new();
        let state = test_state();
        let music = dir.path().join("Music");
        register_library(&state, &music);
        let track_id = add_track(&state, &music.join("Old Name/a.mp3"), "First", 1);
        let taken = music.join("Artist/Album/01 First.mp3");
        fs::create_dir_all(taken.parent().unwrap()).unwrap();
        fs::write(&taken, b"other").unwrap();

        let result = organize_saved_tracks(
            &state,
            std::slice::from_ref(&track_id),
            music.clone(),
            Template::parse(TEMPLATE).unwrap(),
        )
        .unwrap();

        assert_eq!((result.moved_count, result.not_moved_count), (0, 1));
        assert_eq!(fs::read(&taken).unwrap(), b"other");
        assert!(music.join("Old Name/a.mp3").is_file());
    }
}
