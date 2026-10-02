//! バックエンドからフロントエンドへ送るイベント
//!
//! tauri-spectaの型付きイベントとして定義し、`src/lib/bindings.ts`の`events`に
//! 型付きのリスナー（例: `events.importProgress.listen(...)`）が生成される。
//! イベント名は型名のケバブケース（例: `ImportProgress` → `import-progress`）。
//! 新しいイベントを追加したら`lib.rs`の`collect_events!`にも登録すること
//! （未登録のまま`emit`するとパニックする）。

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

/// メニュー「Muspice について」: Aboutダイアログを表示する
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct ShowAboutDialog;

/// メニュー「フォルダをインポート...」: インポートダイアログを開く
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct OpenImportDialog;

/// メニュー「サイドバーを表示/隠す」: サイドバーの表示を切り替える
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct ToggleSidebar;
