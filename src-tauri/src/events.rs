//! バックエンドからフロントエンドへ送るイベント
//!
//! tauri-spectaの型付きイベントとして定義し、`src/lib/bindings.ts`の`events`に
//! 型付きのリスナー（例: `events.importProgress.listen(...)`）が生成される。
//! イベント名は型名のケバブケース（例: `ImportProgress` → `import-progress`）。
//! 新しいイベントを追加したら`lib.rs`の`collect_events!`にも登録すること
//! （未登録のまま`emit`するとパニックする）。

use crate::error::AppError;
use crate::settings::Settings;
use serde::Serialize;
use specta::Type;
use tauri_specta::Event;

/// インポートの進捗
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    /// 処理済みファイル数
    pub current: u32,
    /// 総ファイル数
    pub total: u32,
    /// 現在処理中のファイル名
    pub current_file: String,
}

/// ライブラリフォルダの再スキャンの進捗
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct LibraryScanProgress {
    /// 読み込み済みファイル数（追加・変更のあったファイルのみ数える）
    pub current: u32,
    /// 読み込むファイルの総数
    pub total: u32,
    /// 現在処理中のファイル名
    pub current_file: String,
}

/// デバイスへの同期の進捗（コピー中のファイル）
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSyncProgress {
    /// 同期しているデバイス
    pub device_id: String,
    /// コピーが済んだファイル数
    pub current: u32,
    /// コピーするファイルの総数
    pub total: u32,
    /// コピーが済んだバイト数と、コピーするバイト数の合計
    #[specta(type = specta_typescript::Number)]
    pub bytes_done: i64,
    #[specta(type = specta_typescript::Number)]
    pub bytes_total: i64,
    /// コピー中のファイル名
    pub current_file: String,
}

/// 再スキャン・ライブラリフォルダの削除で、ライブラリのトラックが変わった
///
/// 設定ウィンドウでの操作を、メインウィンドウの一覧（Queryのキャッシュ）に反映する。
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct LibraryChanged;

/// メニュー「Muspice について」: Aboutダイアログを表示する
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct ShowAboutDialog;

/// メニュー「フォルダをインポート...」: インポートダイアログを開く
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct OpenImportDialog;

/// メニュー「サイドバーを表示/隠す」: サイドバーの表示を切り替える
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct ToggleSidebar;

/// 設定が保存された（設定ウィンドウでの変更をメインウィンドウに反映する）
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct SettingsChanged(pub Settings);

/// 再生エンジン（`playback`）からの通知
///
/// `token`は、フロントエンドが再生する曲ごとに振った番号（`playback_play` / `playback_set_next`で
/// 渡したもの）。曲を切り替えた後に届いた、前の曲の通知を見分けるために使う。
#[derive(Debug, Clone, PartialEq, Serialize, Type, Event)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlaybackEvent {
    /// 続けて再生する曲（`playback_set_next`で用意した曲）へ、切れ目なく切り替わった
    Advanced {
        token: u32,
        /// 曲の長さ（秒。ファイルから分からない場合はnull）
        duration: Option<f64>,
    },
    /// 再生位置（再生中に一定の間隔で届く。シークの直後にも届く）
    Position {
        token: u32,
        /// 曲の頭からの秒数
        position: f64,
    },
    /// 曲が最後まで鳴り終わり、続けて再生する曲がなかった
    Ended { token: u32 },
    /// 再生を続けられなくなった（ファイルを読めない・出力デバイスを使えないなど）。再生は止まっている
    Failed { token: u32, error: AppError },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// フロントエンド（`nativePlayback.svelte.ts`）は`type`で通知を見分ける
    #[test]
    fn test_playback_event_is_tagged_with_type() {
        let advanced = serde_json::to_value(PlaybackEvent::Advanced {
            token: 3,
            duration: Some(12.5),
        })
        .unwrap();
        assert_eq!(
            advanced,
            serde_json::json!({ "type": "advanced", "token": 3, "duration": 12.5 })
        );

        let failed = serde_json::to_value(PlaybackEvent::Failed {
            token: 4,
            error: AppError::Playback("再生できません".to_string()),
        })
        .unwrap();
        assert_eq!(
            failed,
            serde_json::json!({
                "type": "failed",
                "token": 4,
                "error": { "code": "PLAYBACK", "message": "再生できません" }
            })
        );
    }
}
