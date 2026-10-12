//! macOSのNow Playing（`MPNowPlayingInfoCenter`）とリモートコマンド（`MPRemoteCommandCenter`）
//!
//! メディアキー・コントロールセンター・イヤホンのボタンの操作は、リモートコマンドとして届く。
//! OSは、再生状態（`playbackState`）を伝えているアプリを「再生中のアプリ」として扱い、そこへ
//! 操作を届ける。macOSではAVAudioSessionがなく、音を鳴らしているかどうかからは判断されないため、
//! 再生状態は必ず伝える。
//!
//! MediaPlayerのオブジェクトは、メインスレッドだけで扱う。

use super::{Backend, NowPlaying};
use crate::events::PlaybackControl;
use crate::metadata::EmbeddedPicture;
use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSImage;
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSData, NSMutableDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemArtwork, MPMediaItemPropertyAlbumTitle,
    MPMediaItemPropertyArtist, MPMediaItemPropertyArtwork, MPMediaItemPropertyPlaybackDuration,
    MPMediaItemPropertyTitle, MPNowPlayingInfoCenter, MPNowPlayingInfoMediaType,
    MPNowPlayingInfoPropertyDefaultPlaybackRate, MPNowPlayingInfoPropertyElapsedPlaybackTime,
    MPNowPlayingInfoPropertyMediaType, MPNowPlayingInfoPropertyPlaybackRate,
    MPNowPlayingPlaybackState, MPRemoteCommand, MPRemoteCommandCenter, MPRemoteCommandEvent,
    MPRemoteCommandHandlerStatus,
};
use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::Arc;
use tauri::AppHandle;

/// OSからの操作を、フロントエンドへ送る関数
type Emit = Arc<dyn Fn(PlaybackControl) + Send + Sync>;

thread_local! {
    /// OSへ渡したアルバムアート（同じ画像を、更新のたびに作り直さない。メインスレッドだけで使う）
    static ARTWORK: RefCell<Option<CachedArtwork>> = const { RefCell::new(None) };
}

struct CachedArtwork {
    /// 元の画像（同じ画像かどうかを、ポインターで見分ける）
    picture: Arc<EmbeddedPicture>,
    /// OSへ渡す形にしたもの（画像として読めなかった場合は`None`）
    artwork: Option<Retained<MPMediaItemArtwork>>,
}

pub struct MacBackend {
    app: AppHandle,
}

impl MacBackend {
    /// リモートコマンドを受け取り始める（メインスレッドで呼ぶ）
    pub fn new(app: AppHandle, emit: impl Fn(PlaybackControl) + Send + Sync + 'static) -> Self {
        register_commands(Arc::new(emit));
        Self { app }
    }
}

impl Backend for MacBackend {
    fn set_now_playing(&self, now_playing: Option<NowPlaying>) {
        if let Err(e) = self.app.run_on_main_thread(move || apply(now_playing)) {
            log::warn!("Now Playingを更新できません: {}", e);
        }
    }
}

/// 再生中の曲を、OSへ伝える（メインスレッドで呼ぶ）
fn apply(now_playing: Option<NowPlaying>) {
    // SAFETY: 共有のオブジェクトを取得するだけで、引数はない
    let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };

    let Some(now_playing) = now_playing else {
        ARTWORK.with_borrow_mut(|cached| *cached = None);
        // SAFETY: 情報なし（nil）と、列挙値を渡す
        unsafe {
            center.setNowPlayingInfo(None);
            center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
        }
        return;
    };

    let info = NSMutableDictionary::<NSString, AnyObject>::new();
    let set_string = |key: &NSString, value: &str| {
        info.insert(key, &NSString::from_str(value));
    };
    let set_number = |key: &NSString, value: f64| {
        info.insert(key, &NSNumber::numberWithDouble(value));
    };
    // SAFETY: キーは、MediaPlayerフレームワークが定義している定数の文字列
    unsafe {
        set_string(MPMediaItemPropertyTitle, &now_playing.title);
        if let Some(artist) = &now_playing.artist {
            set_string(MPMediaItemPropertyArtist, artist);
        }
        if let Some(album) = &now_playing.album {
            set_string(MPMediaItemPropertyAlbumTitle, album);
        }
        if let Some(duration) = now_playing.duration {
            set_number(MPMediaItemPropertyPlaybackDuration, duration);
        }
        // OSは、渡した時点の位置と速さから、今の位置を計算して表示する
        set_number(
            MPNowPlayingInfoPropertyElapsedPlaybackTime,
            now_playing.position,
        );
        set_number(
            MPNowPlayingInfoPropertyPlaybackRate,
            if now_playing.playing { 1.0 } else { 0.0 },
        );
        set_number(MPNowPlayingInfoPropertyDefaultPlaybackRate, 1.0);
        info.insert(
            MPNowPlayingInfoPropertyMediaType,
            &NSNumber::numberWithUnsignedInteger(MPNowPlayingInfoMediaType::Audio.0),
        );
        if let Some(artwork) = now_playing.artwork.as_ref().and_then(artwork_for) {
            info.insert(MPMediaItemPropertyArtwork, &artwork);
        }
    }

    let state = if now_playing.playing {
        MPNowPlayingPlaybackState::Playing
    } else {
        MPNowPlayingPlaybackState::Paused
    };
    // SAFETY: 文字列をキーにした辞書（値は文字列・数値・アートワーク）と、列挙値を渡す
    unsafe {
        center.setNowPlayingInfo(Some(&info));
        center.setPlaybackState(state);
    }
}

/// 画像を、OSへ渡すアートワークにする（同じ画像なら、前回作ったものを使う）
fn artwork_for(picture: &Arc<EmbeddedPicture>) -> Option<Retained<MPMediaItemArtwork>> {
    ARTWORK.with_borrow_mut(|cached| {
        if let Some(cached) = cached.as_ref()
            && Arc::ptr_eq(&cached.picture, picture)
        {
            return cached.artwork.clone();
        }

        let artwork = create_artwork(&picture.data);
        *cached = Some(CachedArtwork {
            picture: picture.clone(),
            artwork: artwork.clone(),
        });
        artwork
    })
}

fn create_artwork(data: &[u8]) -> Option<Retained<MPMediaItemArtwork>> {
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(data))?;
    let size = image.size();
    if size.width <= 0.0 || size.height <= 0.0 {
        return None;
    }

    // OSは表示する大きさを指定して画像を求める。元の画像をそのまま返し、縮小はOSに任せる
    let handler = RcBlock::new(move |_size: CGSize| NonNull::from(&*image));
    // SAFETY: ハンドラーは、ブロックが持ち続ける（アートワークより先に解放されない）画像を返す
    Some(unsafe {
        MPMediaItemArtwork::initWithBoundsSize_requestHandler(
            MPMediaItemArtwork::alloc(),
            size,
            &handler,
        )
    })
}

/// リモートコマンド（OSからの操作）のハンドラーを登録する
///
/// 登録したハンドラーは、アプリの終了まで外さない。
fn register_commands(emit: Emit) {
    let register = |command: &MPRemoteCommand, control: PlaybackControl| {
        let emit = emit.clone();
        let handler = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
            emit(control);
            MPRemoteCommandHandlerStatus::Success
        });
        // SAFETY: ハンドラーは、コマンドが持ち続ける（戻り値は登録を外すときに使うもので、使わない）
        unsafe {
            command.setEnabled(true);
            command.addTargetWithHandler(&handler);
        }
    };

    // 再生位置の変更（コントロールセンターのシークバー）
    let emit_seek = emit.clone();
    let seek = RcBlock::new(move |event: NonNull<MPRemoteCommandEvent>| {
        // SAFETY: このコマンドのハンドラーには、`MPChangePlaybackPositionCommandEvent`が渡される
        let position = unsafe {
            event
                .cast::<MPChangePlaybackPositionCommandEvent>()
                .as_ref()
                .positionTime()
        };
        emit_seek(PlaybackControl::Seek { position });
        MPRemoteCommandHandlerStatus::Success
    });

    // SAFETY: 共有のオブジェクトと、そのコマンドを取得する（引数はない）。ハンドラーは、コマンドが
    // 持ち続ける
    unsafe {
        let center = MPRemoteCommandCenter::sharedCommandCenter();
        register(&center.togglePlayPauseCommand(), PlaybackControl::Toggle);
        register(&center.playCommand(), PlaybackControl::Play);
        register(&center.pauseCommand(), PlaybackControl::Pause);
        register(&center.nextTrackCommand(), PlaybackControl::Next);
        register(&center.previousTrackCommand(), PlaybackControl::Previous);

        let command = center.changePlaybackPositionCommand();
        command.setEnabled(true);
        command.addTargetWithHandler(&seek);
    }
}
