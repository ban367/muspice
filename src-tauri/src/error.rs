//! アプリケーション共通のエラー型
//!
//! Tauriコマンドの戻り値として `{ code, message }` 形式でシリアライズされ、
//! フロントエンド（`src/lib/stores/error.ts`）はcodeでエラーを分類する。
//! messageはユーザー向けの日本語メッセージを保持する。

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

/// アプリケーション全体で使用するエラー型
#[derive(Debug, Error)]
pub enum AppError {
    /// ロック取得の失敗（DBロック・ステートロック）
    #[error("{0}")]
    Lock(String),
    /// データベース操作の失敗
    #[error("{0}")]
    Database(String),
    /// 対象（トラック・プレイリスト・ファイル等）が見つからない
    #[error("{0}")]
    NotFound(String),
    /// 入力バリデーションエラー
    #[error("{0}")]
    Validation(String),
    /// ファイルI/Oエラー
    #[error("{0}")]
    Io(String),
    /// メタデータの抽出・書き込みエラー
    #[error("{0}")]
    Metadata(String),
}

impl AppError {
    /// フロントエンドでの分類に使用するエラーコード
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Lock(_) => "LOCK",
            AppError::Database(_) => "DATABASE",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::Validation(_) => "VALIDATION",
            AppError::Io(_) => "IO",
            AppError::Metadata(_) => "METADATA",
        }
    }
}

/// `{ code, message }` 形式でシリアライズする（Tauriコマンドのエラー戻り値用）
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

/// アプリケーション共通のResult型
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_as_code_and_message() {
        let error = AppError::NotFound("指定されたトラックが見つかりません".to_string());
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "NOT_FOUND");
        assert_eq!(json["message"], "指定されたトラックが見つかりません");
    }

    #[test]
    fn test_display_keeps_message() {
        let error = AppError::Database("クエリの実行に失敗しました: xxx".to_string());
        assert_eq!(error.to_string(), "クエリの実行に失敗しました: xxx");
    }
}
