//! ラウドネスの解析の実行（対象の決め方・アルバムごとの解析・タグへの書き込み）
//!
//! ReplayGainのタグがない曲のラウドネスを測り、ゲインとピークをファイルのタグへ書き込んで、
//! データベースに反映する（ADR-053）。解析は利用者が始め、バックグラウンドで進む。
//!
//! - 解析は、まとまり（アルバムか、アルバムのない1曲）ごとに行う。アルバムは、アルバムの一覧と同じく、
//!   アルバム名とアルバムアーティスト（なければ曲のアーティスト）が同じ曲
//! - アルバムのゲインは、ライブラリにあるそのアルバムの全曲から求めるため、アルバムの1曲でも解析が
//!   要る場合は、全曲を読む。アルバムのゲインとピークは、アルバムの全曲に同じ値を書く
//! - 「タグのない曲」の解析では、すでにある曲のゲインは変えない（ない曲にだけ書く）。選んだ曲の解析では、
//!   選んだ曲のゲインを解析し直す
//! - まとまりは、いくつかを並行して解析する。中断すると、解析の途中のまとまりには何も書かない
//!   （アルバムの一部の曲にだけ書かれた状態を作らない）。次に解析すると、残りの曲から続く

use crate::error::{AppError, AppResult};
use crate::library::{get_file_modified_at, get_file_size, to_count};
use crate::loudness::{Measurement, integrated_loudness, measure_file, replay_gain_db};
use crate::metadata::update_file_replay_gain;
use crate::models::{ReplayGain, Track};
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::thread;

/// 並行して解析するまとまりの数の上限（再生・ほかの操作のために、コアを使い切らない）
const MAX_WORKERS: usize = 4;

/// 今の値と同じとみなす、ゲイン（dB）・ピークの差（タグに書く桁より小さい差では、書き直さない）
const GAIN_TOLERANCE_DB: f64 = 0.005;
const PEAK_TOLERANCE: f64 = 0.000_001;

/// 解析の実行の状態（Tauriの状態として管理する。同時に実行できる解析は1つ）
#[derive(Default)]
pub struct LoudnessScanState {
    running: AtomicBool,
    cancel: AtomicBool,
    /// 読み終わった曲の数と、読む曲の総数（実行中の進み具合。画面を開き直した時に使う）
    current: AtomicU32,
    total: AtomicU32,
}

impl LoudnessScanState {
    /// 解析を始める。ほかの解析が実行中ならNone
    ///
    /// 返したガードを破棄すると、実行中の印が外れる。
    pub fn begin(&self) -> Option<LoudnessScanGuard<'_>> {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        self.cancel.store(false, Ordering::SeqCst);
        self.current.store(0, Ordering::SeqCst);
        self.total.store(0, Ordering::SeqCst);
        Some(LoudnessScanGuard { state: self })
    }

    /// 実行中の解析を中断させる（実行中でなければ何もしない）
    pub fn request_cancel(&self) {
        if self.running.load(Ordering::SeqCst) {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// 中断の要求（曲を読んでいる途中で確認する）
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }

    /// 進み具合を記録する
    pub fn set_progress(&self, current: u32, total: u32) {
        self.current.store(current, Ordering::SeqCst);
        self.total.store(total, Ordering::SeqCst);
    }

    /// 実行中か・読み終わった曲の数・読む曲の総数
    pub fn progress(&self) -> (bool, u32, u32) {
        (
            self.running.load(Ordering::SeqCst),
            self.current.load(Ordering::SeqCst),
            self.total.load(Ordering::SeqCst),
        )
    }
}

/// 解析の実行中の印（破棄すると外れる）
pub struct LoudnessScanGuard<'a> {
    state: &'a LoudnessScanState,
}

impl Drop for LoudnessScanGuard<'_> {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::SeqCst);
    }
}

/// 解析の対象
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanScope {
    /// ライブラリの、ゲインのない曲（すでにあるゲインは変えない）
    Missing,
    /// 選んだ曲（ゲインがあっても、解析し直す）
    Tracks(HashSet<String>),
}

/// 解析する曲
#[derive(Debug, Clone, PartialEq)]
pub struct ScanTrack {
    pub id: String,
    pub file_path: String,
    /// 今のゲインとピーク
    pub replay_gain: ReplayGain,
    /// 曲のゲインとピークを書くか（書かない曲は、アルバムのゲインのために読むだけ）
    pub write_track_gain: bool,
}

/// 解析のまとまり（アルバムか、アルバムのない1曲）
#[derive(Debug, Clone, PartialEq)]
pub struct ScanUnit {
    pub tracks: Vec<ScanTrack>,
    /// アルバムか（アルバムのゲインとピークを、全曲に書く）
    pub is_album: bool,
}

/// 解析の結果
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessScanResult {
    /// ゲインを書き込んだ曲の数
    pub written_count: u32,
    /// 解析したが、今の値と同じだったため書き込まなかった曲の数
    pub unchanged_count: u32,
    /// 解析できなかった・書き込めなかった曲の数
    pub error_count: u32,
    /// 解析できなかった・書き込めなかった理由（`場所: 理由`）
    pub errors: Vec<String>,
    /// 途中で中断したか
    pub cancelled: bool,
}

/// 解析の進捗（読み終わった曲）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanProgress {
    pub current: u32,
    pub total: u32,
    pub current_file: String,
}

/// 曲の、アルバムのまとまりのキー（アルバムのタグがない曲はNone）
fn album_key(track: &Track) -> Option<(String, Option<String>)> {
    let album = track.album.as_deref().filter(|album| !album.is_empty())?;
    let artist = track.album_artist.clone().or_else(|| track.artist.clone());
    Some((album.to_string(), artist))
}

/// 解析するまとまりを決める
///
/// ファイルが見つからない曲は、対象にしない（アルバムのゲインは、見つかる曲から求める）。
/// @param tracks - ライブラリの全曲
pub fn plan_units(tracks: &[Track], scope: &ScanScope) -> Vec<ScanUnit> {
    let is_target = |track: &Track| match scope {
        ScanScope::Missing => track.replay_gain.track_gain.is_none(),
        ScanScope::Tracks(ids) => ids.contains(&track.id),
    };
    let scan_track = |track: &Track| ScanTrack {
        id: track.id.clone(),
        file_path: track.file_path.clone(),
        replay_gain: track.replay_gain.clone(),
        write_track_gain: is_target(track),
    };

    let mut units: Vec<ScanUnit> = Vec::new();
    let mut album_units: HashMap<(String, Option<String>), usize> = HashMap::new();
    for track in tracks.iter().filter(|track| !track.is_missing) {
        match album_key(track) {
            Some(key) => {
                let index = *album_units.entry(key).or_insert_with(|| {
                    units.push(ScanUnit {
                        tracks: Vec::new(),
                        is_album: true,
                    });
                    units.len() - 1
                });
                units[index].tracks.push(scan_track(track));
            }
            None => units.push(ScanUnit {
                tracks: vec![scan_track(track)],
                is_album: false,
            }),
        }
    }

    units.retain(|unit| {
        let wanted = unit.tracks.iter().any(|track| track.write_track_gain);
        match scope {
            // アルバムのゲインが欠けている曲があるアルバムも、解析する
            ScanScope::Missing => {
                wanted
                    || (unit.is_album
                        && unit
                            .tracks
                            .iter()
                            .any(|track| track.replay_gain.album_gain.is_none()))
            }
            ScanScope::Tracks(_) => wanted,
        }
    });
    units
}

/// まとまりの曲に書く値（曲ごと。渡した順）を、測った結果から決める
///
/// @param measurements - 曲ごとの測った結果（測れなかった曲はNone。その曲には書かない）
fn unit_values(unit: &ScanUnit, measurements: &[Option<Measurement>]) -> Vec<Option<ReplayGain>> {
    let measured: Vec<&Measurement> = measurements.iter().flatten().collect();
    let album = unit.is_album.then(|| {
        (
            replay_gain_db(integrated_loudness(measured.iter().copied())),
            measured
                .iter()
                .map(|measurement| measurement.peak)
                .fold(0.0, f64::max),
        )
    });

    unit.tracks
        .iter()
        .zip(measurements)
        .map(|(track, measurement)| {
            let measurement = measurement.as_ref()?;
            let (track_gain, track_peak) = if track.write_track_gain {
                (
                    Some(replay_gain_db(measurement.loudness())),
                    Some(measurement.peak),
                )
            } else {
                (None, None)
            };
            Some(ReplayGain {
                track_gain,
                track_peak,
                album_gain: album.map(|(gain, _)| gain),
                album_peak: album.map(|(_, peak)| peak),
            })
        })
        .collect()
}

/// 書く値が、今の値と同じか（書く項目だけを比べる）
fn is_unchanged(current: &ReplayGain, values: &ReplayGain) -> bool {
    let same = |current: Option<f64>, value: Option<f64>, tolerance: f64| match (current, value) {
        (_, None) => true,
        (Some(current), Some(value)) => (current - value).abs() < tolerance,
        (None, Some(_)) => false,
    };
    same(current.track_gain, values.track_gain, GAIN_TOLERANCE_DB)
        && same(current.track_peak, values.track_peak, PEAK_TOLERANCE)
        && same(current.album_gain, values.album_gain, GAIN_TOLERANCE_DB)
        && same(current.album_peak, values.album_peak, PEAK_TOLERANCE)
}

/// 並行して解析するまとまりの数
fn worker_count(unit_count: usize) -> usize {
    let cores = thread::available_parallelism().map_or(1, |cores| cores.get());
    (cores / 2).clamp(1, MAX_WORKERS).min(unit_count.max(1))
}

/// 解析の途中の集計（並行して解析するスレッドで共有する）
struct Shared<'a> {
    state: &'a AppState,
    cancelled: &'a AtomicBool,
    on_progress: &'a (dyn Fn(ScanProgress) + Sync),
    total: u32,
    done: AtomicU32,
    result: Mutex<LoudnessScanResult>,
}

impl Shared<'_> {
    fn update(&self, update: impl FnOnce(&mut LoudnessScanResult)) {
        // 守るのは集計だけのため、途中でパニックしたスレッドがあっても続ける
        let mut result = self
            .result
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut result);
    }

    fn fail(&self, file_path: &str, error: &AppError) {
        self.update(|result| {
            result.error_count += 1;
            result.errors.push(format!("{}: {}", file_path, error));
        });
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// まとまりを解析して、ゲインとピークをファイルのタグとデータベースへ書き込む
///
/// 解析できなかった・書き込めなかった曲は、結果の`errors`へ理由を入れて残りを続ける。
/// @param cancelled - trueになったら、解析の途中のまとまりを捨てて終わる
/// @param on_progress - 曲を読み終わるたびに呼ぶ
pub fn scan_units(
    state: &AppState,
    units: &[ScanUnit],
    cancelled: &AtomicBool,
    on_progress: &(dyn Fn(ScanProgress) + Sync),
) -> AppResult<LoudnessScanResult> {
    let total = to_count(units.iter().map(|unit| unit.tracks.len()).sum())?;
    let shared = Shared {
        state,
        cancelled,
        on_progress,
        total,
        done: AtomicU32::new(0),
        result: Mutex::new(LoudnessScanResult::default()),
    };
    let next = AtomicUsize::new(0);

    thread::scope(|scope| {
        for _ in 0..worker_count(units.len()) {
            scope.spawn(|| {
                while !shared.is_cancelled() {
                    let Some(unit) = units.get(next.fetch_add(1, Ordering::Relaxed)) else {
                        break;
                    };
                    scan_unit(&shared, unit);
                }
            });
        }
    });

    let mut result = shared
        .result
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    result.cancelled = cancelled.load(Ordering::Relaxed);
    Ok(result)
}

/// 1つのまとまりを解析して書き込む
fn scan_unit(shared: &Shared<'_>, unit: &ScanUnit) {
    // 1. まとまりの全曲を読んで、ラウドネスを測る
    let mut measurements: Vec<Option<Measurement>> = Vec::with_capacity(unit.tracks.len());
    for track in &unit.tracks {
        let path = Path::new(&track.file_path);
        match measure_file(path, shared.cancelled) {
            Ok(Some(measurement)) => measurements.push(Some(measurement)),
            // 中断: このまとまりには、何も書かない
            Ok(None) => return,
            Err(e) => {
                shared.fail(&track.file_path, &e);
                measurements.push(None);
            }
        }
        (shared.on_progress)(ScanProgress {
            current: shared.done.fetch_add(1, Ordering::Relaxed) + 1,
            total: shared.total,
            current_file: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("不明なファイル")
                .to_string(),
        });
    }
    if shared.is_cancelled() {
        return;
    }

    // 2. 値を決めて、ファイルのタグとデータベースへ書く（書き始めたまとまりは、最後まで書く）
    for (track, values) in unit.tracks.iter().zip(unit_values(unit, &measurements)) {
        let Some(values) = values else { continue };
        if is_unchanged(&track.replay_gain, &values) {
            shared.update(|result| result.unchanged_count += 1);
            continue;
        }
        match write_track(shared.state, track, &values) {
            Ok(true) => shared.update(|result| result.written_count += 1),
            // ファイルには、すでに同じ値が書かれていた（ピークを書けない形式など）
            Ok(false) => shared.update(|result| result.unchanged_count += 1),
            Err(e) => shared.fail(&track.file_path, &e),
        }
    }
}

/// 1曲のゲインとピークを、ファイルのタグへ書き込み、データベースに反映する
///
/// @returns ファイルへ書き込んだか（すでに同じ値が書かれていれば、書き込まない）
fn write_track(state: &AppState, track: &ScanTrack, values: &ReplayGain) -> AppResult<bool> {
    let path = Path::new(&track.file_path);
    // データベースには、書き込んだ後にファイルから読める値を記録する（ファイルのタグを正とする）
    let (in_file, written) = update_file_replay_gain(path, values)?;
    if !written && in_file == track.replay_gain {
        return Ok(false);
    }
    let file_state = written.then(|| (get_file_size(path).ok(), get_file_modified_at(path)));

    state.with_db(|db| {
        crate::repository::set_track_replay_gain(db, &track.id, &in_file)?;
        // 書き込みでファイルが変わったことを記録し、再スキャンで読み直さないようにする
        match file_state {
            Some((Some(file_size), file_modified_at)) => {
                crate::repository::set_track_file_state(db, &track.id, file_size, file_modified_at)
            }
            _ => Ok(()),
        }
    })?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(
        id: &str,
        album: Option<&str>,
        track_gain: Option<f64>,
        album_gain: Option<f64>,
    ) -> Track {
        Track {
            id: id.to_string(),
            file_path: format!("/music/{id}.flac"),
            file_name: format!("{id}.flac"),
            title: Some(id.to_string()),
            artist: Some("Artist".to_string()),
            album: album.map(str::to_string),
            album_artist: None,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration: Some(200),
            file_size: 1,
            format: "flac".to_string(),
            bitrate: None,
            sample_rate: None,
            is_favorite: false,
            rating: 0,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            replay_gain: ReplayGain {
                track_gain,
                track_peak: track_gain.map(|_| 0.9),
                album_gain,
                album_peak: album_gain.map(|_| 0.95),
            },
            sort_tags: Default::default(),
            is_missing: false,
        }
    }

    /// まとまりごとの、(曲のID, 曲のゲインを書くか)
    fn summary(units: &[ScanUnit]) -> Vec<(bool, Vec<(&str, bool)>)> {
        units
            .iter()
            .map(|unit| {
                (
                    unit.is_album,
                    unit.tracks
                        .iter()
                        .map(|track| (track.id.as_str(), track.write_track_gain))
                        .collect(),
                )
            })
            .collect()
    }

    fn selected(ids: &[&str]) -> ScanScope {
        ScanScope::Tracks(ids.iter().map(|id| id.to_string()).collect())
    }

    #[test]
    fn test_missing_scope_scans_albums_that_lack_any_gain() {
        let tracks = [
            // 1曲だけゲインがないアルバム: 全曲を読み、ない曲にだけ曲のゲインを書く
            track("a1", Some("A"), Some(-6.0), Some(-7.0)),
            track("a2", Some("A"), None, None),
            // すべてそろっているアルバム: 解析しない
            track("b1", Some("B"), Some(-6.0), Some(-7.0)),
            track("b2", Some("B"), Some(-5.0), Some(-7.0)),
            // 曲のゲインはあるが、アルバムのゲインが欠けているアルバム: アルバムのゲインのために読む
            track("c1", Some("C"), Some(-6.0), None),
            track("c2", Some("C"), Some(-5.0), Some(-7.0)),
            // アルバムのない曲: ゲインのない曲だけ
            track("s1", None, None, None),
            track("s2", None, Some(-3.0), None),
        ];

        let units = plan_units(&tracks, &ScanScope::Missing);

        assert_eq!(
            summary(&units),
            [
                (true, vec![("a1", false), ("a2", true)]),
                (true, vec![("c1", false), ("c2", false)]),
                (false, vec![("s1", true)]),
            ]
        );
    }

    #[test]
    fn test_selected_scope_rescans_the_selected_tracks_with_their_albums() {
        let tracks = [
            track("a1", Some("A"), Some(-6.0), Some(-7.0)),
            track("a2", Some("A"), Some(-5.0), Some(-7.0)),
            track("b1", Some("B"), None, None),
            track("s1", None, Some(-3.0), None),
            track("s2", None, None, None),
        ];

        let units = plan_units(&tracks, &selected(&["a2", "s1", "unknown"]));

        // 選んだ曲は、ゲインがあっても解析し直す。アルバムのほかの曲は、アルバムのゲインのために読む
        assert_eq!(
            summary(&units),
            [
                (true, vec![("a1", false), ("a2", true)]),
                (false, vec![("s1", true)]),
            ]
        );
    }

    #[test]
    fn test_albums_are_grouped_like_the_album_list() {
        let mut compilation_1 = track("v1", Some("Hits"), None, None);
        compilation_1.album_artist = Some("Various".to_string());
        compilation_1.artist = Some("One".to_string());
        let mut compilation_2 = track("v2", Some("Hits"), None, None);
        compilation_2.album_artist = Some("Various".to_string());
        compilation_2.artist = Some("Two".to_string());
        // 同じ名前の、別のアーティストのアルバム
        let mut other = track("o1", Some("Hits"), None, None);
        other.artist = Some("Other".to_string());
        // ファイルが見つからない曲は、対象にしない
        let mut missing = track("m1", Some("Hits"), None, None);
        missing.album_artist = Some("Various".to_string());
        missing.is_missing = true;
        // アルバム名が空の曲は、アルバムのない曲として扱う
        let empty = track("e1", Some(""), None, None);

        let units = plan_units(
            &[compilation_1, other, compilation_2, missing, empty],
            &ScanScope::Missing,
        );

        assert_eq!(
            summary(&units),
            [
                (true, vec![("v1", true), ("v2", true)]),
                (true, vec![("o1", true)]),
                (false, vec![("e1", true)]),
            ]
        );
    }

    /// 実際のファイルを使う確認: 1kHzの正弦波（ステレオ）のWAVを作り、ライブラリに登録する
    struct Library {
        dir: crate::device_manifest::test_support::TempDir,
        state: AppState,
    }

    impl Library {
        fn new() -> Self {
            let conn = crate::db::open_in_memory();
            Self {
                dir: crate::device_manifest::test_support::TempDir::new(),
                state: AppState::new(conn),
            }
        }

        /// `dbfs`の大きさの曲を作って登録する（`album`がNoneなら、アルバムのない曲）
        fn add(&self, name: &str, album: Option<&str>, dbfs: f64) -> String {
            use std::f64::consts::PI;
            let path = self.dir.path().join(format!("{name}.wav"));
            let rate = 44_100u32;
            let amplitude = 10f64.powf(dbfs / 20.0) * f64::from(i16::MAX);
            let samples: Vec<i16> = (0..rate * 2)
                .flat_map(|frame| {
                    let value = (amplitude
                        * (2.0 * PI * 1000.0 * f64::from(frame) / f64::from(rate)).sin())
                        as i16;
                    [value, value]
                })
                .collect();
            crate::playback::test_audio::write_wav_samples(&path, rate, 2, &samples);

            let mut new_track = track(&uuid::Uuid::new_v4().to_string(), album, None, None);
            new_track.file_path = path.to_str().unwrap().to_string();
            new_track.file_name = format!("{name}.wav");
            new_track.file_size = get_file_size(&path).unwrap();
            self.state
                .with_db(|db| crate::repository::insert_track(db, &new_track, Some(1)))
                .unwrap();
            new_track.id
        }

        /// 再生エンジンのテスト用のファイルを写して登録する（アルバムのない曲）
        fn add_fixture(&self, name: &str) -> String {
            let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/playback");
            let path = self.dir.path().join(name);
            std::fs::copy(fixtures.join(name), &path).unwrap();
            let mut new_track = track(&uuid::Uuid::new_v4().to_string(), None, None, None);
            new_track.file_path = path.to_str().unwrap().to_string();
            new_track.file_name = name.to_string();
            self.state
                .with_db(|db| crate::repository::insert_track(db, &new_track, Some(1)))
                .unwrap();
            new_track.id
        }

        fn tracks(&self) -> Vec<Track> {
            self.state
                .with_db(|db| crate::repository::find_all_tracks(db))
                .unwrap()
        }

        fn find(&self, id: &str) -> Track {
            self.state
                .with_db(|db| crate::repository::find_track_by_id(db, id))
                .unwrap()
        }

        fn scan(
            &self,
            scope: &ScanScope,
            cancelled: bool,
        ) -> (LoudnessScanResult, Vec<ScanProgress>) {
            let units = plan_units(&self.tracks(), scope);
            let progress = Mutex::new(Vec::new());
            let result = scan_units(&self.state, &units, &AtomicBool::new(cancelled), &|event| {
                progress.lock().unwrap().push(event)
            })
            .unwrap();
            (result, progress.into_inner().unwrap())
        }
    }

    fn close(actual: Option<f64>, expected: f64) -> bool {
        actual.is_some_and(|actual| (actual - expected).abs() < 0.1)
    }

    #[test]
    fn test_scan_writes_gains_to_files_and_the_library() {
        let library = Library::new();
        let loud = library.add("loud", Some("Album"), -12.0);
        let quiet = library.add("quiet", Some("Album"), -30.0);
        let single = library.add("single", None, -20.0);
        let pending = || {
            library
                .state
                .with_db(|db| crate::repository::count_tracks_without_replay_gain(db))
                .unwrap()
        };
        assert_eq!(pending(), 3);

        let (result, progress) = library.scan(&ScanScope::Missing, false);

        assert_eq!(
            result,
            LoudnessScanResult {
                written_count: 3,
                ..LoudnessScanResult::default()
            }
        );
        // 曲を読み終わるたびに、進捗を知らせる
        assert_eq!(progress.len(), 3);
        assert!(progress.iter().all(|event| event.total == 3));
        assert_eq!(progress.iter().map(|event| event.current).max(), Some(3));

        // 曲のゲインは、-18 LUFSとの差。アルバムのゲインは、静かな曲をゲートで除いた平均（-12 LUFS）から
        let loud_track = library.find(&loud);
        assert!(close(loud_track.replay_gain.track_gain, -6.0));
        assert!(close(loud_track.replay_gain.track_peak, 0.2512));
        assert!(close(loud_track.replay_gain.album_gain, -6.0));
        let quiet_track = library.find(&quiet);
        assert!(close(quiet_track.replay_gain.track_gain, 12.0));
        assert_eq!(
            quiet_track.replay_gain.album_gain,
            loud_track.replay_gain.album_gain
        );
        assert_eq!(
            quiet_track.replay_gain.album_peak,
            loud_track.replay_gain.album_peak
        );
        // アルバムのない曲には、アルバムのゲインを書かない
        let single_track = library.find(&single);
        assert!(close(single_track.replay_gain.track_gain, 2.0));
        assert_eq!(single_track.replay_gain.album_gain, None);

        // ファイルのタグにも同じ値があり、ファイルの大きさを記録し直している（再スキャンで読み直さない）
        for track in [&loud_track, &quiet_track, &single_track] {
            let path = Path::new(&track.file_path);
            assert_eq!(
                crate::metadata::read_file_replay_gain(path).unwrap(),
                track.replay_gain
            );
            assert_eq!(track.file_size, get_file_size(path).unwrap());
        }

        // もう一度解析しても、対象はない
        assert_eq!(pending(), 0);
        assert!(plan_units(&library.tracks(), &ScanScope::Missing).is_empty());
        // 選んだ曲を解析し直す: 同じ値のため、書き込まない
        let (again, _) = library.scan(&selected(&[&loud]), false);
        assert_eq!(
            again,
            LoudnessScanResult {
                unchanged_count: 2,
                ..LoudnessScanResult::default()
            }
        );
    }

    #[test]
    fn test_rescanning_a_format_without_peaks_does_not_rewrite_the_file() {
        let library = Library::new();
        // Opusのタグには、ピークを書けない（データベースのピークは、ないまま）
        let id = library.add_fixture("opus.opus");
        let (first, _) = library.scan(&ScanScope::Missing, false);
        assert_eq!(first.written_count, 1);
        let scanned = library.find(&id);
        assert!(scanned.replay_gain.track_gain.is_some());
        assert_eq!(scanned.replay_gain.track_peak, None);
        let path = Path::new(&scanned.file_path);
        let modified = std::fs::metadata(path).unwrap().modified().unwrap();

        // 解析し直しても、同じ値のため、ファイルを書き換えない
        let (again, _) = library.scan(&selected(&[&id]), false);

        assert_eq!(
            again,
            LoudnessScanResult {
                unchanged_count: 1,
                ..LoudnessScanResult::default()
            }
        );
        assert_eq!(
            std::fs::metadata(path).unwrap().modified().unwrap(),
            modified
        );
        assert_eq!(library.find(&id).replay_gain, scanned.replay_gain);
    }

    #[test]
    fn test_cancelled_scan_writes_nothing() {
        let library = Library::new();
        let id = library.add("loud", Some("Album"), -12.0);

        let (result, progress) = library.scan(&ScanScope::Missing, true);

        assert_eq!(
            result,
            LoudnessScanResult {
                cancelled: true,
                ..LoudnessScanResult::default()
            }
        );
        assert!(progress.is_empty());
        assert_eq!(library.find(&id).replay_gain, ReplayGain::default());
    }

    #[test]
    fn test_unreadable_tracks_are_reported_and_the_rest_continue() {
        let library = Library::new();
        let good = library.add("good", Some("Album"), -12.0);
        let broken = library.add("broken", Some("Album"), -12.0);
        std::fs::write(library.dir.path().join("broken.wav"), b"not audio").unwrap();

        let (result, _) = library.scan(&ScanScope::Missing, false);

        assert_eq!(result.written_count, 1);
        assert_eq!(result.error_count, 1);
        assert!(
            result.errors[0].contains("broken.wav"),
            "{:?}",
            result.errors
        );
        // 読めた曲だけから、アルバムのゲインを求める
        assert!(close(library.find(&good).replay_gain.album_gain, -6.0));
        assert_eq!(library.find(&broken).replay_gain, ReplayGain::default());
    }

    fn measurement(energy: f64, blocks: usize, peak: f64) -> Measurement {
        Measurement {
            blocks: vec![energy; blocks],
            peak,
        }
    }

    /// LUFSから、ブロックの大きさ
    fn energy_of(lufs: f64) -> f64 {
        10f64.powf((lufs + 0.691) / 10.0)
    }

    fn unit_of(tracks: &[Track], scope: &ScanScope) -> ScanUnit {
        plan_units(tracks, scope).remove(0)
    }

    #[test]
    fn test_unit_values_use_track_and_album_loudness() {
        let tracks = [
            track("a1", Some("A"), Some(-1.0), None),
            track("a2", Some("A"), None, None),
            track("a3", Some("A"), None, None),
        ];
        let unit = unit_of(&tracks, &ScanScope::Missing);
        let measurements = [
            Some(measurement(energy_of(-20.0), 100, 0.5)),
            Some(measurement(energy_of(-26.0), 200, 0.9)),
            // 読めなかった曲には、何も書かない
            None,
        ];

        let values = unit_values(&unit, &measurements);

        let close = |actual: Option<f64>, expected: f64| {
            actual.is_some_and(|actual| (actual - expected).abs() < 0.05)
        };
        // アルバム: 2曲のブロックをまとめた平均（-20が100個、-26が200個 → 約-23）と、最大のピーク
        let first = values[0].as_ref().unwrap();
        assert_eq!((first.track_gain, first.track_peak), (None, None));
        assert!(close(first.album_gain, 5.0));
        assert_eq!(first.album_peak, Some(0.9));
        // ゲインのなかった曲には、曲のゲイン（-18 - (-26) = +8）とピークも書く
        let second = values[1].as_ref().unwrap();
        assert!(close(second.track_gain, 8.0));
        assert_eq!(second.track_peak, Some(0.9));
        assert_eq!(second.album_gain, first.album_gain);
        assert_eq!(values[2], None);
    }

    #[test]
    fn test_unit_values_for_single_tracks_and_silence() {
        let unit = unit_of(&[track("s1", None, None, None)], &ScanScope::Missing);

        // アルバムのない曲には、アルバムのゲインを書かない
        let values = unit_values(&unit, &[Some(measurement(energy_of(-10.0), 50, 1.0))]);
        let single = values[0].as_ref().unwrap();
        assert!((single.track_gain.unwrap() + 8.0).abs() < 0.05);
        assert_eq!((single.album_gain, single.album_peak), (None, None));

        // 測れなかった曲（無音・短すぎる）は、補正しない（0 dB）
        let values = unit_values(&unit, &[Some(Measurement::default())]);
        assert_eq!(values[0].as_ref().unwrap().track_gain, Some(0.0));
    }

    #[test]
    fn test_is_unchanged_compares_only_the_values_to_write() {
        let current = ReplayGain {
            track_gain: Some(-6.48),
            track_peak: Some(0.988769),
            album_gain: Some(-7.0),
            album_peak: None,
        };
        let album_only = |gain: f64| ReplayGain {
            album_gain: Some(gain),
            ..ReplayGain::default()
        };

        // タグに書く桁より小さい違いは、同じとみなす
        assert!(is_unchanged(&current, &album_only(-7.003)));
        assert!(!is_unchanged(&current, &album_only(-7.02)));
        // 今ない値を書く場合は、違う
        assert!(!is_unchanged(
            &current,
            &ReplayGain {
                album_gain: Some(-7.0),
                album_peak: Some(0.9),
                ..ReplayGain::default()
            }
        ));
        // 書かない項目は、比べない
        assert!(is_unchanged(&current, &ReplayGain::default()));
    }
}
