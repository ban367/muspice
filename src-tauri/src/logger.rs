use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// ログレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl LogLevel {
    fn as_str(&self) -> &str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warning => "WARNING",
            LogLevel::Error => "ERROR",
        }
    }
}

/// ロガー
pub struct Logger {
    log_file_path: PathBuf,
    min_level: LogLevel,
    file_mutex: Mutex<()>,
}

impl Logger {
    /// 新しいロガーを作成
    pub fn new(log_dir: PathBuf, min_level: LogLevel) -> Result<Self, std::io::Error> {
        // ログディレクトリが存在しない場合は作成
        fs::create_dir_all(&log_dir)?;

        // ログファイルのパスを設定（日付ごとに分ける）
        let date = Local::now().format("%Y-%m-%d").to_string();
        let log_file_path = log_dir.join(format!("muspice_{}.log", date));

        Ok(Logger {
            log_file_path,
            min_level,
            file_mutex: Mutex::new(()),
        })
    }

    /// ログを記録
    pub fn log(&self, level: LogLevel, message: &str) {
        // 最小レベル未満のログは無視
        if level < self.min_level {
            return;
        }

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let log_entry = format!("[{}] [{}] {}\n", timestamp, level.as_str(), message);

        // コンソールに出力
        match level {
            LogLevel::Error => eprintln!("{}", log_entry.trim()),
            _ => println!("{}", log_entry.trim()),
        }

        // ファイルに書き込み
        if let Err(e) = self.write_to_file(&log_entry) {
            eprintln!("ログファイルへの書き込みに失敗しました: {}", e);
        }
    }

    /// ファイルに書き込み
    fn write_to_file(&self, log_entry: &str) -> Result<(), std::io::Error> {
        // ファイルアクセスを排他制御
        let _lock = self.file_mutex.lock().unwrap();

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)?;

        file.write_all(log_entry.as_bytes())?;
        file.flush()?;

        Ok(())
    }

    /// デバッグログ
    #[allow(dead_code)]
    pub fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    /// 情報ログ
    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    /// 警告ログ
    #[allow(dead_code)]
    pub fn warning(&self, message: &str) {
        self.log(LogLevel::Warning, message);
    }

    /// エラーログ
    #[allow(dead_code)]
    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    /// エラーオブジェクトをログに記録
    #[allow(dead_code)]
    pub fn log_error<E: std::fmt::Display>(&self, context: &str, error: &E) {
        let message = format!("{}: {}", context, error);
        self.error(&message);
    }
}

/// グローバルロガーのインスタンス
static mut GLOBAL_LOGGER: Option<Logger> = None;

/// グローバルロガーを初期化
pub fn init_logger(log_dir: PathBuf, min_level: LogLevel) -> Result<(), std::io::Error> {
    let logger = Logger::new(log_dir, min_level)?;
    unsafe {
        GLOBAL_LOGGER = Some(logger);
    }
    Ok(())
}

/// グローバルロガーを取得
#[allow(static_mut_refs)]
fn get_logger() -> Option<&'static Logger> {
    unsafe { GLOBAL_LOGGER.as_ref() }
}

/// デバッグログを記録
#[allow(dead_code)]
pub fn debug(message: &str) {
    if let Some(logger) = get_logger() {
        logger.debug(message);
    }
}

/// 情報ログを記録
pub fn info(message: &str) {
    if let Some(logger) = get_logger() {
        logger.info(message);
    }
}

/// 警告ログを記録
#[allow(dead_code)]
pub fn warning(message: &str) {
    if let Some(logger) = get_logger() {
        logger.warning(message);
    }
}

/// エラーログを記録
#[allow(dead_code)]
pub fn error(message: &str) {
    if let Some(logger) = get_logger() {
        logger.error(message);
    }
}

/// エラーオブジェクトをログに記録
#[allow(dead_code)]
pub fn log_error<E: std::fmt::Display>(context: &str, error: &E) {
    if let Some(logger) = get_logger() {
        logger.log_error(context, error);
    }
}

/// ログマクロ
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logger::debug(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::info(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warning {
    ($($arg:tt)*) => {
        $crate::logger::warning(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::error(&format!($($arg)*))
    };
}
