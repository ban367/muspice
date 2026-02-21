//! システム関連コマンド

use std::path::Path;

/// ファイルの場所をシステムのファイルマネージャーで開く
#[tauri::command]
pub async fn show_in_folder(path: String) -> Result<(), String> {
    use std::process::Command;

    let file_path = Path::new(&path);

    if !file_path.exists() {
        return Err("ファイルが見つかりません".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", file_path.to_str().unwrap()])
            .spawn()
            .map_err(|e| format!("ファイルマネージャーを開けませんでした: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .args(["/select,", file_path.to_str().unwrap()])
            .spawn()
            .map_err(|e| format!("ファイルマネージャーを開けませんでした: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // 親ディレクトリを開く
        if let Some(parent) = file_path.parent() {
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("ファイルマネージャーを開けませんでした: {}", e))?;
        }
    }

    Ok(())
}
