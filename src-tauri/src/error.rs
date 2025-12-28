use std::fmt;

/// アプリケーション全体で使用するカスタムエラー型
#[derive(Debug)]
pub enum AppError {
    /// ファイルが見つからない
    FileNotFound(String),
    /// サポートされていないファイル形式
    UnsupportedFormat(String),
    /// メタデータ抽出エラー
    MetadataExtraction(String),
    /// データベースエラー
    Database(String),
    /// 再生エラー
    Playback(String),
    /// バリデーションエラー
    Validation(String),
    /// I/Oエラー
    Io(String),
    /// ロックエラー
    Lock(String),
    /// その他のエラー
    Other(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::FileNotFound(msg) => write!(f, "ファイルが見つかりません: {}", msg),
            AppError::UnsupportedFormat(msg) => {
                write!(f, "サポートされていないファイル形式です: {}", msg)
            }
            AppError::MetadataExtraction(msg) => {
                write!(f, "メタデータの抽出に失敗しました: {}", msg)
            }
            AppError::Database(msg) => write!(f, "データベースエラー: {}", msg),
            AppError::Playback(msg) => write!(f, "再生エラー: {}", msg),
            AppError::Validation(msg) => write!(f, "バリデーションエラー: {}", msg),
            AppError::Io(msg) => write!(f, "I/Oエラー: {}", msg),
            AppError::Lock(msg) => write!(f, "ロックエラー: {}", msg),
            AppError::Other(msg) => write!(f, "エラー: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

/// Result型のエイリアス（将来の使用のため）
#[allow(dead_code)]
pub type AppResult<T> = Result<T, AppError>;

/// std::io::Errorからの変換
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => AppError::FileNotFound(err.to_string()),
            _ => AppError::Io(err.to_string()),
        }
    }
}

/// rusqlite::Errorからの変換
impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        match err {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::Database("データが見つかりません".to_string())
            }
            _ => AppError::Database(err.to_string()),
        }
    }
}

/// lofty::LoftyErrorからの変換
impl From<lofty::error::LoftyError> for AppError {
    fn from(err: lofty::error::LoftyError) -> Self {
        AppError::MetadataExtraction(err.to_string())
    }
}

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
