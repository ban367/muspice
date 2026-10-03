//! バックエンドからフロントエンドへ送るイベント
//!
//! tauri-spectaの型付きイベントとして定義し、`src/lib/bindings.ts`の`events`に
//! 型付きのリスナー（例: `events.importProgress.listen(...)`）が生成される。
//! イベント名は型名のケバブケース（例: `ImportProgress` → `import-progress`）。
//! 新しいイベントを追加したら`lib.rs`の`collect_events!`にも登録すること
//! （未登録のまま`emit`するとパニックする）。

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
