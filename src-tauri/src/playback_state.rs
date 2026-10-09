//! 再生状態の保存と復元
//!
//! 音量・シャッフル・リピート・再生キュー（トラックIDの並び）・再生していた曲を、アプリデータ配下の
//! `playback-state.json`に保存し、次の起動で復元する。再生位置は保存しない（復元した曲は、
//! 再生ボタンで頭から再生する。起動しただけでは再生を始めない）。
//!
//! 再生キューはフロントエンドが持つため（ADR-025）、保存する内容はフロントエンドから受け取る。
//! キューは数万曲になることがあるため、キューが変わった時だけ受け取り、それ以外の変更
//! （曲の切り替わり・音量など）では、キュー以外の値だけを受け取る。
//!
//! 復元の時は、保存してあるトラックIDをDBと照らし、ライブラリからなくなった曲と、
//! ファイルが見つからない曲を除く。

use crate::error::{AppError, AppResult};
use crate::models::Track;
use crate::validation::validate_track_id;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 保存できる再生キューの長さの上限（ファイルが際限なく大きくならないようにする）
const MAX_QUEUE_LENGTH: usize = 500_000;

/// リピートの設定
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RepeatMode {
    /// リピートしない
    #[default]
    Off,
    /// キューの最後まで再生したら、先頭へ戻る
    All,
    /// 同じ曲を繰り返す
    One,
}

/// 保存する再生状態のうち、キュー以外の値
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackCursor {
    /// 音量（0.0〜1.0）
    pub volume: f32,
    /// シャッフルが有効か
    pub shuffle: bool,
    pub repeat: RepeatMode,
    /// キューの中の、再生していた曲の位置（何も再生していなければnull）
    pub current_index: Option<u32>,
}

impl Default for PlaybackCursor {
    fn default() -> Self {
        Self {
            volume: 1.0,
            shuffle: false,
            repeat: RepeatMode::Off,
            current_index: None,
        }
    }
}

/// 保存する再生キュー（トラックIDの並び）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackQueue {
    /// 再生する順のトラックID
    pub track_ids: Vec<String>,
    /// シャッフルする前の順のトラックID（シャッフルが有効な時だけ。解除した時に、この順へ戻す）
    pub original_track_ids: Option<Vec<String>>,
}

/// 復元する再生状態
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoredPlaybackState {
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    /// 再生キュー（再生する順。ライブラリからなくなった曲・ファイルが見つからない曲は除く）
    pub queue: Vec<Track>,
    /// シャッフルする前の順のトラックID（`queue`にある曲だけ。シャッフルが無効ならnull）
    pub original_track_ids: Option<Vec<String>>,
    /// `queue`の中の、再生していた曲の位置（その曲がなくなっていれば、次の曲の位置。なければnull）
    pub current_index: Option<u32>,
}

/// ファイルに保存する内容（`PlaybackCursor`と`PlaybackQueue`の項目を並べたもの）
///
/// 項目を追加しても古いファイルを読めるよう、ファイルにない項目は既定値で補う。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct StoredState {
    volume: f32,
    shuffle: bool,
    repeat: RepeatMode,
    current_index: Option<u32>,
    track_ids: Vec<String>,
    original_track_ids: Option<Vec<String>>,
}

impl Default for StoredState {
    fn default() -> Self {
        Self::new(PlaybackCursor::default(), PlaybackQueue::default())
    }
}

impl StoredState {
    fn new(cursor: PlaybackCursor, queue: PlaybackQueue) -> Self {
        Self {
            volume: cursor.volume,
            shuffle: cursor.shuffle,
            repeat: cursor.repeat,
            current_index: cursor.current_index,
            track_ids: queue.track_ids,
            original_track_ids: queue.original_track_ids,
        }
    }

    fn queue(&self) -> PlaybackQueue {
        PlaybackQueue {
            track_ids: self.track_ids.clone(),
            original_track_ids: self.original_track_ids.clone(),
        }
    }

    /// 値を受け入れられる範囲に収める: 音量は0〜1（数値でなければ既定の音量）、再生していた曲の
    /// 位置がキューの範囲を外れていれば「何も再生していない」
    fn sanitized(mut self) -> Self {
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            PlaybackCursor::default().volume
        };
        if self
            .current_index
            .is_some_and(|index| index as usize >= self.track_ids.len())
        {
            self.current_index = None;
        }
        self
    }
}

/// 保存してある再生状態と保存先（Tauriの管理状態）
pub struct PlaybackStateStore {
    path: PathBuf,
    current: Mutex<StoredState>,
}

impl PlaybackStateStore {
    /// 保存先から再生状態を読み込んで作成する（ない・読めない・壊れている場合は、何も再生していない状態）
    pub fn load(path: PathBuf) -> Self {
        let current = Mutex::new(load_state(&path));
        Self { path, current }
    }

    /// 再生状態を保存する
    ///
    /// `queue`がNoneなら、キューは保存してあるものから変えない。
    pub fn save(&self, cursor: PlaybackCursor, queue: Option<PlaybackQueue>) -> AppResult<()> {
        if let Some(queue) = &queue {
            validate_queue(queue)?;
        }
        let mut current = self
            .current
            .lock()
            .map_err(|e| AppError::Lock(format!("再生状態の保存に失敗しました: {}", e)))?;
        let queue = queue.unwrap_or_else(|| current.queue());
        let next = StoredState::new(cursor, queue).sanitized();
        if *current == next {
            return Ok(());
        }
        write_state(&self.path, &next)?;
        *current = next;
        Ok(())
    }

    /// 保存してある再生状態を、今のライブラリに合わせて返す
    pub fn restore(&self, conn: &Connection) -> AppResult<RestoredPlaybackState> {
        let stored = self
            .current
            .lock()
            .map_err(|e| AppError::Lock(format!("再生状態の読み込みに失敗しました: {}", e)))?
            .clone();

        let tracks = crate::repository::find_tracks_by_ids(conn, &stored.track_ids)?;
        // 再生できる曲（ライブラリにあり、ファイルが見つかる曲）
        let playable = |id: &String| tracks.get(id).filter(|track| !track.is_missing);

        let mut queue = Vec::with_capacity(stored.track_ids.len());
        let mut current_index = None;
        for (index, id) in stored.track_ids.iter().enumerate() {
            // 再生していた曲（なくなっていれば、その次に残っている曲）の、除いた後の位置
            if current_index.is_none()
                && stored
                    .current_index
                    .is_some_and(|current| index as u32 >= current)
                && playable(id).is_some()
            {
                current_index = Some(queue.len() as u32);
            }
            if let Some(track) = playable(id) {
                queue.push(track.clone());
            }
        }
        let original_track_ids = stored.original_track_ids.as_ref().map(|ids| {
            ids.iter()
                .filter(|id| playable(id).is_some())
                .cloned()
                .collect()
        });

        Ok(RestoredPlaybackState {
            volume: stored.volume,
            shuffle: stored.shuffle,
            repeat: stored.repeat,
            queue,
            original_track_ids,
            current_index,
        })
    }
}

fn validate_queue(queue: &PlaybackQueue) -> AppResult<()> {
    let original = queue.original_track_ids.as_deref().unwrap_or_default();
    if queue.track_ids.len() > MAX_QUEUE_LENGTH || original.len() > MAX_QUEUE_LENGTH {
        return Err(AppError::Validation(format!(
            "再生キューが長すぎます（{}曲まで）",
            MAX_QUEUE_LENGTH
        )));
    }
    queue
        .track_ids
        .iter()
        .chain(original)
        .try_for_each(|id| validate_track_id(id))
}

/// ファイルから再生状態を読み込む（ない・読めない・不正な場合は、何も再生していない状態）
fn load_state(path: &Path) -> StoredState {
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(e) if e.kind() == ErrorKind::NotFound => return StoredState::default(),
        Err(e) => {
            log::warn!("再生状態のファイルを読み込めませんでした: {}", e);
            return StoredState::default();
        }
    };
    match serde_json::from_str::<StoredState>(&json) {
        Ok(state) if validate_queue(&state.queue()).is_ok() => state.sanitized(),
        Ok(_) => {
            log::warn!("再生状態のファイルの値が不正なため、保存してある再生状態を使いません");
            StoredState::default()
        }
        Err(e) => {
            log::warn!(
                "再生状態のファイルを解析できないため、保存してある再生状態を使いません: {}",
                e
            );
            StoredState::default()
        }
    }
}

/// 再生状態をファイルに保存する
///
/// 書き込み途中で中断してもファイルが壊れないよう、一時ファイルに書いてから置き換える。
fn write_state(path: &Path, state: &StoredState) -> AppResult<()> {
    let json = serde_json::to_string(state)
        .map_err(|e| AppError::Io(format!("再生状態の変換に失敗しました: {}", e)))?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, json)
        .map_err(|e| AppError::Io(format!("再生状態の保存に失敗しました: {}", e)))?;
    fs::rename(&temp_path, path)
        .map_err(|e| AppError::Io(format!("再生状態の保存に失敗しました: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テストごとに独立した一時ディレクトリの、再生状態のファイルのパス
    fn temp_state_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muspice-playback-state-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("playback-state.json")
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    /// テスト用のトラックID（UUIDの形式）
    fn id(n: u32) -> String {
        format!("00000000-0000-4000-8000-{:012}", n)
    }

    fn insert_track(conn: &Connection, n: u32) {
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, title, format, file_size)
             VALUES (?1, ?2, ?3, ?4, 'mp3', 1)",
            rusqlite::params![
                id(n),
                format!("/music/{n}.mp3"),
                format!("{n}.mp3"),
                format!("曲{n}")
            ],
        )
        .unwrap();
    }

    fn cursor(current_index: Option<u32>) -> PlaybackCursor {
        PlaybackCursor {
            volume: 0.4,
            shuffle: false,
            repeat: RepeatMode::All,
            current_index,
        }
    }

    fn queue(ids: &[u32]) -> PlaybackQueue {
        PlaybackQueue {
            track_ids: ids.iter().map(|&n| id(n)).collect(),
            original_track_ids: None,
        }
    }

    fn queue_ids(state: &RestoredPlaybackState) -> Vec<String> {
        state.queue.iter().map(|track| track.id.clone()).collect()
    }

    #[test]
    fn test_missing_file_restores_an_empty_state() {
        let store = PlaybackStateStore::load(temp_state_path("missing"));

        let state = store.restore(&setup_db()).unwrap();

        assert_eq!(state.volume, 1.0);
        assert!(!state.shuffle);
        assert_eq!(state.repeat, RepeatMode::Off);
        assert!(state.queue.is_empty());
        assert_eq!(state.current_index, None);
    }

    #[test]
    fn test_saved_state_is_restored_after_reload() {
        let conn = setup_db();
        (1..=3).for_each(|n| insert_track(&conn, n));
        let path = temp_state_path("round-trip");
        let store = PlaybackStateStore::load(path.clone());

        store
            .save(
                PlaybackCursor {
                    shuffle: true,
                    ..cursor(Some(1))
                },
                Some(PlaybackQueue {
                    track_ids: vec![id(3), id(1), id(2)],
                    original_track_ids: Some(vec![id(1), id(2), id(3)]),
                }),
            )
            .unwrap();

        // アプリを起動し直しても、同じ内容で復元する
        let state = PlaybackStateStore::load(path.clone())
            .restore(&conn)
            .unwrap();
        assert_eq!(state.volume, 0.4);
        assert!(state.shuffle);
        assert_eq!(state.repeat, RepeatMode::All);
        assert_eq!(queue_ids(&state), vec![id(3), id(1), id(2)]);
        assert_eq!(state.queue[1].title.as_deref(), Some("曲1"));
        assert_eq!(state.original_track_ids, Some(vec![id(1), id(2), id(3)]));
        assert_eq!(state.current_index, Some(1));
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn test_cursor_only_save_keeps_the_queue() {
        let conn = setup_db();
        (1..=3).for_each(|n| insert_track(&conn, n));
        let path = temp_state_path("cursor");
        let store = PlaybackStateStore::load(path.clone());
        store
            .save(cursor(Some(0)), Some(queue(&[1, 2, 3])))
            .unwrap();

        // 曲が切り替わっただけ（キューは変わっていない）
        store.save(cursor(Some(2)), None).unwrap();

        let state = PlaybackStateStore::load(path).restore(&conn).unwrap();
        assert_eq!(queue_ids(&state), vec![id(1), id(2), id(3)]);
        assert_eq!(state.current_index, Some(2));
    }

    #[test]
    fn test_restore_drops_tracks_that_are_gone_or_missing() {
        let conn = setup_db();
        (1..=5).for_each(|n| insert_track(&conn, n));
        let store = PlaybackStateStore::load(temp_state_path("gone"));
        store
            .save(
                PlaybackCursor {
                    shuffle: true,
                    ..cursor(Some(3))
                },
                Some(PlaybackQueue {
                    track_ids: (1..=5).map(id).collect(),
                    original_track_ids: Some((1..=5).rev().map(id).collect()),
                }),
            )
            .unwrap();
        // 2はライブラリから外し、4はファイルが見つからない曲にする
        conn.execute("DELETE FROM tracks WHERE id = ?1", [id(2)])
            .unwrap();
        conn.execute(
            "UPDATE tracks SET missing_since = '2026-01-01T00:00:00Z' WHERE id = ?1",
            [id(4)],
        )
        .unwrap();

        let state = store.restore(&conn).unwrap();

        assert_eq!(queue_ids(&state), vec![id(1), id(3), id(5)]);
        assert_eq!(state.original_track_ids, Some(vec![id(5), id(3), id(1)]));
        // 再生していた曲（4）がなくなったため、その次に残っている曲（5）の位置になる
        assert_eq!(state.current_index, Some(2));
    }

    #[test]
    fn test_restore_keeps_the_current_track_when_earlier_tracks_are_gone() {
        let conn = setup_db();
        (1..=4).for_each(|n| insert_track(&conn, n));
        let store = PlaybackStateStore::load(temp_state_path("shift"));
        store
            .save(cursor(Some(2)), Some(queue(&[1, 2, 3, 4])))
            .unwrap();
        conn.execute("DELETE FROM tracks WHERE id = ?1", [id(1)])
            .unwrap();

        let state = store.restore(&conn).unwrap();

        // 前の曲がなくなった分、位置がずれても、同じ曲（3）を指す
        assert_eq!(state.queue[state.current_index.unwrap() as usize].id, id(3));
    }

    #[test]
    fn test_restore_has_no_current_track_when_the_rest_of_the_queue_is_gone() {
        let conn = setup_db();
        (1..=2).for_each(|n| insert_track(&conn, n));
        let store = PlaybackStateStore::load(temp_state_path("tail"));
        store.save(cursor(Some(1)), Some(queue(&[1, 2]))).unwrap();
        conn.execute("DELETE FROM tracks WHERE id = ?1", [id(2)])
            .unwrap();

        let state = store.restore(&conn).unwrap();

        assert_eq!(queue_ids(&state), vec![id(1)]);
        assert_eq!(state.current_index, None);
    }

    #[test]
    fn test_save_sanitizes_values() {
        let conn = setup_db();
        insert_track(&conn, 1);
        let store = PlaybackStateStore::load(temp_state_path("sanitize"));

        // 範囲外の音量は丸め、キューの外を指す位置は「何も再生していない」にする
        store
            .save(
                PlaybackCursor {
                    volume: 7.0,
                    ..cursor(Some(9))
                },
                Some(queue(&[1])),
            )
            .unwrap();
        let state = store.restore(&conn).unwrap();
        assert_eq!(state.volume, 1.0);
        assert_eq!(state.current_index, None);

        store
            .save(
                PlaybackCursor {
                    volume: f32::NAN,
                    ..cursor(None)
                },
                None,
            )
            .unwrap();
        assert_eq!(store.restore(&conn).unwrap().volume, 1.0);
    }

    #[test]
    fn test_save_rejects_invalid_track_ids() {
        let path = temp_state_path("invalid");
        let store = PlaybackStateStore::load(path.clone());

        let result = store.save(
            cursor(None),
            Some(PlaybackQueue {
                track_ids: vec!["../etc/passwd".to_string()],
                original_track_ids: None,
            }),
        );

        assert!(matches!(result, Err(AppError::Validation(_))));
        assert!(!path.exists());
    }

    #[test]
    fn test_corrupted_file_restores_an_empty_state() {
        let path = temp_state_path("corrupted");
        fs::write(&path, "{ not json").unwrap();
        assert!(
            PlaybackStateStore::load(path.clone())
                .restore(&setup_db())
                .unwrap()
                .queue
                .is_empty()
        );

        // トラックIDの形式でない値が入っているファイルも使わない
        fs::write(&path, r#"{ "volume": 0.5, "trackIds": ["x"] }"#).unwrap();
        let state = PlaybackStateStore::load(path).restore(&setup_db()).unwrap();
        assert_eq!(state.volume, 1.0);
    }

    #[test]
    fn test_file_with_missing_fields_uses_defaults() {
        let conn = setup_db();
        let path = temp_state_path("partial");
        fs::write(&path, r#"{ "volume": 0.25 }"#).unwrap();

        let state = PlaybackStateStore::load(path).restore(&conn).unwrap();

        assert_eq!(state.volume, 0.25);
        assert_eq!(state.repeat, RepeatMode::Off);
        assert!(state.queue.is_empty());
    }
}
