//! 曲の解析（テンポ・キー・ペース）
//!
//! macOS 27のMusicUnderstanding（端末内で動くAppleの楽曲解析）で、曲のテンポ（BPM）・キー・ペースを求め、
//! DBの`track_analysis`に保存する（ADR-104）。AIシャッフルの「似た曲」が使う（`ai_shuffle`）。
//!
//! - 解析はSwiftのコード（`swift/music_analysis.swift`）が行う。build.rsは、SDKにMusicUnderstandingが
//!   ある時だけそれを組み込み、`music_understanding`の印を立てる。組み込んでいない時と、macOS 27より
//!   古いmacOSでは、解析を使えない（`is_available`がfalse）
//! - 解析は利用者が設定画面から始め、バックグラウンドのスレッドで1曲ずつ進む。止めるまでに終わらなければ、
//!   次の起動で続きから再開する（`resume`）。解析の途中でライブラリが変わっても（音楽フォルダの変更で
//!   ライブラリを置き換えた・曲を読み込んだなど）、外した曲は飛ばし、読み込んだ曲も続けて解析する
//!   （`analyze_pending`）
//! - 曲の長さを一緒に保存し、ファイルの差し替えで長さが変わった曲は解析し直す（タグの書き換え
//!   （評価など）では、ファイルの更新日時が変わっても、解析し直さない）
//! - 解析できなかった曲（AVFoundationが読めない形式など）も、解析し直さないよう記録する。
//!   見つからない・読めないファイル（NASの切断など）は記録せず、次の解析で読み直す

use crate::error::{AppError, AppResult};
use crate::events::MusicAnalysisProgress;
use crate::state::AppState;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use specta::Type;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// 解析を続けていたか（`app_meta`のキー。値が`1`の間、次の起動で再開する）
const REQUESTED_META_KEY: &str = "music_analysis_requested";

fn db_error(e: rusqlite::Error) -> AppError {
    AppError::Database(format!("曲の解析のデータの読み書きに失敗しました: {}", e))
}

/// 曲のキー（主音と長調・短調）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MusicKey {
    /// 主音のピッチクラス（C = 0, C# = 1, ..., B = 11）
    pub pitch_class: u8,
    pub minor: bool,
}

impl MusicKey {
    /// 保存してある値から作る（範囲の外の値はNone）
    pub fn from_stored(pitch_class: Option<i64>, mode: Option<i64>) -> Option<Self> {
        let pitch_class = u8::try_from(pitch_class?)
            .ok()
            .filter(|value| *value < 12)?;
        let minor = match mode? {
            0 => false,
            1 => true,
            _ => return None,
        };
        Some(Self { pitch_class, minor })
    }

    /// Camelot表記の番号（1〜12）。平行調（例: D majorとB minor）は同じ番号で、番号が1違うキーは、
    /// 五度圏で隣どうし。C major = 8B、A minor = 8A
    pub fn camelot_number(self) -> u8 {
        let relative_major = if self.minor {
            (self.pitch_class + 3) % 12
        } else {
            self.pitch_class
        };
        (relative_major * 7 + 7) % 12 + 1
    }

    /// 2つのキーのCamelotの番号の差（0〜6。円環で数える）
    pub fn camelot_distance(self, other: Self) -> u8 {
        let difference = self.camelot_number().abs_diff(other.camelot_number());
        difference.min(12 - difference)
    }
}

/// 1曲の解析の結果（求まらなかった項目はNone）
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MusicFeatures {
    pub bpm: Option<f64>,
    pub key: Option<MusicKey>,
    pub pace: Option<f64>,
}

/// 1曲の解析の結末
///
/// 値を作るのは、Swiftのコードを組み込んだ時の`analyze_file`だけ（組み込まない時は、解析しない）。
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(all(target_os = "macos", music_understanding)), allow(dead_code))]
pub enum AnalyzeOutcome {
    Analyzed(MusicFeatures),
    /// ファイルを音声として開けない（AVFoundationが読めない形式など）
    Unsupported,
    /// 解析に失敗した
    Failed,
}

#[cfg(all(target_os = "macos", music_understanding))]
mod ffi {
    use std::os::raw::c_char;

    pub const STATUS_ANALYZED: i32 = 0;
    pub const STATUS_UNAVAILABLE: i32 = 1;
    pub const STATUS_UNSUPPORTED: i32 = 2;

    unsafe extern "C" {
        pub fn muspice_music_analysis_available() -> bool;
        pub fn muspice_music_analysis_analyze(
            path: *const c_char,
            bpm: *mut f64,
            key_pitch_class: *mut i32,
            key_mode: *mut i32,
            pace: *mut f64,
        ) -> i32;
    }
}

/// このMacで曲の解析を使えるか（MusicUnderstandingを組み込んでビルドし、macOS 27以降で動いている）
#[cfg(all(target_os = "macos", music_understanding))]
pub fn is_available() -> bool {
    // SAFETY: 引数のない関数で、OSの版を調べるだけ
    unsafe { ffi::muspice_music_analysis_available() }
}

#[cfg(not(all(target_os = "macos", music_understanding)))]
pub fn is_available() -> bool {
    false
}

/// 1曲を解析する（終わるまで返らない。数秒かかる）
///
/// 解析を使えない時はNone。
#[cfg(all(target_os = "macos", music_understanding))]
pub fn analyze_file(path: &Path) -> Option<AnalyzeOutcome> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return Some(AnalyzeOutcome::Unsupported);
    };
    let mut bpm = f64::NAN;
    let mut key_pitch_class = -1;
    let mut key_mode = -1;
    let mut pace = f64::NAN;
    // SAFETY: パスはNUL終端の文字列で、呼び出しの間だけ使われる。結果の書き込み先は、
    // この関数のローカル変数（呼び出しから戻るまで有効）
    let status = unsafe {
        ffi::muspice_music_analysis_analyze(
            path.as_ptr(),
            &mut bpm,
            &mut key_pitch_class,
            &mut key_mode,
            &mut pace,
        )
    };
    match status {
        ffi::STATUS_ANALYZED => Some(AnalyzeOutcome::Analyzed(MusicFeatures {
            bpm: Some(bpm).filter(|value| value.is_finite() && *value > 0.0),
            key: MusicKey::from_stored(Some(key_pitch_class.into()), Some(key_mode.into())),
            pace: Some(pace).filter(|value| value.is_finite()),
        })),
        ffi::STATUS_UNAVAILABLE => None,
        ffi::STATUS_UNSUPPORTED => Some(AnalyzeOutcome::Unsupported),
        _ => Some(AnalyzeOutcome::Failed),
    }
}

#[cfg(not(all(target_os = "macos", music_understanding)))]
pub fn analyze_file(_path: &Path) -> Option<AnalyzeOutcome> {
    None
}

// ---- DB ----

/// 解析の結果が、曲の今のファイルに当てはまる条件（`t`は`tracks`、`a`は`track_analysis`）
///
/// 長さが変わった曲（ファイルを別の音声に差し替えた）は、解析していないものとして扱う。
const VALID_ANALYSIS_JOIN: &str =
    "LEFT JOIN track_analysis a ON a.track_id = t.id AND a.duration IS t.duration";

/// 解析する曲（見つからない曲を除き、解析していない曲を、よく再生する順に）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PendingTrack {
    pub id: String,
    pub file_path: String,
    pub duration: Option<i32>,
}

/// 解析していない曲（見つからない曲を除く）。よく再生する曲から解析し、AIシャッフルに早く効かせる
pub fn pending_tracks(conn: &Connection) -> AppResult<Vec<PendingTrack>> {
    let sql = format!(
        "SELECT t.id, t.file_path, t.duration FROM tracks t {VALID_ANALYSIS_JOIN}
         WHERE t.missing_since IS NULL AND a.track_id IS NULL
         ORDER BY t.play_count DESC, t.rowid"
    );
    let mut stmt = conn.prepare(&sql).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PendingTrack {
                id: row.get(0)?,
                file_path: row.get(1)?,
                duration: row.get(2)?,
            })
        })
        .map_err(db_error)?;
    rows.collect::<rusqlite::Result<_>>().map_err(db_error)
}

/// 曲が、`pending_tracks`で読んだ時のまま、解析していない曲か
///
/// ライブラリにあり、見つからない曲でなく、パスと長さが変わっておらず、解析の結果がない時にtrue。
/// 読んだ後に、ライブラリから外した（音楽フォルダの変更でライブラリを置き換えたなど）・場所を付け替えた・
/// ファイルを差し替えた曲はfalse。
fn is_still_pending(conn: &Connection, track: &PendingTrack) -> AppResult<bool> {
    let sql = format!(
        "SELECT EXISTS (
            SELECT 1 FROM tracks t {VALID_ANALYSIS_JOIN}
            WHERE t.id = ?1 AND t.file_path = ?2 AND t.duration IS ?3
              AND t.missing_since IS NULL AND a.track_id IS NULL
         )"
    );
    conn.query_row(
        &sql,
        params![track.id, track.file_path, track.duration],
        |row| row.get(0),
    )
    .map_err(db_error)
}

/// 1曲の解析の結果を保存する（曲がライブラリからなくなっていれば、何もしない）
pub fn save_outcome(
    conn: &Connection,
    track_id: &str,
    duration: Option<i32>,
    outcome: &AnalyzeOutcome,
) -> AppResult<()> {
    let (features, failed) = match outcome {
        AnalyzeOutcome::Analyzed(features) => (*features, false),
        AnalyzeOutcome::Unsupported | AnalyzeOutcome::Failed => (MusicFeatures::default(), true),
    };
    conn.execute(
        "INSERT INTO track_analysis
            (track_id, duration, bpm, key_pitch_class, key_mode, pace, failed, analyzed_at)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now')
         WHERE EXISTS (SELECT 1 FROM tracks WHERE id = ?1)
         ON CONFLICT(track_id) DO UPDATE SET
            duration = excluded.duration,
            bpm = excluded.bpm,
            key_pitch_class = excluded.key_pitch_class,
            key_mode = excluded.key_mode,
            pace = excluded.pace,
            failed = excluded.failed,
            analyzed_at = excluded.analyzed_at",
        params![
            track_id,
            duration,
            features.bpm,
            features.key.map(|key| key.pitch_class),
            features.key.map(|key| u8::from(key.minor)),
            features.pace,
            failed,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

/// 解析の進み具合の数（見つからない曲を除く）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnalysisCounts {
    /// 曲の数
    pub total: u32,
    /// 解析した曲の数
    pub analyzed: u32,
    /// 解析できなかった曲の数
    pub failed: u32,
}

pub fn count_analysis(conn: &Connection) -> AppResult<AnalysisCounts> {
    let sql = format!(
        "SELECT COUNT(*),
                COALESCE(SUM(a.track_id IS NOT NULL AND a.failed = 0), 0),
                COALESCE(SUM(a.track_id IS NOT NULL AND a.failed = 1), 0)
         FROM tracks t {VALID_ANALYSIS_JOIN}
         WHERE t.missing_since IS NULL"
    );
    let (total, analyzed, failed): (i64, i64, i64) = conn
        .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(db_error)?;
    let count = |value: i64| u32::try_from(value).unwrap_or(u32::MAX);
    Ok(AnalysisCounts {
        total: count(total),
        analyzed: count(analyzed),
        failed: count(failed),
    })
}

/// 解析の結果をすべて消す
pub fn delete_all(conn: &Connection) -> AppResult<()> {
    conn.execute("DELETE FROM track_analysis", [])
        .map_err(db_error)?;
    Ok(())
}

/// 曲ごとの解析の結果（解析できた曲だけ。AIシャッフルが使う）
pub fn load_features(
    conn: &Connection,
) -> AppResult<std::collections::HashMap<String, MusicFeatures>> {
    let sql = format!(
        "SELECT t.id, a.bpm, a.key_pitch_class, a.key_mode, a.pace FROM tracks t {VALID_ANALYSIS_JOIN}
         WHERE a.track_id IS NOT NULL AND a.failed = 0"
    );
    let mut stmt = conn.prepare(&sql).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            let features = MusicFeatures {
                bpm: row
                    .get::<_, Option<f64>>(1)?
                    .filter(|value| value.is_finite() && *value > 0.0),
                key: MusicKey::from_stored(row.get(2)?, row.get(3)?),
                pace: row
                    .get::<_, Option<f64>>(4)?
                    .filter(|value| value.is_finite()),
            };
            Ok((row.get::<_, String>(0)?, features))
        })
        .map_err(db_error)?;
    rows.collect::<rusqlite::Result<_>>().map_err(db_error)
}

/// 解析を続けていたか（次の起動で再開するか）
fn is_requested(conn: &Connection) -> AppResult<bool> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            [REQUESTED_META_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    Ok(value.as_deref() == Some("1"))
}

fn set_requested(conn: &Connection, requested: bool) -> AppResult<()> {
    if requested {
        conn.execute(
            "INSERT INTO app_meta (key, value) VALUES (?1, '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [REQUESTED_META_KEY],
        )
        .map_err(db_error)?;
    } else {
        conn.execute("DELETE FROM app_meta WHERE key = ?1", [REQUESTED_META_KEY])
            .map_err(db_error)?;
    }
    Ok(())
}

// ---- 実行 ----

/// 解析の状態（画面に出す）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MusicAnalysisStatus {
    /// このMacで解析を使えるか（macOS 27以降）
    pub available: bool,
    /// 解析を実行中か
    pub running: bool,
    /// 曲の数（見つからない曲を除く）
    pub total: u32,
    /// 解析した曲の数
    pub analyzed: u32,
    /// 解析できなかった曲の数
    pub failed: u32,
}

/// 解析の実行の状態（Tauriの状態として管理する。同時に実行できる解析は1つ）
#[derive(Default)]
pub struct MusicAnalysisState {
    running: AtomicBool,
    cancel: AtomicBool,
}

impl MusicAnalysisState {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// 今の解析の状態を求める
pub fn status(state: &AppState, analysis: &MusicAnalysisState) -> AppResult<MusicAnalysisStatus> {
    let counts = state.with_db(|db| count_analysis(db))?;
    Ok(MusicAnalysisStatus {
        available: is_available(),
        running: analysis.is_running(),
        total: counts.total,
        analyzed: counts.analyzed,
        failed: counts.failed,
    })
}

/// 解析をバックグラウンドで始める（実行中なら何もしない）
///
/// 止めるまでに終わらなければ、次の起動で再開する。
pub fn start(app: &AppHandle) -> AppResult<()> {
    if !is_available() {
        return Err(AppError::Validation(
            "このMacでは曲の解析を使えません（macOS 27以降が必要です）".to_string(),
        ));
    }
    let analysis = app.state::<MusicAnalysisState>();
    if analysis
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    analysis.cancel.store(false, Ordering::SeqCst);
    if let Err(e) = app
        .state::<AppState>()
        .with_db(|db| set_requested(db, true))
    {
        analysis.running.store(false, Ordering::SeqCst);
        return Err(e);
    }

    let worker_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("music-analysis".to_string())
        .spawn(move || {
            let app = worker_app;
            let analysis = app.state::<MusicAnalysisState>();
            let completed = run(&app);
            analysis.running.store(false, Ordering::SeqCst);
            // 最後まで解析した・止めた時は、次の起動で再開しない（アプリの終了で途切れた時だけ再開する）
            if completed.unwrap_or_else(|e| {
                log::warn!("Music analysis stopped with an error: {}", e);
                true
            }) && let Err(e) = app
                .state::<AppState>()
                .with_db(|db| set_requested(db, false))
            {
                log::warn!("Failed to clear the music analysis request: {}", e);
            }
            emit_status(&app);
        });
    if let Err(e) = spawned {
        analysis.running.store(false, Ordering::SeqCst);
        return Err(AppError::Io(format!(
            "曲の解析を始められませんでした: {}",
            e
        )));
    }
    emit_status(app);
    Ok(())
}

/// 実行中の解析を止める（解析中の曲が終わってから止まる。実行中でなければ何もしない）
pub fn stop(app: &AppHandle) -> AppResult<()> {
    let analysis = app.state::<MusicAnalysisState>();
    if analysis.is_running() {
        analysis.cancel.store(true, Ordering::SeqCst);
    }
    // 実行中でなくても、次の起動で再開しないようにする
    app.state::<AppState>()
        .with_db(|db| set_requested(db, false))
}

/// 前回の起動で解析が途中だったら、続きから再開する（起動時に呼ぶ）
pub fn resume(app: &AppHandle) {
    if !is_available() {
        return;
    }
    match app.state::<AppState>().with_db(|db| is_requested(db)) {
        Ok(true) => {
            log::info!("Resuming the music analysis");
            if let Err(e) = start(app) {
                log::warn!("Failed to resume the music analysis: {}", e);
            }
        }
        Ok(false) => {}
        Err(e) => log::warn!("Failed to read the music analysis request: {}", e),
    }
}

/// 解析していない曲を、1曲ずつ解析して保存する
///
/// @returns 最後まで解析したか、止めたか（true）。解析を使えなくなった時はfalse
fn run(app: &AppHandle) -> AppResult<bool> {
    let analysis = app.state::<MusicAnalysisState>();
    analyze_pending(
        &app.state::<AppState>(),
        || analysis.cancel.load(Ordering::SeqCst),
        analyze_file,
        || emit_status(app),
    )
}

/// 解析していない曲を、1曲ずつ解析して保存する（`run`の本体。解析する関数と知らせ方を受け取る）
///
/// 解析には何時間もかかることがあり、その間にライブラリが変わる（音楽フォルダの変更でライブラリを
/// 置き換える・インポート・曲を外す・場所を付け替えるなど）。そのため、解析する曲の一覧は、読んだ時点の
/// ものとして扱う:
/// - 1曲ごとに、解析する前に、まだ解析していない曲のままかを確かめ（`is_still_pending`）、変わった曲は
///   飛ばす（外した曲を解析し続けない）。保存する前にも確かめ、解析している間に変わった曲（場所を
///   付け替えたなど）の結果は捨てる（古いファイルの結果を、今のファイルのものとして残さない）
/// - 一覧を終えたら、解析していない曲を読み直して続ける（解析の途中に読み込んだ曲・場所を付け替えた曲も
///   解析する）。読めなかったファイルは、この実行では、同じパス・長さのまま繰り返さない
///
/// @param is_cancelled - 止めるのを頼まれたか（1曲ごとに確かめる）
/// @param analyze - 1曲を解析する関数（解析を使えない時はNone）
/// @param on_saved - 1曲の結果を保存した後に呼ぶ（画面へ知らせる）
/// @returns 最後まで解析したか、止めたか（true）。解析を使えなくなった時はfalse
fn analyze_pending(
    state: &AppState,
    is_cancelled: impl Fn() -> bool,
    mut analyze: impl FnMut(&Path) -> Option<AnalyzeOutcome>,
    mut on_saved: impl FnMut(),
) -> AppResult<bool> {
    // この実行で扱った曲（ID・パス・長さ。読めなかったファイルを、一覧を読み直すたびに試さない）
    let mut attempted: HashSet<PendingTrack> = HashSet::new();
    loop {
        let tracks: Vec<PendingTrack> = state
            .with_db(|db| pending_tracks(db))?
            .into_iter()
            .filter(|track| !attempted.contains(track))
            .collect();
        if tracks.is_empty() {
            log::info!("Music analysis finished");
            return Ok(true);
        }
        log::info!("Analyzing {} tracks", tracks.len());

        for track in tracks {
            if is_cancelled() {
                log::info!("Music analysis was stopped");
                return Ok(true);
            }
            // 一覧を読んだ後に変わった曲は飛ばす（今の状態は、一覧を読み直した時に拾う）
            if !state.with_db(|db| is_still_pending(db, &track))? {
                continue;
            }
            attempted.insert(track.clone());
            // 見つからない・読めないファイル（NASの切断など）は記録せず、次の解析で読み直す
            let path = Path::new(&track.file_path);
            if !path.is_file() {
                log::debug!("Skipping a file that cannot be read: {}", track.file_path);
                continue;
            }
            let Some(outcome) = analyze(path) else {
                return Ok(false);
            };
            if !matches!(outcome, AnalyzeOutcome::Analyzed(_)) {
                log::info!("Could not analyze {}: {:?}", track.file_path, outcome);
            }
            // 解析している間に変わった曲（場所を付け替えた・ライブラリから外したなど）の結果は、今の
            // ファイルのものでないため捨てる。確かめと保存は、DBのロックの中で続けて行う
            let saved = state.with_db(|db| {
                if !is_still_pending(db, &track)? {
                    return Ok(false);
                }
                save_outcome(db, &track.id, track.duration, &outcome)?;
                Ok(true)
            })?;
            if saved {
                on_saved();
            } else {
                log::debug!(
                    "Discarding the analysis of a track that changed while being analyzed: {}",
                    track.file_path
                );
            }
        }
    }
}

/// 今の状態を画面へ知らせる
fn emit_status(app: &AppHandle) {
    let status = match status(&app.state::<AppState>(), &app.state::<MusicAnalysisState>()) {
        Ok(status) => status,
        Err(e) => {
            log::warn!("Failed to read the music analysis status: {}", e);
            return;
        }
    };
    if let Err(e) = MusicAnalysisProgress(status).emit(app) {
        log::warn!("Failed to emit the music analysis progress event: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::device_manifest::test_support::TempDir;

    fn insert_track(conn: &Connection, id: &str, duration: i32, play_count: i32) {
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, duration, play_count)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                format!("/music/{id}.flac"),
                format!("{id}.flac"),
                duration,
                play_count
            ],
        )
        .unwrap();
    }

    fn key(pitch_class: u8, minor: bool) -> MusicKey {
        MusicKey { pitch_class, minor }
    }

    #[test]
    fn test_camelot_number() {
        // C major = 8B、A minor = 8A（平行調は同じ番号）、G major = 9B、F major = 7B
        assert_eq!(key(0, false).camelot_number(), 8);
        assert_eq!(key(9, true).camelot_number(), 8);
        assert_eq!(key(7, false).camelot_number(), 9);
        assert_eq!(key(5, false).camelot_number(), 7);
        // F# major = 2B、B major = 1B（12の次は1）
        assert_eq!(key(6, false).camelot_number(), 2);
        assert_eq!(key(11, false).camelot_number(), 1);
    }

    #[test]
    fn test_camelot_distance_wraps_around() {
        assert_eq!(key(0, false).camelot_distance(key(9, true)), 0);
        assert_eq!(key(0, false).camelot_distance(key(7, false)), 1);
        // 1Bと12B（E major = 12B）は隣
        assert_eq!(key(11, false).camelot_distance(key(4, false)), 1);
        // C major（8）とF# major（2）は、いちばん遠い
        assert_eq!(key(0, false).camelot_distance(key(6, false)), 6);
    }

    #[test]
    fn test_music_key_from_stored_rejects_out_of_range() {
        assert_eq!(MusicKey::from_stored(Some(3), Some(1)), Some(key(3, true)));
        assert_eq!(MusicKey::from_stored(Some(12), Some(0)), None);
        assert_eq!(MusicKey::from_stored(Some(-1), Some(0)), None);
        assert_eq!(MusicKey::from_stored(Some(3), Some(2)), None);
        assert_eq!(MusicKey::from_stored(None, Some(0)), None);
    }

    #[test]
    fn test_pending_tracks_and_counts() {
        let conn = open_in_memory();
        insert_track(&conn, "a", 200, 1);
        insert_track(&conn, "b", 180, 5);
        insert_track(&conn, "c", 240, 0);

        // よく再生する曲から
        let pending: Vec<String> = pending_tracks(&conn)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(pending, vec!["b", "a", "c"]);

        let features = MusicFeatures {
            bpm: Some(120.0),
            key: Some(key(0, false)),
            pace: Some(0.5),
        };
        save_outcome(&conn, "b", Some(180), &AnalyzeOutcome::Analyzed(features)).unwrap();
        save_outcome(&conn, "c", Some(240), &AnalyzeOutcome::Unsupported).unwrap();

        let pending: Vec<String> = pending_tracks(&conn)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(pending, vec!["a"]);
        assert_eq!(
            count_analysis(&conn).unwrap(),
            AnalysisCounts {
                total: 3,
                analyzed: 1,
                failed: 1
            }
        );
        let loaded = load_features(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["b"], features);
    }

    #[test]
    fn test_changed_duration_needs_analysis_again() {
        let conn = open_in_memory();
        insert_track(&conn, "a", 200, 0);
        save_outcome(&conn, "a", Some(200), &AnalyzeOutcome::Failed).unwrap();
        assert!(pending_tracks(&conn).unwrap().is_empty());

        // ファイルを差し替えて長さが変わった
        conn.execute("UPDATE tracks SET duration = 210 WHERE id = 'a'", [])
            .unwrap();
        assert_eq!(pending_tracks(&conn).unwrap().len(), 1);
        assert_eq!(count_analysis(&conn).unwrap().failed, 0);
    }

    #[test]
    fn test_missing_tracks_are_not_counted() {
        let conn = open_in_memory();
        insert_track(&conn, "a", 200, 0);
        insert_track(&conn, "b", 200, 0);
        conn.execute(
            "UPDATE tracks SET missing_since = datetime('now') WHERE id = 'b'",
            [],
        )
        .unwrap();
        assert_eq!(pending_tracks(&conn).unwrap().len(), 1);
        assert_eq!(count_analysis(&conn).unwrap().total, 1);
    }

    #[test]
    fn test_saving_for_a_removed_track_does_nothing() {
        let conn = open_in_memory();
        save_outcome(&conn, "gone", Some(100), &AnalyzeOutcome::Failed).unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM track_analysis", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn test_analysis_is_removed_with_the_track() {
        let conn = open_in_memory();
        insert_track(&conn, "a", 200, 0);
        save_outcome(&conn, "a", Some(200), &AnalyzeOutcome::Failed).unwrap();
        conn.execute("DELETE FROM tracks WHERE id = 'a'", [])
            .unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM track_analysis", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn test_requested_flag() {
        let conn = open_in_memory();
        assert!(!is_requested(&conn).unwrap());
        set_requested(&conn, true).unwrap();
        assert!(is_requested(&conn).unwrap());
        set_requested(&conn, false).unwrap();
        assert!(!is_requested(&conn).unwrap());
    }

    const ANALYZED: AnalyzeOutcome = AnalyzeOutcome::Analyzed(MusicFeatures {
        bpm: Some(120.0),
        key: None,
        pace: None,
    });

    /// 曲のファイルを作り、曲として登録する（ファイルのパスを返す）
    fn insert_file_track(state: &AppState, dir: &Path, id: &str, play_count: i32) -> String {
        let path = dir.join(format!("{id}.flac"));
        std::fs::write(&path, b"audio").unwrap();
        let path = path.to_string_lossy().into_owned();
        state
            .with_db(|db| {
                db.execute(
                    "INSERT INTO tracks (id, file_path, file_name, duration, play_count)
                     VALUES (?1, ?2, ?3, 200, ?4)",
                    params![id, path, format!("{id}.flac"), play_count],
                )
                .map_err(db_error)
            })
            .unwrap();
        path
    }

    /// 解析の結果がある曲のID（IDの順）
    fn analyzed_ids(state: &AppState) -> Vec<String> {
        state
            .with_db(|db| {
                let mut stmt = db
                    .prepare("SELECT track_id FROM track_analysis ORDER BY track_id")
                    .map_err(db_error)?;
                let ids = stmt.query_map([], |row| row.get(0)).map_err(db_error)?;
                ids.collect::<rusqlite::Result<_>>().map_err(db_error)
            })
            .unwrap()
    }

    #[test]
    fn test_analysis_follows_library_changes_during_the_run() {
        let dir = TempDir::new();
        let state = AppState::new(open_in_memory());
        let first = insert_file_track(&state, dir.path(), "first", 9);
        insert_file_track(&state, dir.path(), "removed", 5);
        insert_file_track(&state, dir.path(), "moved", 1);
        let moved_path = dir.path().join("elsewhere.flac");
        std::fs::write(&moved_path, b"audio").unwrap();
        let moved_path = moved_path.to_string_lossy().into_owned();
        let imported_path = dir
            .path()
            .join("imported.flac")
            .to_string_lossy()
            .into_owned();

        let mut analyzed = Vec::new();
        let completed = analyze_pending(
            &state,
            || false,
            |path| {
                let path = path.to_string_lossy().into_owned();
                // 1曲目を解析している間に、ライブラリが変わる（音楽フォルダの変更で曲を外す・場所を
                // 付け替える・新しい曲を読み込む）。外した曲のファイルは残っている
                if path == first {
                    state
                        .with_db(|db| {
                            db.execute("DELETE FROM tracks WHERE id = 'removed'", [])
                                .map_err(db_error)?;
                            db.execute(
                                "UPDATE tracks SET file_path = ?1 WHERE id = 'moved'",
                                [&moved_path],
                            )
                            .map_err(db_error)
                        })
                        .unwrap();
                    insert_file_track(&state, dir.path(), "imported", 0);
                }
                analyzed.push(path);
                Some(ANALYZED)
            },
            || {},
        )
        .unwrap();

        assert!(completed);
        // 外した曲は解析せず、付け替えた曲は新しい場所で、読み込んだ曲も続けて解析する
        assert_eq!(analyzed, vec![first, moved_path, imported_path]);
        assert_eq!(analyzed_ids(&state), vec!["first", "imported", "moved"]);
    }

    #[test]
    fn test_analysis_of_a_track_moved_while_analyzing_is_discarded() {
        let dir = TempDir::new();
        let state = AppState::new(open_in_memory());
        let old_path = insert_file_track(&state, dir.path(), "track", 0);
        let new_path = dir.path().join("moved.flac");
        std::fs::write(&new_path, b"audio").unwrap();
        let new_path = new_path.to_string_lossy().into_owned();

        let mut analyzed = Vec::new();
        let completed = analyze_pending(
            &state,
            || false,
            |path| {
                let path = path.to_string_lossy().into_owned();
                // 古い場所のファイルを解析している間に、曲の場所を付け替える（長さは同じ）
                if path == old_path {
                    state
                        .with_db(|db| {
                            db.execute(
                                "UPDATE tracks SET file_path = ?1 WHERE id = 'track'",
                                [&new_path],
                            )
                            .map_err(db_error)
                        })
                        .unwrap();
                }
                // どちらのファイルを解析したかを、BPMで見分ける
                let bpm = if path == old_path { 100.0 } else { 120.0 };
                analyzed.push(path);
                Some(AnalyzeOutcome::Analyzed(MusicFeatures {
                    bpm: Some(bpm),
                    key: None,
                    pace: None,
                }))
            },
            || {},
        )
        .unwrap();

        assert!(completed);
        // 古い場所の結果は捨て、新しい場所で解析し直す
        assert_eq!(analyzed, vec![old_path, new_path]);
        let features = state.with_db(|db| load_features(db)).unwrap();
        assert_eq!(features["track"].bpm, Some(120.0));
    }

    #[test]
    fn test_unreadable_files_are_tried_once_per_run() {
        let dir = TempDir::new();
        let state = AppState::new(open_in_memory());
        let gone = insert_file_track(&state, dir.path(), "gone", 1);
        std::fs::remove_file(&gone).unwrap();
        insert_file_track(&state, dir.path(), "present", 0);

        let mut calls = 0;
        let completed = analyze_pending(
            &state,
            || false,
            |_| {
                calls += 1;
                Some(ANALYZED)
            },
            || {},
        )
        .unwrap();

        // 一覧を読み直しても、読めないファイルを繰り返さずに終わる
        assert!(completed);
        assert_eq!(calls, 1);
        assert_eq!(analyzed_ids(&state), vec!["present"]);
        // 読めないファイルは記録しない（次の解析で読み直す）
        let pending = state.with_db(|db| pending_tracks(db)).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "gone");
    }

    #[test]
    fn test_analysis_stops_when_cancelled_or_unavailable() {
        let dir = TempDir::new();
        let state = AppState::new(open_in_memory());
        insert_file_track(&state, dir.path(), "a", 0);

        // 止めるのを頼まれていれば、解析せずに終える（止めた扱い）
        assert!(analyze_pending(&state, || true, |_| Some(ANALYZED), || {}).unwrap());
        assert!(analyzed_ids(&state).is_empty());
        // 解析を使えなくなったら、やめる（次の起動で再開する）
        assert!(!analyze_pending(&state, || false, |_| None, || {}).unwrap());
        assert!(analyzed_ids(&state).is_empty());
    }

    /// 実際にMusicUnderstandingで解析する（このMacで使える時だけ。数秒〜十数秒かかるため、
    /// 普段のテストでは動かさない: `cargo test music_analysis -- --ignored`）
    #[test]
    #[ignore = "runs the on-device model and takes several seconds"]
    fn test_analyze_generated_file() {
        if !is_available() {
            eprintln!("Music analysis is not available on this Mac; skipping");
            return;
        }
        let dir = std::env::temp_dir().join(format!("muspice-analysis-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("click.wav");
        std::fs::write(&path, click_track_wav(120.0, 20.0)).unwrap();

        let outcome = analyze_file(&path).expect("analysis should be available");
        std::fs::remove_dir_all(&dir).unwrap();
        let AnalyzeOutcome::Analyzed(features) = outcome else {
            panic!("expected the file to be analyzed: {outcome:?}");
        };
        let bpm = features.bpm.expect("a tempo should be detected");
        // 倍・半分のテンポと取られることがあるため、どれかに近ければよい
        assert!(
            [60.0, 120.0, 240.0]
                .iter()
                .any(|expected| (bpm - expected).abs() < 3.0),
            "unexpected tempo: {bpm}"
        );

        // 音声として読めないファイルは、解析できない形式として扱う
        let text =
            std::env::temp_dir().join(format!("muspice-analysis-{}.mp3", uuid::Uuid::new_v4()));
        std::fs::write(&text, b"not audio").unwrap();
        let outcome = analyze_file(&text);
        std::fs::remove_file(&text).unwrap();
        assert_eq!(outcome, Some(AnalyzeOutcome::Unsupported));
    }

    /// 決まったテンポでクリックが鳴る、モノラル・16bitのWAV
    fn click_track_wav(bpm: f64, seconds: f64) -> Vec<u8> {
        let rate = 44_100u32;
        let samples = (f64::from(rate) * seconds) as usize;
        let beat = (f64::from(rate) * 60.0 / bpm) as usize;
        let mut data = Vec::with_capacity(samples * 2);
        for index in 0..samples {
            let position = index % beat;
            // 拍の頭に短い音（1kHzを減衰させる）
            let value = if position < 2_000 {
                let envelope = 1.0 - position as f64 / 2_000.0;
                (2.0 * std::f64::consts::PI * 1_000.0 * index as f64 / f64::from(rate)).sin()
                    * envelope
                    * 0.8
            } else {
                0.0
            };
            data.extend_from_slice(&((value * f64::from(i16::MAX)) as i16).to_le_bytes());
        }
        let mut wav = Vec::with_capacity(44 + data.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // モノラル
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        wav
    }

    #[test]
    fn test_delete_all() {
        let conn = open_in_memory();
        insert_track(&conn, "a", 200, 0);
        save_outcome(&conn, "a", Some(200), &AnalyzeOutcome::Failed).unwrap();
        delete_all(&conn).unwrap();
        assert_eq!(count_analysis(&conn).unwrap().failed, 0);
        assert_eq!(pending_tracks(&conn).unwrap().len(), 1);
    }
}
