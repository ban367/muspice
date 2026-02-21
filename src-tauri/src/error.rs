use thiserror::Error;

/// アプリケーション全体で使用するカスタムエラー型
#[derive(Error, Debug)]
pub enum AppError {
    /// ファイルが見つからない
    #[error("ファイルが見つかりません: {0}")]
    FileNotFound(String),
    /// サポートされていないファイル形式
    #[error("サポートされていないファイル形式です: {0}")]
    UnsupportedFormat(String),
    /// メタデータ抽出エラー
    #[error("メタデータの抽出に失敗しました: {0}")]
    MetadataExtraction(String),
    /// データベースエラー
    #[error("データベースエラー: {0}")]
    Database(String),
    /// 再生エラー
    #[error("再生エラー: {0}")]
    Playback(String),
    /// バリデーションエラー
    #[error("バリデーションエラー: {0}")]
    Validation(String),
    /// I/Oエラー
    #[error("I/Oエラー: {0}")]
    Io(#[from] std::io::Error),
    /// ロックエラー
    #[error("ロックエラー: {0}")]
    Lock(String),
    /// rusqliteエラー
    #[error("データベースエラー: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// loftyエラー
    #[error("メタデータの抽出に失敗しました: {0}")]
    Lofty(#[from] lofty::error::LoftyError),
    /// その他のエラー
    #[error("エラー: {0}")]
    Other(String),
}

/// Result型のエイリアス（将来の使用のため）
#[allow(dead_code)]
pub type AppResult<T> = Result<T, AppError>;

/// Stringからの変換（既存のエラーハンドリングとの互換性のため）
impl From<String> for AppError {
    fn from(err: String) -> Self {
        AppError::Other(err)
    }
}

/// &strからの変換
impl From<&str> for AppError {
    fn from(err: &str) -> Self {
        AppError::Other(err.to_string())
    }
}

/// AppErrorをStringに変換（Tauriコマンドの戻り値用）
impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.to_string()
    }
}

/// エラーハンドリングヘルパー関数（将来の使用のため）
#[allow(dead_code)]
pub mod helpers {
    use super::AppError;

    /// ファイルが存在するかチェック
    pub fn check_file_exists(path: &std::path::Path) -> Result<(), AppError> {
        if !path.exists() {
            return Err(AppError::FileNotFound(format!("{}", path.display())));
        }
        Ok(())
    }

    /// ファイル形式がサポートされているかチェック
    pub fn check_supported_format(extension: &str) -> Result<(), AppError> {
        let supported = ["mp3", "flac", "wav", "m4a"];
        if !supported.contains(&extension.to_lowercase().as_str()) {
            return Err(AppError::UnsupportedFormat(format!(
                "拡張子 '{}' はサポートされていません",
                extension
            )));
        }
        Ok(())
    }

    /// データベースロックを取得
    pub fn acquire_db_lock<T>(
        mutex: &std::sync::Mutex<T>,
    ) -> Result<std::sync::MutexGuard<'_, T>, AppError> {
        mutex
            .lock()
            .map_err(|e| AppError::Lock(format!("データベースロックの取得に失敗しました: {}", e)))
    }

    /// ステートロックを取得
    pub fn acquire_state_lock<T>(
        mutex: &std::sync::Mutex<T>,
    ) -> Result<std::sync::MutexGuard<'_, T>, AppError> {
        mutex
            .lock()
            .map_err(|e| AppError::Lock(format!("ステートロックの取得に失敗しました: {}", e)))
    }
}
