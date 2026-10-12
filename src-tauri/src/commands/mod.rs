//! Tauriコマンドハンドラーモジュール
//!
//! 各ドメインごとにサブモジュールに分割されたコマンドをフラットにre-exportする。
//! lib.rsのuse文を変更不要にするため、全コマンドをここから公開する。

use crate::error::{AppError, AppResult};

mod devices;
mod import;
mod library_folders;
mod metadata_cmd;
mod playback;
mod player;
mod playlist_cmd;
mod settings;
mod stats;
mod system;
mod tracks;

// インポート関連
pub use import::import_folder;

// ライブラリフォルダ（インポートしたフォルダ）の一覧・削除・再スキャンと、見つからない曲の削除
pub use library_folders::{
    get_library_folders, remove_library_folder, remove_missing_tracks, rescan_library_folder,
};
// 自動の再スキャン（`library_sync`）が使う
pub(crate) use library_folders::rescan_folder;

// 転送先デバイス（SDカードなどのフォルダ）の登録・設定・同期
pub use devices::{
    cancel_device_sync, get_sync_devices, plan_device_sync, register_sync_device,
    relink_sync_device, remove_sync_device, run_device_sync, update_sync_device,
};

// トラック取得・検索・フィルタリング・一覧（アルバム・アーティスト・ジャンル）・削除
pub use tracks::{
    delete_tracks_command, delete_tracks_with_files_command, filter_tracks, get_album_tracks,
    get_albums, get_all_tracks, get_artist_albums, get_artists, get_genre_tracks, get_genres,
    get_unique_albums, get_unique_artists, get_unique_genres, search_tracks,
};

// メタデータ編集
pub use metadata_cmd::{
    refresh_library_metadata, update_multiple_tracks_metadata, update_track_metadata,
    write_library_metadata_to_files,
};

// プレイリスト管理
pub use playlist_cmd::{
    add_tracks_to_playlist, create_playlist, delete_playlist, get_playlist_tracks, get_playlists,
    remove_track_from_playlist, rename_playlist, reorder_playlist_tracks,
};

// 再生中のトラックの記録
pub use player::{get_current_track, set_current_track};

// 再生エンジン
pub use playback::{
    get_output_devices, playback_pause, playback_play, playback_resume, playback_seek,
    playback_set_equalizer, playback_set_next, playback_set_volume, playback_stop,
};

// 統計（お気に入り・レーティング・再生回数）
pub use stats::{
    get_favorite_tracks, get_most_played_tracks, get_recently_played_tracks, increment_play_count,
    set_rating, toggle_favorite,
};

// 設定
pub use settings::{get_settings, save_settings};

// システム
pub use system::{PROJECT_URL, open_project_page, show_in_folder};

/// 重い同期処理をブロッキング処理用のスレッドで実行する
///
/// ファイルI/O・タグ解析・大量のDB書き込みをasyncコマンド内で直接行うと、
/// Tauriの非同期ランタイムのワーカースレッドを占有し、他のコマンドの応答が遅れる。
/// クロージャは`'static`である必要があるため、状態が必要な場合は
/// `AppHandle`を移動して`app.state::<AppState>()`で取得する。
async fn run_blocking<T, F>(f: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Io(format!("バックグラウンド処理が異常終了しました: {}", e)))?
}
