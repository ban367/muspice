//! OSのメディアキー・Now Playing（コントロールセンターなど）との連携
//!
//! 音はRust側の再生エンジン（`playback`）が鳴らし、WebViewは何も再生しないため、WebViewの
//! Media Session APIではOSに伝わらない。そこで、OSのAPIを直接使う。
//!
//! - 再生中の曲: フロントエンド（`playback.svelte.ts`）が、プレーヤーバーに表示している曲・
//!   再生中かどうか・再生位置を`set_now_playing`で伝える。ここで曲の情報とアルバムアートを
//!   ライブラリから読み、OSへ渡す
//! - OSからの操作（メディアキー・コントロールセンター・イヤホンのボタン）: `PlaybackControl`として
//!   フロントエンドへ送る（メニューバーの「再生」メニューと同じ経路）。再生キューを持つ
//!   フロントエンドが、次・前の曲を決めて再生する
//!
//! 対応しているのはmacOSだけ（`macos.rs`）。ほかのOSでは何もしない。

#[cfg(target_os = "macos")]
mod macos;

use crate::error::AppResult;
use crate::events::PlaybackControl;
use crate::metadata::EmbeddedPicture;
use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_track_id;
use serde::Deserialize;
use specta::Type;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri_specta::Event;

/// フロントエンドから届く、プレーヤーバーの状態
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingUpdate {
    /// 再生中（一時停止中）の曲
    pub track_id: String,
    /// 再生中か（falseは一時停止中）
    pub playing: bool,
    /// 再生位置（曲の頭からの秒数）
    pub position: f64,
    /// 曲の長さ（秒。再生エンジンがファイルから読んだ値。分からなければnullで、ライブラリの値を使う）
    pub duration: Option<f64>,
}

/// OSへ渡す、再生中の曲
// macOS以外では渡す先がなく、読まれない
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// 曲の長さ（秒）
    pub duration: Option<f64>,
    /// 再生位置（秒）
    pub position: f64,
    pub playing: bool,
    /// アルバムアート（同じ曲の間は、同じ`Arc`を渡す）
    pub artwork: Option<Arc<EmbeddedPicture>>,
}

/// OSごとの実装
pub trait Backend: Send + Sync {
    /// OSへ伝えられるか（falseなら、曲の情報・アルバムアートを読まない）
    fn is_available(&self) -> bool {
        true
    }

    /// 再生中の曲を、OSへ伝える（`None`は、再生している曲がない）
    fn set_now_playing(&self, now_playing: Option<NowPlaying>);
}

/// 対応していないOS: 何もしない
#[cfg(not(target_os = "macos"))]
struct NoBackend;

#[cfg(not(target_os = "macos"))]
impl Backend for NoBackend {
    fn is_available(&self) -> bool {
        false
    }

    fn set_now_playing(&self, _now_playing: Option<NowPlaying>) {}
}

/// OSへ渡した曲のアルバムアート（曲が変わるまで、読み直さない）
struct Artwork {
    track_id: String,
    file_path: String,
    picture: Option<Arc<EmbeddedPicture>>,
}

/// OSのメディアキー・Now Playingとの連携（Tauriの状態として管理する）
pub struct MediaControls {
    backend: Box<dyn Backend>,
    /// OSへ渡している曲のアルバムアート。更新を1つずつ反映するためのロックも兼ねる
    artwork: Mutex<Option<Artwork>>,
}

impl MediaControls {
    /// OSとの連携を始める（OSからの操作を受け取り始める。メインスレッドで呼ぶ）
    pub fn start(app: &AppHandle) -> Self {
        let events = app.clone();
        let emit = move |control: PlaybackControl| {
            if let Err(e) = control.emit(&events) {
                log::warn!("再生の操作をフロントエンドへ送れません: {}", e);
            }
        };

        #[cfg(target_os = "macos")]
        let backend: Box<dyn Backend> = Box::new(macos::MacBackend::new(app.clone(), emit));
        #[cfg(not(target_os = "macos"))]
        let backend: Box<dyn Backend> = {
            // 操作を受け取る先がない
            let _ = emit;
            Box::new(NoBackend)
        };

        Self::with_backend(backend)
    }

    fn with_backend(backend: Box<dyn Backend>) -> Self {
        Self {
            backend,
            artwork: Mutex::new(None),
        }
    }

    /// プレーヤーバーの状態を、OSへ伝える（`None`は、再生している曲がない）
    ///
    /// 曲の情報は、トラックIDからライブラリ（DB）で読む。ファイルを読む場合があるため、
    /// ブロッキング処理用のスレッドで呼ぶ。
    pub fn set(&self, state: &AppState, update: Option<NowPlayingUpdate>) -> AppResult<()> {
        let Some(update) = update else {
            self.clear();
            return Ok(());
        };

        validate_track_id(&update.track_id)?;
        if !self.backend.is_available() {
            return Ok(());
        }
        let track = state
            .with_db(|db| crate::repository::find_track_by_id(db, &update.track_id))
            // ライブラリから外した曲など: 前の曲の情報を残さない
            .inspect_err(|_| self.clear())?;

        self.publish(&update, &track, || {
            crate::album_art::picture_for_track(state, &track.id, &track.file_path)
        });
        Ok(())
    }

    /// 再生している曲がないことを、OSへ伝える
    fn clear(&self) {
        let mut artwork = self.lock_artwork();
        *artwork = None;
        self.backend.set_now_playing(None);
    }

    /// 曲の情報を組み立てて、OSへ伝える
    ///
    /// `load_artwork`は、曲（とそのファイル）が前回と変わったときだけ呼ぶ。
    fn publish(
        &self,
        update: &NowPlayingUpdate,
        track: &Track,
        load_artwork: impl FnOnce() -> Option<EmbeddedPicture>,
    ) {
        let mut artwork = self.lock_artwork();
        let cached = artwork
            .as_ref()
            .filter(|cached| cached.track_id == track.id && cached.file_path == track.file_path);
        let picture = match cached {
            Some(cached) => cached.picture.clone(),
            None => {
                let picture = load_artwork().map(Arc::new);
                *artwork = Some(Artwork {
                    track_id: track.id.clone(),
                    file_path: track.file_path.clone(),
                    picture: picture.clone(),
                });
                picture
            }
        };

        // 曲の長さは、再生エンジンがファイルから読んだ値を優先する（ライブラリの値は秒単位）
        let duration = update
            .duration
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .or_else(|| {
                track
                    .duration
                    .filter(|duration| *duration > 0)
                    .map(f64::from)
            });
        let position = if update.position.is_finite() {
            update.position.max(0.0)
        } else {
            0.0
        };

        self.backend.set_now_playing(Some(NowPlaying {
            // プレーヤーバーと同じく、タイトルがなければファイル名を出す
            title: non_empty(&track.title).unwrap_or_else(|| track.file_name.clone()),
            artist: non_empty(&track.artist),
            album: non_empty(&track.album),
            duration,
            position: duration.map_or(position, |duration| position.min(duration)),
            playing: update.playing,
            artwork: picture,
        }));
    }

    /// アルバムアートのロックを取得する（守るのは表示用の情報だけのため、途中でパニックした
    /// スレッドがあっても続ける）
    fn lock_artwork(&self) -> std::sync::MutexGuard<'_, Option<Artwork>> {
        self.artwork
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 空（空白だけ）の値を、値なしとして扱う
fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use rusqlite::Connection;
    use std::cell::Cell;

    const TRACK_ID: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER_TRACK_ID: &str = "22222222-2222-4222-8222-222222222222";

    /// OSへ伝えた内容を記録するバックエンド
    #[derive(Default)]
    struct Recorder {
        updates: Mutex<Vec<Option<NowPlaying>>>,
        /// 対応していないOSのふりをする
        unavailable: bool,
    }

    impl Backend for Arc<Recorder> {
        fn is_available(&self) -> bool {
            !self.unavailable
        }

        fn set_now_playing(&self, now_playing: Option<NowPlaying>) {
            self.updates.lock().unwrap().push(now_playing);
        }
    }

    impl Recorder {
        fn last(&self) -> Option<NowPlaying> {
            self.updates
                .lock()
                .unwrap()
                .last()
                .cloned()
                .expect("OSへ何も伝えていません")
        }
    }

    fn controls() -> (MediaControls, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        (
            MediaControls::with_backend(Box::new(recorder.clone())),
            recorder,
        )
    }

    fn test_state() -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        AppState::new(conn)
    }

    fn insert_track(state: &AppState, id: &str, title: Option<&str>, duration: Option<i32>) {
        state
            .with_db(|db| {
                db.execute(
                    "INSERT INTO tracks (id, file_path, file_name, title, artist, album, duration,
                                         format, file_size, created_at, updated_at)
                     VALUES (?1, ?2, 'song.flac', ?3, 'Artist', '  ', ?4,
                             'flac', 1000, datetime('now'), datetime('now'))",
                    rusqlite::params![id, format!("/music/{id}.flac"), title, duration],
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
    }

    fn track(state: &AppState, id: &str) -> Track {
        state
            .with_db(|db| crate::repository::find_track_by_id(db, id))
            .unwrap()
    }

    fn update(track_id: &str, position: f64, duration: Option<f64>) -> NowPlayingUpdate {
        NowPlayingUpdate {
            track_id: track_id.to_string(),
            playing: true,
            position,
            duration,
        }
    }

    fn picture() -> Option<EmbeddedPicture> {
        Some(EmbeddedPicture {
            data: vec![1, 2, 3],
            mime_type: "image/png".to_string(),
        })
    }

    #[test]
    fn test_publishes_track_information_from_the_library() {
        let state = test_state();
        insert_track(&state, TRACK_ID, Some("Song"), Some(200));
        let (controls, recorder) = controls();

        controls.publish(
            &NowPlayingUpdate {
                playing: false,
                ..update(TRACK_ID, 12.5, Some(201.3))
            },
            &track(&state, TRACK_ID),
            picture,
        );

        let now_playing = recorder.last().unwrap();
        assert_eq!(now_playing.title, "Song");
        assert_eq!(now_playing.artist.as_deref(), Some("Artist"));
        // 空白だけの値は、値なしとして扱う
        assert_eq!(now_playing.album, None);
        // 曲の長さは、再生エンジンが読んだ値を優先する
        assert_eq!(now_playing.duration, Some(201.3));
        assert_eq!(now_playing.position, 12.5);
        assert!(!now_playing.playing);
        assert_eq!(now_playing.artwork.unwrap().data, vec![1, 2, 3]);
    }

    #[test]
    fn test_falls_back_to_file_name_and_library_duration() {
        let state = test_state();
        insert_track(&state, TRACK_ID, None, Some(200));
        let (controls, recorder) = controls();

        controls.publish(
            &update(TRACK_ID, 0.0, None),
            &track(&state, TRACK_ID),
            || None,
        );

        let now_playing = recorder.last().unwrap();
        assert_eq!(now_playing.title, "song.flac");
        assert_eq!(now_playing.duration, Some(200.0));
        assert!(now_playing.playing);
        assert!(now_playing.artwork.is_none());
    }

    #[test]
    fn test_keeps_position_within_the_track() {
        let state = test_state();
        insert_track(&state, TRACK_ID, Some("Song"), None);
        let (controls, recorder) = controls();
        let track = track(&state, TRACK_ID);

        controls.publish(&update(TRACK_ID, 500.0, Some(180.0)), &track, || None);
        assert_eq!(recorder.last().unwrap().position, 180.0);

        controls.publish(&update(TRACK_ID, -3.0, Some(180.0)), &track, || None);
        assert_eq!(recorder.last().unwrap().position, 0.0);

        // 曲の長さが分からない場合は、位置をそのまま渡す
        controls.publish(&update(TRACK_ID, f64::NAN, Some(f64::NAN)), &track, || None);
        let now_playing = recorder.last().unwrap();
        assert_eq!(now_playing.position, 0.0);
        assert_eq!(now_playing.duration, None);
    }

    #[test]
    fn test_reads_artwork_only_when_the_track_changes() {
        let state = test_state();
        insert_track(&state, TRACK_ID, Some("Song"), None);
        insert_track(&state, OTHER_TRACK_ID, Some("Other"), None);
        let (controls, recorder) = controls();
        let loads = Cell::new(0);
        let load = || {
            loads.set(loads.get() + 1);
            picture()
        };

        let first = track(&state, TRACK_ID);
        controls.publish(&update(TRACK_ID, 0.0, None), &first, load);
        let artwork = recorder.last().unwrap().artwork.unwrap();
        // 一時停止・シークなどの更新では、同じ画像を渡す
        controls.publish(&update(TRACK_ID, 30.0, None), &first, load);
        assert_eq!(loads.get(), 1);
        assert!(Arc::ptr_eq(
            &artwork,
            &recorder.last().unwrap().artwork.unwrap()
        ));

        controls.publish(
            &update(OTHER_TRACK_ID, 0.0, None),
            &track(&state, OTHER_TRACK_ID),
            load,
        );
        assert_eq!(loads.get(), 2);

        // 再生している曲がなくなった後は、同じ曲でも読み直す
        controls.clear();
        assert!(recorder.last().is_none());
        controls.publish(
            &update(OTHER_TRACK_ID, 0.0, None),
            &track(&state, OTHER_TRACK_ID),
            load,
        );
        assert_eq!(loads.get(), 3);
    }

    #[test]
    fn test_set_reads_the_track_by_id() {
        let state = test_state();
        insert_track(&state, TRACK_ID, Some("Song"), Some(200));
        let (controls, recorder) = controls();

        controls
            .set(&state, Some(update(TRACK_ID, 1.0, None)))
            .unwrap();
        // ファイルがないため、アルバムアートは「なし」になる
        let now_playing = recorder.last().unwrap();
        assert_eq!(now_playing.title, "Song");
        assert!(now_playing.artwork.is_none());

        controls.set(&state, None).unwrap();
        assert!(recorder.last().is_none());
    }

    #[test]
    fn test_set_clears_now_playing_when_the_track_is_not_in_the_library() {
        let state = test_state();
        insert_track(&state, TRACK_ID, Some("Song"), None);
        let (controls, recorder) = controls();
        controls
            .set(&state, Some(update(TRACK_ID, 0.0, None)))
            .unwrap();

        let result = controls.set(&state, Some(update(OTHER_TRACK_ID, 0.0, None)));
        assert!(matches!(result, Err(AppError::NotFound(_))));
        assert!(recorder.last().is_none());

        // WebViewからは、トラックIDの形式のものだけを受け取る
        let result = controls.set(&state, Some(update("../etc/passwd", 0.0, None)));
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn test_set_reads_nothing_when_the_os_is_not_supported() {
        let state = test_state();
        let recorder = Arc::new(Recorder {
            unavailable: true,
            ..Recorder::default()
        });
        let controls = MediaControls::with_backend(Box::new(recorder.clone()));

        // ライブラリを読まないため、ない曲でもエラーにならない（IDの形式だけは確かめる）
        controls
            .set(&state, Some(update(TRACK_ID, 0.0, None)))
            .unwrap();
        assert!(recorder.updates.lock().unwrap().is_empty());
        assert!(matches!(
            controls.set(&state, Some(update("x", 0.0, None))),
            Err(AppError::Validation(_))
        ));
    }
}
