//! 形式の変換のコマンド

use super::import::{ImportTargets, create_track_from_file, import_targets};
use super::{file_dialog, run_blocking};
use crate::error::{AppError, AppResult};
use crate::events::{ConvertProgress, LibraryChanged};
use crate::file_organize::{Template, move_no_replace};
use crate::library::{DuplicateAction, get_file_modified_at, to_count};
use crate::models::Track;
use crate::state::AppState;
use crate::transcode::{
    self, AAC_BITRATES_KBPS, ConvertEntry, ConvertFormat, ConvertMode, ConvertStatus, PlanOptions,
    PlannedConversion,
};
use crate::validation::validate_track_id;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::thread;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 並行して変換する曲の数の上限（再生・ほかの操作のために、コアを使い切らない）
const MAX_WORKERS: usize = 4;

/// 変換の指定
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConvertOptions {
    pub format: ConvertFormat,
    /// AACのビットレート（kbps。`AAC_BITRATES_KBPS`のどれか。AACのほかでは使わない）
    pub aac_bitrate_kbps: u32,
    pub mode: ConvertMode,
    /// 書き出す場所の書式（ファイルの整理と同じ書き方。書き出しだけで使う）
    pub template: String,
    /// 書き出した曲を、ライブラリに加えるか（書き出しだけで使う）
    pub add_to_library: bool,
}

/// 変換の計画（選んだ曲の順）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConvertPlan {
    pub entries: Vec<ConvertEntry>,
}

/// 変換の結果
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConvertResult {
    /// 変換した曲の数
    pub converted_count: u32,
    /// 変換しなかった曲の数（ファイルが見つからない・出力先がふさがっている）
    pub skipped_count: u32,
    /// 変換できなかった曲の数
    pub failed_count: u32,
    /// 変換できなかった理由（`場所: 理由`）
    pub errors: Vec<String>,
    /// 途中で中断したか
    pub cancelled: bool,
}

/// 変換の実行の状態（Tauriの状態として管理する。同時に実行できる変換は1つ）
#[derive(Default)]
pub struct ConvertState {
    running: AtomicBool,
    cancel: AtomicBool,
}

impl ConvertState {
    /// 変換を始める。ほかの変換が実行中ならNone（返したガードを破棄すると、実行中の印が外れる）
    fn begin(&self) -> Option<ConvertGuard<'_>> {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        self.cancel.store(false, Ordering::SeqCst);
        Some(ConvertGuard { state: self })
    }
}

struct ConvertGuard<'a> {
    state: &'a ConvertState,
}

impl Drop for ConvertGuard<'_> {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::SeqCst);
    }
}

/// 入力を確かめて、書式を解釈する
fn validate_input(track_ids: &[String], options: &ConvertOptions) -> AppResult<Template> {
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "変換する曲が指定されていません".to_string(),
        ));
    }
    for track_id in track_ids {
        validate_track_id(track_id)?;
    }
    if options.format == ConvertFormat::Aac
        && !AAC_BITRATES_KBPS.contains(&options.aac_bitrate_kbps)
    {
        return Err(AppError::Validation(
            "AACのビットレートが選べる値ではありません".to_string(),
        ));
    }
    Template::parse(&options.template)
}

/// 選んだ曲の、変換の計画を立てる
fn build_plan(
    state: &AppState,
    track_ids: &[String],
    options: &ConvertOptions,
    template: &Template,
    export_folder: Option<&Path>,
) -> AppResult<(Vec<ConvertEntry>, Vec<PlannedConversion>)> {
    let (tracks, registered_paths) = state.with_db(|db| {
        let mut by_id = crate::repository::find_tracks_by_ids(db, track_ids)?;
        // 選んだ順にする（同じIDの2つ目以降と、ライブラリにない曲は除く）
        let tracks: Vec<Track> = track_ids
            .iter()
            .filter_map(|track_id| by_id.remove(track_id))
            .collect();
        Ok((tracks, crate::repository::find_all_file_paths(db)?))
    })?;

    // ファイルの確認はファイルシステムへのアクセスのため、DBロックの外で行う
    Ok(transcode::plan(
        &tracks,
        &PlanOptions {
            format: options.format,
            mode: options.mode,
            template,
            export_folder,
            registered_paths: &registered_paths,
        },
    ))
}

/// 変換先にできる形式を取得する（この環境で変換できなければ、空）
///
/// 形式の変換は、macOSだけで使える。空の場合、画面は変換の項目を出さない。
#[tauri::command]
#[specta::specta]
pub async fn get_convert_formats() -> AppResult<Vec<ConvertFormat>> {
    Ok(if transcode::is_available() {
        vec![
            ConvertFormat::Aac,
            ConvertFormat::Alac,
            ConvertFormat::Flac,
            ConvertFormat::Wav,
            ConvertFormat::Aiff,
        ]
    } else {
        Vec::new()
    })
}

/// 選んだ曲の、変換の計画（曲ごとの出力先と、変換するかどうか）を取得する
///
/// ファイルは作らない。書き出しでは、書き出すフォルダをまだ選んでいないため、出力先の同じ名前の
/// ファイルは調べない（`convert_tracks`が、フォルダを選んだ後に調べる）。
#[tauri::command]
#[specta::specta]
pub async fn plan_conversion(
    track_ids: Vec<String>,
    options: ConvertOptions,
    app: AppHandle,
) -> AppResult<ConvertPlan> {
    let template = validate_input(&track_ids, &options)?;

    run_blocking(move || {
        let state = app.state::<AppState>();
        let (entries, _) = build_plan(state.inner(), &track_ids, &options, &template, None)?;
        Ok(ConvertPlan { entries })
    })
    .await
}

/// 選んだ曲を、別の形式に変換する
///
/// 書き出しでは、書き出すフォルダを選ぶダイアログを開く（選ばなかった場合は`null`を返す）。
/// 置き換えでは、元のファイルと同じ場所に変換後のファイルを作り、曲のIDを保ったまま入れ替えて、
/// 元のファイルをOSのゴミ箱へ移す。
///
/// 終わるまで返らない（`ConvertProgress`を送る）。同時に実行できる変換は1つで、実行中は`LOCK`。
/// 変換できなかった曲は、結果の`errors`へ理由を入れて残りを続ける。
#[tauri::command]
#[specta::specta]
pub async fn convert_tracks(
    track_ids: Vec<String>,
    options: ConvertOptions,
    app: AppHandle,
) -> AppResult<Option<ConvertResult>> {
    let template = validate_input(&track_ids, &options)?;
    if !transcode::is_available() {
        return Err(AppError::Validation(
            "この環境では、形式を変換できません".to_string(),
        ));
    }

    run_blocking(move || {
        let convert = app.state::<ConvertState>();
        let _running = convert
            .begin()
            .ok_or_else(|| AppError::Lock("形式の変換を実行中です".to_string()))?;
        let state = app.state::<AppState>();

        // 書き出すフォルダは、Rust側のダイアログで選ぶ（WebViewからパスを受け取らない。ADR-005）
        let export_folder = match options.mode {
            ConvertMode::Export => {
                let Some(folder) = file_dialog(&app).blocking_pick_folder() else {
                    return Ok(None);
                };
                Some(
                    folder
                        .into_path()
                        .map_err(|e| AppError::Io(format!("選んだフォルダを開けません: {}", e)))?,
                )
            }
            ConvertMode::Replace => None,
        };
        let temp_dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| AppError::Io(format!("一時フォルダを決められません: {}", e)))?
            .join("convert");

        let job = ConvertJob {
            state: state.inner(),
            options: &options,
            temp_dir: &temp_dir,
            cancelled: &convert.cancel,
            trash: &crate::trash::move_to_trash,
        };
        let (result, outputs) = job.run(
            &track_ids,
            &template,
            export_folder.as_deref(),
            &|progress| {
                if let Err(e) = progress.emit(&app) {
                    log::warn!("変換の進捗イベントの送信に失敗しました: {}", e);
                }
            },
        )?;

        // 書き出した曲を、ライブラリに加える（曲だけを登録する。フォルダは、ライブラリフォルダにしない）
        let mut library_changed =
            options.mode == ConvertMode::Replace && result.converted_count > 0;
        if options.mode == ConvertMode::Export && options.add_to_library && !outputs.is_empty() {
            let targets = ImportTargets::from_paths(&outputs);
            match import_targets(state.inner(), &targets, DuplicateAction::Skip, &|_| {}) {
                Ok(imported) => library_changed |= imported.imported_count > 0,
                Err(e) => log::warn!("書き出した曲を、ライブラリに加えられませんでした: {}", e),
            }
        }
        if library_changed && let Err(e) = LibraryChanged.emit(&app) {
            log::warn!("ライブラリの変更の通知に失敗しました: {}", e);
        }

        log::info!(
            "形式を変換しました: 変換={}, 変換しない={}, 失敗={}, 中断={}",
            result.converted_count,
            result.skipped_count,
            result.failed_count,
            result.cancelled
        );
        Ok(Some(result))
    })
    .await
}

/// 実行中の変換を中断する（実行中でなければ何もしない）
///
/// 変換の途中の曲は、出力を残さずに終わる（置き換えでは、元のファイルをそのまま残す）。
#[tauri::command]
#[specta::specta]
pub async fn cancel_conversion(convert: State<'_, ConvertState>) -> AppResult<()> {
    if convert.running.load(Ordering::SeqCst) {
        convert.cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// 変換の実行（同期処理）
struct ConvertJob<'a> {
    state: &'a AppState,
    options: &'a ConvertOptions,
    /// デコードした音声の一時ファイルを置くフォルダ
    temp_dir: &'a Path,
    cancelled: &'a AtomicBool,
    /// ファイルをゴミ箱へ移す処理（ゴミ箱の中での場所を返す。テストでは、差し替える）
    trash: &'a (dyn Fn(&Path) -> AppResult<Option<PathBuf>> + Sync),
}

/// 1曲の変換の結果
enum Outcome {
    Converted,
    /// 中断した（出力は残していない）
    Cancelled,
}

impl ConvertJob<'_> {
    /// 計画を立てて、変換できる曲を変換する
    ///
    /// @returns 結果と、できたファイル（書き出しで、ライブラリに加えるために使う）
    fn run(
        &self,
        track_ids: &[String],
        template: &Template,
        export_folder: Option<&Path>,
        on_progress: &(dyn Fn(ConvertProgress) + Sync),
    ) -> AppResult<(ConvertResult, Vec<PathBuf>)> {
        self.clean_temp_dir();
        let (entries, conversions) =
            build_plan(self.state, track_ids, self.options, template, export_folder)?;
        let skipped = entries
            .iter()
            .filter(|entry| entry.status != ConvertStatus::Convert)
            .count();
        let total = to_count(conversions.len())?;

        let result = Mutex::new(ConvertResult {
            skipped_count: to_count(skipped)?,
            ..ConvertResult::default()
        });
        let outputs = Mutex::new(Vec::new());
        let next = AtomicUsize::new(0);
        let done = AtomicU32::new(0);
        let cores = thread::available_parallelism().map_or(1, |cores| cores.get());
        let workers = (cores / 2)
            .clamp(1, MAX_WORKERS)
            .min(conversions.len().max(1));

        thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    while !self.cancelled.load(Ordering::Relaxed) {
                        let Some(planned) = conversions.get(next.fetch_add(1, Ordering::Relaxed))
                        else {
                            break;
                        };
                        // 置き換えでは、変換の間に同じ曲のタグを書き換える処理と重ねない
                        // （元のファイルから読んだタグを、変換したファイルへ写すため）
                        let _file = (self.options.mode == ConvertMode::Replace)
                            .then(|| self.state.lock_track_file(&planned.track_id));
                        let outcome = self.convert(planned);
                        // 守るのは集計だけのため、途中でパニックしたスレッドがあっても続ける
                        let mut result = result.lock().unwrap_or_else(|p| p.into_inner());
                        match outcome {
                            Ok(Outcome::Converted) => {
                                result.converted_count += 1;
                                outputs
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .push(planned.output.clone());
                            }
                            Ok(Outcome::Cancelled) => continue,
                            Err(e) => {
                                result.failed_count += 1;
                                result
                                    .errors
                                    .push(format!("{}: {}", planned.source.display(), e));
                            }
                        }
                        drop(result);
                        on_progress(ConvertProgress {
                            current: done.fetch_add(1, Ordering::Relaxed) + 1,
                            total,
                            current_file: planned
                                .source
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("不明なファイル")
                                .to_string(),
                        });
                    }
                });
            }
        });

        let mut result = result.into_inner().unwrap_or_else(|p| p.into_inner());
        result.cancelled = self.cancelled.load(Ordering::Relaxed);
        Ok((
            result,
            outputs.into_inner().unwrap_or_else(|p| p.into_inner()),
        ))
    }

    /// 前の変換が異常終了して残った一時ファイルを消す
    fn clean_temp_dir(&self) {
        let Ok(entries) = fs::read_dir(self.temp_dir) else {
            return;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            if path.extension().is_some_and(|extension| extension == "caf") {
                fs::remove_file(&path).ok();
            }
        }
    }

    /// 1曲を変換する
    fn convert(&self, planned: &PlannedConversion) -> AppResult<Outcome> {
        let directory = planned
            .output
            .parent()
            .ok_or_else(|| AppError::Io("出力先のフォルダが決まりません".to_string()))?;
        fs::create_dir_all(directory)
            .map_err(|e| AppError::Io(format!("出力先のフォルダを作れませんでした: {}", e)))?;

        // 出力先と同じフォルダの、一時的な名前に書いてから、出力先の名前にする
        // （途中まで書いたファイルを、出力先の名前で見せない。出力先のファイルを上書きしない）
        let extension = self.options.format.extension();
        let partial = directory.join(format!(
            ".{}.converting.{}",
            uuid::Uuid::new_v4(),
            extension
        ));
        let converted = transcode::convert_file(
            &planned.source,
            &partial,
            self.options.format,
            self.options.aac_bitrate_kbps,
            self.temp_dir,
            self.cancelled,
        )?;
        if !converted {
            return Ok(Outcome::Cancelled);
        }

        let placed = match self.options.mode {
            ConvertMode::Export => place(&partial, &planned.output),
            ConvertMode::Replace => self.replace(planned, &partial),
        };
        if placed.is_err() {
            fs::remove_file(&partial).ok();
        }
        placed.map(|()| Outcome::Converted)
    }

    /// 元のファイルを、変換したファイルに入れ替える
    ///
    /// 変換したファイルを出力先に置き、元のファイルをゴミ箱へ移し、ライブラリの曲の場所と形式を
    /// 書き換える。どこかで失敗した場合は、元の状態に戻す（元のファイルを、元の場所へ戻す）。
    fn replace(&self, planned: &PlannedConversion, partial: &Path) -> AppResult<()> {
        let same_path = planned.output == planned.source;
        // 元のファイルを、ゴミ箱から元の場所へ戻す
        let restore = |trashed: &Option<PathBuf>| {
            let restored = trashed
                .as_deref()
                .is_some_and(|trashed| move_no_replace(trashed, &planned.source).is_ok());
            if !restored {
                log::error!(
                    "元のファイルを、ゴミ箱から戻せませんでした: {}",
                    planned.source.display()
                );
            }
        };

        // 拡張子が同じ場合は、元のファイルをゴミ箱へ移してからでないと、出力先に置けない
        let trashed = if same_path {
            let trashed = (self.trash)(&planned.source)?;
            if let Err(e) = place(partial, &planned.output) {
                restore(&trashed);
                return Err(e);
            }
            trashed
        } else {
            place(partial, &planned.output)?;
            match (self.trash)(&planned.source) {
                Ok(trashed) => trashed,
                Err(e) => {
                    fs::remove_file(&planned.output).ok();
                    return Err(e);
                }
            }
        };

        let recorded = create_track_from_file(&planned.output).and_then(|file| {
            let modified_at = get_file_modified_at(&planned.output);
            self.state.with_db(|db| {
                crate::repository::replace_track_file(db, &planned.track_id, &file, modified_at)
            })
        });
        if let Err(e) = recorded {
            fs::remove_file(&planned.output).ok();
            restore(&trashed);
            return Err(e);
        }
        Ok(())
    }
}

/// 変換したファイルを、出力先の名前にする（出力先にファイルがあれば、上書きせずに失敗する）
fn place(partial: &Path, output: &Path) -> AppResult<()> {
    move_no_replace(partial, output).map_err(|e| {
        AppError::Io(if e.kind() == std::io::ErrorKind::AlreadyExists {
            "出力先に同じ名前のファイルがあります".to_string()
        } else {
            format!("変換したファイルを、出力先に置けませんでした: {}", e)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(format: ConvertFormat, mode: ConvertMode) -> ConvertOptions {
        ConvertOptions {
            format,
            aac_bitrate_kbps: transcode::DEFAULT_AAC_BITRATE_KBPS,
            mode,
            template: "%artist%/%title%".to_string(),
            add_to_library: false,
        }
    }

    #[test]
    fn test_validate_input() {
        let id = uuid::Uuid::new_v4().to_string();
        let ids = std::slice::from_ref(&id);
        let valid = options(ConvertFormat::Aac, ConvertMode::Export);
        assert!(validate_input(ids, &valid).is_ok());

        let invalid = |result: AppResult<Template>| matches!(result, Err(AppError::Validation(_)));
        assert!(invalid(validate_input(&[], &valid)));
        assert!(invalid(validate_input(&["x".to_string()], &valid)));
        assert!(invalid(validate_input(
            ids,
            &ConvertOptions {
                aac_bitrate_kbps: 100,
                ..valid.clone()
            }
        )));
        assert!(invalid(validate_input(
            ids,
            &ConvertOptions {
                template: "%unknown%".to_string(),
                ..valid.clone()
            }
        )));
        // AACのほかでは、ビットレートを確かめない
        assert!(
            validate_input(
                ids,
                &ConvertOptions {
                    format: ConvertFormat::Flac,
                    aac_bitrate_kbps: 0,
                    ..valid
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn test_only_one_conversion_runs_at_a_time() {
        let state = ConvertState::default();
        let running = state.begin().expect("始められるはず");
        assert!(state.begin().is_none());
        drop(running);
        assert!(state.begin().is_some());
    }

    /// 実際に変換する確認（`afconvert`のあるmacOSだけ）
    #[cfg(target_os = "macos")]
    mod converting {
        use super::*;
        use crate::device_manifest::test_support::TempDir;
        use crate::models::ReplayGain;
        use crate::playback::test_audio::write_wav_samples;

        /// ライブラリと、ゴミ箱の代わりのフォルダ
        struct Fixture {
            dir: TempDir,
            state: AppState,
            /// ゴミ箱へ移せないことにするか
            trash_fails: AtomicBool,
        }

        impl Fixture {
            fn new() -> Self {
                let conn = crate::db::open_in_memory();
                Self {
                    dir: TempDir::new(),
                    state: AppState::new(conn),
                    trash_fails: AtomicBool::new(false),
                }
            }

            fn path(&self, relative: &str) -> PathBuf {
                self.dir.path().join(relative)
            }

            /// 曲のファイル（WAV）を作り、ライブラリに登録する
            fn add(&self, relative: &str, title: &str) -> String {
                let path = self.path(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                // 1秒（macOSのFLACのエンコーダーは、約0.1秒より短い音声を変換できない）
                let samples: Vec<i16> = (0..16_000).map(|i| (i % 200 * 100) as i16).collect();
                write_wav_samples(&path, 8_000, 2, &samples);
                let track = Track {
                    id: uuid::Uuid::new_v4().to_string(),
                    file_path: path.to_str().unwrap().to_string(),
                    file_name: path.file_name().unwrap().to_str().unwrap().to_string(),
                    title: Some(title.to_string()),
                    artist: Some("Artist".to_string()),
                    album: None,
                    album_artist: None,
                    genre: None,
                    year: None,
                    track_number: None,
                    disc_number: None,
                    duration: Some(1),
                    file_size: fs::metadata(&path).unwrap().len() as i64,
                    format: "WAV".to_string(),
                    bitrate: None,
                    sample_rate: Some(8_000),
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
                self.state
                    .with_db(|db| crate::repository::insert_track(db, &track, Some(1)))
                    .unwrap();
                track.id
            }

            fn find(&self, id: &str) -> Track {
                self.state
                    .with_db(|db| crate::repository::find_track_by_id(db, id))
                    .unwrap()
            }

            /// ゴミ箱の代わりに、`trash`フォルダへ移す
            fn trash(&self, path: &Path) -> AppResult<Option<PathBuf>> {
                if self.trash_fails.load(Ordering::Relaxed) {
                    return Err(AppError::Io("ゴミ箱へ移せません".to_string()));
                }
                let trashed = self.path("trash").join(path.file_name().unwrap());
                fs::create_dir_all(trashed.parent().unwrap()).unwrap();
                fs::rename(path, &trashed).unwrap();
                Ok(Some(trashed))
            }

            fn run(
                &self,
                ids: &[String],
                options: &ConvertOptions,
                export_folder: Option<&Path>,
                cancelled: bool,
            ) -> (ConvertResult, Vec<PathBuf>, Vec<ConvertProgress>) {
                let temp_dir = self.path("temp");
                let cancelled = AtomicBool::new(cancelled);
                let trash = |path: &Path| self.trash(path);
                let job = ConvertJob {
                    state: &self.state,
                    options,
                    temp_dir: &temp_dir,
                    cancelled: &cancelled,
                    trash: &trash,
                };
                let progress = Mutex::new(Vec::new());
                let template = Template::parse(&options.template).unwrap();
                let (result, outputs) = job
                    .run(ids, &template, export_folder, &|event| {
                        progress.lock().unwrap().push(event)
                    })
                    .unwrap();
                (result, outputs, progress.into_inner().unwrap())
            }
        }

        #[test]
        fn test_export_writes_converted_files_and_leaves_the_library_alone() {
            let fixture = Fixture::new();
            let first = fixture.add("music/a.wav", "First");
            let second = fixture.add("music/b.wav", "Second");
            let blocked = fixture.add("music/c.wav", "Blocked");
            let export = fixture.path("export");
            // 出力先に、同じ名前のファイルがある
            fs::create_dir_all(export.join("Artist")).unwrap();
            fs::write(export.join("Artist/Blocked.flac"), b"other").unwrap();

            let ids = [first.clone(), second, blocked];
            let (result, mut outputs, progress) = fixture.run(
                &ids,
                &options(ConvertFormat::Flac, ConvertMode::Export),
                Some(&export),
                false,
            );

            assert_eq!(
                result,
                ConvertResult {
                    converted_count: 2,
                    skipped_count: 1,
                    ..ConvertResult::default()
                }
            );
            outputs.sort();
            assert_eq!(
                outputs,
                [
                    export.join("Artist/First.flac"),
                    export.join("Artist/Second.flac")
                ]
            );
            assert!(outputs.iter().all(|output| output.is_file()));
            assert_eq!(
                fs::read(export.join("Artist/Blocked.flac")).unwrap(),
                b"other"
            );
            assert_eq!(progress.len(), 2);
            assert!(progress.iter().all(|event| event.total == 2));
            // 元のファイルとライブラリは、変えない
            assert!(fixture.path("music/a.wav").is_file());
            assert_eq!(fixture.find(&first).format, "WAV");
            // 書きかけのファイル・一時ファイルを残さない
            assert_eq!(fs::read_dir(export.join("Artist")).unwrap().count(), 3);
            assert_eq!(fs::read_dir(fixture.path("temp")).unwrap().count(), 0);
        }

        #[test]
        fn test_replace_swaps_the_file_and_keeps_the_track() {
            let fixture = Fixture::new();
            let id = fixture.add("music/a.wav", "First");
            fixture
                .state
                .with_db(|db| {
                    crate::repository::set_tracks_favorite(db, std::slice::from_ref(&id), true)
                })
                .unwrap();

            let (result, _, _) = fixture.run(
                std::slice::from_ref(&id),
                &options(ConvertFormat::Flac, ConvertMode::Replace),
                None,
                false,
            );

            assert_eq!(result.converted_count, 1, "{:?}", result.errors);
            let flac = fixture.path("music/a.flac");
            assert!(flac.is_file());
            // 元のファイルは、ゴミ箱へ移る
            assert!(!fixture.path("music/a.wav").exists());
            assert!(fixture.path("trash/a.wav").is_file());
            // 曲のID・タイトル・お気に入りはそのままで、場所と形式が変わる
            let track = fixture.find(&id);
            assert_eq!(track.file_path, flac.to_str().unwrap());
            assert_eq!(track.file_name, "a.flac");
            assert_eq!(track.format, "FLAC");
            assert_eq!(track.file_size, fs::metadata(&flac).unwrap().len() as i64);
            assert_eq!(track.title.as_deref(), Some("First"));
            assert!(track.is_favorite);
            assert!(!track.is_missing);

            // 同じ形式への置き換え（出力先が、元のファイル自身）
            let (again, _, _) = fixture.run(
                std::slice::from_ref(&id),
                &options(ConvertFormat::Flac, ConvertMode::Replace),
                None,
                false,
            );
            assert_eq!(again.converted_count, 1);
            assert!(flac.is_file());
            assert!(fixture.path("trash/a.flac").is_file());
            assert_eq!(fixture.find(&id).file_path, flac.to_str().unwrap());
            assert_eq!(fs::read_dir(fixture.path("music")).unwrap().count(), 1);
        }

        #[test]
        fn test_replace_is_undone_when_the_original_cannot_be_trashed() {
            let fixture = Fixture::new();
            let id = fixture.add("music/a.wav", "First");
            fixture.trash_fails.store(true, Ordering::Relaxed);

            let (result, _, _) = fixture.run(
                std::slice::from_ref(&id),
                &options(ConvertFormat::Flac, ConvertMode::Replace),
                None,
                false,
            );

            assert_eq!(result.converted_count, 0);
            assert_eq!(result.failed_count, 1);
            assert!(result.errors[0].contains("ゴミ箱"), "{:?}", result.errors);
            // 元のファイルとライブラリは、そのまま。変換したファイルは残さない
            assert!(fixture.path("music/a.wav").is_file());
            assert_eq!(fs::read_dir(fixture.path("music")).unwrap().count(), 1);
            assert_eq!(fixture.find(&id).format, "WAV");
        }

        #[test]
        fn test_replace_restores_the_original_when_the_library_cannot_be_updated() {
            let fixture = Fixture::new();
            let id = fixture.add("music/a.wav", "First");
            let source = fixture.path("music/a.wav");
            let options = options(ConvertFormat::Flac, ConvertMode::Replace);
            let template = Template::parse(&options.template).unwrap();
            let (_, conversions) = build_plan(
                &fixture.state,
                std::slice::from_ref(&id),
                &options,
                &template,
                None,
            )
            .unwrap();
            // 変換している間に、曲がライブラリから外された
            fixture
                .state
                .with_db(|db| crate::library::delete_tracks(db, std::slice::from_ref(&id)))
                .unwrap();
            let temp_dir = fixture.path("temp");
            let cancelled = AtomicBool::new(false);
            let trash = |path: &Path| fixture.trash(path);
            let job = ConvertJob {
                state: &fixture.state,
                options: &options,
                temp_dir: &temp_dir,
                cancelled: &cancelled,
                trash: &trash,
            };

            let outcome = job.convert(&conversions[0]);

            assert!(matches!(outcome, Err(AppError::NotFound(_))));
            assert!(source.is_file());
            assert_eq!(fs::read_dir(fixture.path("music")).unwrap().count(), 1);
            assert_eq!(fs::read_dir(fixture.path("trash")).unwrap().count(), 0);
        }

        #[test]
        fn test_cancelled_conversion_changes_nothing() {
            let fixture = Fixture::new();
            let id = fixture.add("music/a.wav", "First");

            let (result, outputs, progress) = fixture.run(
                std::slice::from_ref(&id),
                &options(ConvertFormat::Flac, ConvertMode::Replace),
                None,
                true,
            );

            assert_eq!(
                result,
                ConvertResult {
                    cancelled: true,
                    ..ConvertResult::default()
                }
            );
            assert!(outputs.is_empty());
            assert!(progress.is_empty());
            assert!(fixture.path("music/a.wav").is_file());
            assert_eq!(fs::read_dir(fixture.path("music")).unwrap().count(), 1);
        }
    }
}
