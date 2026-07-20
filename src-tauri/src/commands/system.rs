//! システム関連コマンド

use crate::error::{AppError, AppResult};
use crate::validation::validate_file_path;
use std::path::Path;

/// ファイルの場所をシステムのファイルマネージャーで開く
#[tauri::command]
pub async fn show_in_folder(path: String) -> AppResult<()> {
    use std::process::Command;

    // パスバリデーション（パストラバーサル攻撃対策）
    validate_file_path(&path)?;

    let file_path = Path::new(&path);

    if !file_path.exists() {
        return Err(AppError::NotFound("ファイルが見つかりません".to_string()));
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", file_path.to_str().unwrap()])
            .spawn()
            .map_err(|e| {
                AppError::Io(format!("ファイルマネージャーを開けませんでした: {}", e))
            })?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .args(["/select,", file_path.to_str().unwrap()])
            .spawn()
            .map_err(|e| {
                AppError::Io(format!("ファイルマネージャーを開けませんでした: {}", e))
            })?;
    }

    #[cfg(target_os = "linux")]
    {
        // 親ディレクトリを開く
        if let Some(parent) = file_path.parent() {
            Command::new("xdg-open").arg(parent).spawn().map_err(|e| {
                AppError::Io(format!("ファイルマネージャーを開けませんでした: {}", e))
            })?;
        }
    }

    Ok(())
}
