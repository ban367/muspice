//! ウィンドウへドロップされたファイルのパスの受け取り（macOS）
//!
//! Finderなどからウィンドウへドロップされたファイルのパスは、WebViewからは分からない。
//! HTML5のドロップ（`drop`）で得られるのは、名前と中身だけ。Tauriのファイルドロップ
//! （`dragDropEnabled`）はパスを渡してくれるが、有効にすると、ページの中の曲のドラッグ&ドロップが
//! 動かなくなる（ADR-018）。
//!
//! そこで、WebViewからは「ドロップされた」ことと名前だけを受け取り、パスは、OSのドラッグ用の
//! ペーストボードからRust側で読む（ドラッグが終わった後も、次のドラッグが始まるまで内容が残る）。
//! WebViewへパスを渡さず、WebViewからパスを受け取らない（ADR-005・ADR-050）。
//!
//! - ペーストボードの内容のうち、WebViewが伝えた名前のものだけを使う（別のドラッグの内容を、
//!   取り違えて取り込まない）
//! - 同じドラッグの内容は、1回だけ使う（ペーストボードの版で見分ける）

use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;
use unicode_normalization::UnicodeNormalization;

/// 最後に使った、ドラッグ用のペーストボードの版
#[derive(Default)]
pub struct DroppedFiles {
    last_change_count: Mutex<Option<i64>>,
}

/// ドロップされたファイル・フォルダのパスを返す
///
/// `names`は、WebViewのドロップのイベントで分かった名前（パスは含まない）。
pub fn take_dropped_paths(
    app: &AppHandle,
    state: &DroppedFiles,
    names: &[String],
) -> AppResult<Vec<PathBuf>> {
    let (change_count, paths) = read_drag_pasteboard(app)?;
    let mut last = state
        .last_change_count
        .lock()
        .map_err(|_| AppError::Lock("ドロップの状態を取得できません".to_string()))?;
    if *last == Some(change_count) {
        return Err(AppError::Validation(
            "このドロップは、すでにインポートしました".to_string(),
        ));
    }

    let matched = match_dropped_names(names, paths)?;
    *last = Some(change_count);
    Ok(matched)
}

/// 名前を比べる形にする（macOSのファイル名は、濁点などを分けた形で渡されることがある）
fn comparable(name: &str) -> String {
    name.nfc().collect()
}

/// ペーストボードのパスのうち、WebViewが伝えた名前のものを返す
///
/// 伝えられた名前が、どれもペーストボードにあることを確かめる（1つでもなければ、別のドラッグの
/// 内容とみなしてエラーにする）。
fn match_dropped_names(names: &[String], paths: Vec<PathBuf>) -> AppResult<Vec<PathBuf>> {
    let mismatch = || {
        AppError::Validation(
            "ドロップされたファイルを確認できませんでした。もう一度ドロップしてください"
                .to_string(),
        )
    };
    if names.is_empty() {
        return Err(mismatch());
    }

    let wanted: Vec<String> = names.iter().map(|name| comparable(name)).collect();
    let name_of = |path: &PathBuf| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(comparable)
    };
    let available: Vec<String> = paths.iter().filter_map(name_of).collect();
    if !wanted.iter().all(|name| available.contains(name)) {
        return Err(mismatch());
    }

    Ok(paths
        .into_iter()
        .filter(|path| name_of(path).is_some_and(|name| wanted.contains(&name)))
        .collect())
}

/// ドラッグ用のペーストボードの版と、入っているファイルのパスを読む
#[cfg(target_os = "macos")]
fn read_drag_pasteboard(app: &AppHandle) -> AppResult<(i64, Vec<PathBuf>)> {
    // AppKitのクラスのため、メインスレッドで読む（コマンドは別のスレッドで動く）
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(macos::read_drag_pasteboard());
    })
    .map_err(|e| AppError::Io(format!("ドロップされたファイルを読み取れません: {}", e)))?;
    receiver
        .recv()
        .map_err(|_| AppError::Io("ドロップされたファイルを読み取れません".to_string()))
}

#[cfg(not(target_os = "macos"))]
fn read_drag_pasteboard(_app: &AppHandle) -> AppResult<(i64, Vec<PathBuf>)> {
    Err(AppError::Validation(
        "ウィンドウへのドロップでのインポートは、macOSでだけ使えます。メニューからファイルを選んでください"
            .to_string(),
    ))
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2_app_kit::{NSPasteboard, NSPasteboardNameDrag, NSPasteboardTypeFileURL};
    use objc2_foundation::NSURL;
    use std::path::PathBuf;

    pub fn read_drag_pasteboard() -> (i64, Vec<PathBuf>) {
        // SAFETY: AppKitが定義している定数（ペーストボードの名前・種類）を読むだけ
        let (name, file_url_type) = unsafe { (NSPasteboardNameDrag, NSPasteboardTypeFileURL) };
        let pasteboard = NSPasteboard::pasteboardWithName(name);
        let change_count = pasteboard.changeCount() as i64;

        let mut paths = Vec::new();
        for item in pasteboard.pasteboardItems().iter().flatten() {
            let Some(url) = item
                .stringForType(file_url_type)
                .and_then(|url| NSURL::URLWithString(&url))
            else {
                continue;
            };
            // ファイルの参照の形（`file:///.file/id=...`）で入っていることがあるため、パスの形に直す
            let url = url.filePathURL().unwrap_or(url);
            if let Some(path) = url.path() {
                paths.push(PathBuf::from(path.to_string()));
            }
        }
        (change_count, paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(values: &[&str]) -> Vec<PathBuf> {
        values.iter().map(PathBuf::from).collect()
    }

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn test_returns_the_paths_with_the_reported_names() {
        let pasteboard = paths(&["/Music/a.flac", "/Music/Album", "/Downloads/b.mp3"]);

        assert_eq!(
            match_dropped_names(&names(&["a.flac", "Album", "b.mp3"]), pasteboard.clone()).unwrap(),
            pasteboard
        );
        // 伝えられた名前のものだけを返す（WebViewが、フォルダを伝えない場合など）
        assert_eq!(
            match_dropped_names(&names(&["b.mp3", "a.flac"]), pasteboard.clone()).unwrap(),
            paths(&["/Music/a.flac", "/Downloads/b.mp3"])
        );
    }

    #[test]
    fn test_rejects_names_that_are_not_on_the_pasteboard() {
        let pasteboard = paths(&["/Music/a.flac"]);

        // 別のドラッグの内容（名前が合わない）
        assert!(matches!(
            match_dropped_names(&names(&["other.flac"]), pasteboard.clone()),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            match_dropped_names(&names(&["a.flac", "other.flac"]), pasteboard.clone()),
            Err(AppError::Validation(_))
        ));
        // 名前がない・ペーストボードにファイルがない
        assert!(match_dropped_names(&[], pasteboard).is_err());
        assert!(match_dropped_names(&names(&["a.flac"]), Vec::new()).is_err());
    }

    /// 濁点などを分けた形（NFD）の名前も、同じ名前として扱う
    #[test]
    fn test_compares_names_regardless_of_unicode_normalization() {
        let decomposed = "カ\u{3099}ムラン.flac";
        let composed = "ガムラン.flac";
        assert_ne!(decomposed, composed);

        let pasteboard = vec![PathBuf::from(format!("/Music/{decomposed}"))];
        assert_eq!(
            match_dropped_names(&names(&[composed]), pasteboard.clone()).unwrap(),
            pasteboard
        );
    }
}
