//! Tauriコマンドハンドラーモジュール
//!
//! 各ドメインごとにサブモジュールに分割されたコマンドをフラットにre-exportする。
//! lib.rsのuse文を変更不要にするため、全コマンドをここから公開する。

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder};

mod album_art;
mod backup;
mod convert;
mod devices;
mod import;
mod library_folders;
mod library_xml;
mod loudness;
mod m3u;
mod metadata_cmd;
mod music_folder;
mod organize;
mod playback;
mod player;
mod playlist_cmd;
mod settings;
mod stats;
mod system;
mod tracks;

// インポート関連
pub(crate) use import::{
    ImportRun, ImportTargets, ScannedImportFolder, copy_destination, import_and_notify,
};
pub use import::{
    PendingImport, cancel_pending_import, import_dropped_files, import_files, import_folder,
    import_pending_folders, take_opened_files,
};

// ライブラリフォルダ（インポートしたフォルダ）の一覧・削除・場所の付け替え・再スキャン・
// 読み込まないフォルダと、見つからない曲の削除
pub use library_folders::{
    PendingRelocation, add_library_folder_exclusion, cancel_library_folder_relocation,
    get_library_folders, plan_library_folder_relocation, relocate_library_folder,
    remove_excluded_tracks, remove_library_folder, remove_library_folder_exclusion,
    remove_missing_tracks, rescan_library_folder,
};
// 自動の再スキャン（`library_sync`）が使う
pub(crate) use library_folders::rescan_folder;

// 音楽フォルダ（新しい曲を置く宛先）の設定と、書式の例
pub use music_folder::{
    clear_music_folder, get_music_folder, preview_music_folder_template, set_music_folder,
};

// 転送先デバイス（SDカードなどのフォルダ）の登録・設定・同期
pub use devices::{
    cancel_device_sync, get_sync_devices, plan_device_sync, register_sync_device,
    relink_sync_device, remove_sync_device, run_device_sync, update_sync_device,
};

// トラック取得・検索・フィルタリング・一覧（アルバム・アーティスト・ジャンル）・削除
pub use tracks::{
    can_move_to_trash, delete_tracks_command, delete_tracks_with_files_command,
    get_album_artist_albums, get_album_tracks, get_albums, get_all_tracks, get_artist_albums,
    get_artists, get_genre_tracks, get_genres, get_unique_albums, get_unique_artists,
    get_unique_genres, search_tracks,
};

// メタデータ編集
pub use metadata_cmd::{
    apply_metadata_changes, get_track_lyrics, get_track_tags, refresh_library_metadata,
    update_multiple_tracks_metadata, update_track_metadata, write_library_metadata_to_files,
};

// 形式の変換
pub use convert::{
    ConvertState, cancel_conversion, convert_tracks, get_convert_formats, plan_conversion,
};

// 音量（ラウドネス）の解析
pub use loudness::{cancel_loudness_scan, get_loudness_scan_status, start_loudness_scan};

// ファイルの整理（タグに基づくリネーム・移動）
pub use organize::{OrganizeState, cancel_file_organize, organize_files, plan_file_organize};

// アルバムアートの編集（埋め込み・取り除き）と、情報の取得
pub use album_art::{get_album_art_info, remove_album_art, set_album_art};

// プレイリスト管理
pub use playlist_cmd::{
    add_tracks_to_playlist, count_smart_playlist_tracks, create_playlist, create_playlist_folder,
    create_playlist_with_tracks, create_smart_playlist, delete_playlist, delete_playlist_folder,
    get_playlist_folders, get_playlist_tracks, get_playlists, move_playlist,
    remove_tracks_from_playlist, rename_playlist, rename_playlist_folder, reorder_playlist_folders,
    reorder_playlist_tracks, reorder_playlists, reshuffle_smart_playlists,
    set_playlist_description, update_smart_playlist,
};

// ライブラリのXML（iTunes形式）の取り込み
pub use library_xml::import_library_xml;

// プレイリストのM3Uの読み込み・書き出し
pub use m3u::{export_playlist_m3u, import_m3u_playlists};

// 再生中のトラックの記録と、再生状態の保存・復元
pub use player::{get_current_track, get_playback_state, save_playback_state, set_current_track};

// 再生エンジンと、OSのNow Playing
pub use playback::{
    get_output_devices, playback_pause, playback_play, playback_resume, playback_seek,
    playback_set_equalizer, playback_set_next, playback_set_speed, playback_set_volume,
    playback_stop, set_now_playing,
};

// 統計（お気に入り・レーティング・再生回数・スキップ回数・再生履歴）
pub use stats::{
    get_favorite_tracks, get_most_played_tracks, get_play_history, increment_play_count,
    increment_skip_count, set_favorite, set_rating,
};

// 設定
pub use settings::{get_global_shortcuts, get_settings, save_settings, set_mini_player};

// システム
pub use system::{PROJECT_URL, open_project_page, show_in_folder};

// ライブラリのデータ（DB・設定）のバックアップと復元
pub use backup::{
    PendingRestore, cancel_library_restore, export_library_backup, plan_library_restore,
    restore_library_backup,
};

/// ファイルを選ぶダイアログ（メインウィンドウがあれば、その上に出す）
///
/// 読み書きするファイルはRust側のダイアログで選び、WebViewからはパスを受け取らない（ADR-005）。
fn file_dialog(app: &AppHandle) -> FileDialogBuilder<tauri::Wry> {
    let dialog = app.dialog().file();
    match app.get_webview_window("main") {
        Some(window) => dialog.set_parent(&window),
        None => dialog,
    }
}

/// フォルダを選ぶダイアログを開き、選んだフォルダを返す（選ばずに閉じた場合はNone）
///
/// フォルダもRust側のダイアログで選び、WebViewからはパスを受け取らない（ADR-065）。ダイアログ
/// （タイトル・開く場所・上に出すウィンドウ）は、呼び出し側が作る。ダイアログを開いている間は
/// スレッドを止めるため、ブロッキング処理用のスレッド（`run_blocking`）から呼ぶ。
fn pick_folder(dialog: FileDialogBuilder<tauri::Wry>) -> AppResult<Option<PathBuf>> {
    let Some(folder) = dialog.blocking_pick_folder() else {
        return Ok(None);
    };
    let path = folder
        .into_path()
        .map_err(|e| AppError::Io(format!("選んだフォルダを扱えません: {}", e)))?;
    if !path.is_absolute() || !path.is_dir() {
        return Err(AppError::NotFound(format!(
            "フォルダが見つかりません: {}",
            path.display()
        )));
    }
    Ok(Some(path))
}

/// 保存のダイアログで選んだパスに拡張子を補ったパスを、保存先にする
///
/// ダイアログが上書きを確かめたのは、選んだパスだけ。拡張子を補ったパスに別のファイルがある場合は、
/// 確かめずに置き換えないよう`VALIDATION`にする（選び直してもらう）。
/// @param chosen - ダイアログで選んだパス
/// @param completed - 拡張子を補ったパス（補う必要がなければ`chosen`と同じ）
fn completed_save_path(chosen: &Path, completed: PathBuf) -> AppResult<PathBuf> {
    // リンク切れのシンボリックリンクも、あるものとして扱う（たどった先へ書かない）
    if completed != chosen && std::fs::symlink_metadata(&completed).is_ok() {
        let name = completed
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Err(AppError::Validation(format!(
            "同じ名前のファイル（{}）があります。別の名前で保存してください",
            name
        )));
    }
    Ok(completed)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    /// 拡張子を補ったパスに別のファイルがあれば、確かめずに置き換えない
    #[test]
    fn test_completed_save_path_does_not_overwrite_unconfirmed_files() {
        let dir = TempDir::new();
        let chosen = dir.path().join("list.txt");
        let completed = dir.path().join("list.txt.m3u8");

        assert_eq!(
            completed_save_path(&chosen, completed.clone()).unwrap(),
            completed
        );
        std::fs::write(&completed, "前からあるファイル").unwrap();
        assert!(matches!(
            completed_save_path(&chosen, completed.clone()),
            Err(AppError::Validation(_))
        ));
        // ダイアログで選んだパスそのもの（上書きを確かめた）なら、そのまま
        assert_eq!(
            completed_save_path(&completed, completed.clone()).unwrap(),
            completed
        );
    }
}
