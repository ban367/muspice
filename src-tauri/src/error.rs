//! アプリケーション共通のエラー型
//!
//! Tauriコマンドの戻り値として `{ code, message }` 形式でシリアライズされ、
//! フロントエンド（`src/lib/stores/error.ts`）はcodeでエラーを分類する。
//! messageはユーザー向けの日本語メッセージを保持する。

use serde::Serialize;
use thiserror::Error;

/// アプリケーション全体で使用するエラー型
///
/// serdeのadjacently tagged表現により `{ "code": "LOCK", "message": "..." }`
/// 形式でシリアライズされる。specta::Typeによりコードのリテラル型union
/// としてTypeScriptへエクスポートされる。
#[derive(Debug, Error, Serialize, specta::Type)]
#[serde(tag = "code", content = "message", rename_all = "SCREAMING_SNAKE_CASE")]
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
    fn test_serialize_lock_code() {
        let error = AppError::Lock("データベースロックの取得に失敗しました".to_string());
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "LOCK");
    }

    #[test]
    fn test_display_keeps_message() {
        let error = AppError::Database("クエリの実行に失敗しました: xxx".to_string());
        assert_eq!(error.to_string(), "クエリの実行に失敗しました: xxx");
    }
}
