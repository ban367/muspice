//! Tauriコマンドハンドラーモジュール
//!
//! 各ドメインごとにサブモジュールに分割されたコマンドをフラットにre-exportする。
//! lib.rsのuse文を変更不要にするため、全コマンドをここから公開する。

mod import;
mod metadata_cmd;
mod player;
mod playlist_cmd;
mod stats;
mod system;
mod tracks;

// インポート関連
pub use import::import_folder;

// トラック取得・検索・フィルタリング・グループ化・削除
pub use tracks::{
    delete_tracks_command, delete_tracks_with_files_command, filter_tracks, get_albums_grouped,
    get_all_tracks, get_artists_grouped, get_genres_grouped, get_unique_albums, get_unique_artists,
    get_unique_genres, search_tracks,
};

// メタデータ編集
pub use metadata_cmd::{
    refresh_library_metadata, update_multiple_tracks_metadata, update_track_metadata,
    update_track_metadata_with_file, validate_metadata_command,
};

// プレイリスト管理
pub use playlist_cmd::{
    add_track_to_playlist, create_playlist, delete_playlist, get_playlists,
    remove_track_from_playlist, rename_playlist, reorder_playlist_tracks,
};

// プレーヤー・アルバムアート
pub use player::{get_album_art, get_current_track, get_track_file_path, set_current_track};

// 統計（お気に入り・レーティング・再生回数）
pub use stats::{
    get_favorite_tracks, get_most_played_tracks, get_recently_played_tracks, increment_play_count,
    set_rating, toggle_favorite,
};

// システム
pub use system::show_in_folder;
