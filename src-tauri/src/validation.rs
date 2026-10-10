use crate::error::{AppError, AppResult};
use std::path::Path;

/// ファイルパスをバリデーション（パストラバーサル攻撃対策）
pub fn validate_file_path(path: &str) -> AppResult<()> {
    // 空のパスをチェック
    if path.is_empty() {
        return Err(AppError::Validation("ファイルパスが空です".to_string()));
    }

    // Null文字をチェック
    if path.contains('\0') {
        return Err(AppError::Validation(
            "不正なファイルパス: Null文字が含まれています".to_string(),
        ));
    }

    // パスの長さをチェック（システム制限）
    if path.len() > 4096 {
        return Err(AppError::Validation("ファイルパスが長すぎます".to_string()));
    }

    // パストラバーサルパターンをチェック（パスコンポーネント単位で判定）
    // ファイル名に".."を含むケース（例: "Artist..Live.mp3"）は許可する
    let file_path = Path::new(path);
    for component in file_path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(AppError::Validation(
                "不正なファイルパス: 親ディレクトリへのアクセスは許可されていません".to_string(),
            ));
        }
    }

    Ok(())
}

/// プレイリスト名をバリデーション
pub fn validate_playlist_name(name: &str) -> AppResult<()> {
    // 空の名前をチェック
    if name.trim().is_empty() {
        return Err(AppError::Validation(
            "プレイリスト名を入力してください".to_string(),
        ));
    }

    // 長さをチェック
    if name.len() > 100 {
        return Err(AppError::Validation(
            "プレイリスト名は100文字以内で入力してください".to_string(),
        ));
    }

    // 危険な文字をチェック（ファイルシステムで問題となる文字）
    let dangerous_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    if name.chars().any(|c| dangerous_chars.contains(&c)) {
        return Err(AppError::Validation(
            "プレイリスト名に使用できない文字が含まれています".to_string(),
        ));
    }

    // Null文字をチェック
    if name.contains('\0') {
        return Err(AppError::Validation(
            "不正なプレイリスト名: Null文字が含まれています".to_string(),
        ));
    }

    Ok(())
}

/// プレイリストのフォルダの名前をバリデーション
pub fn validate_playlist_folder_name(name: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Validation(
            "フォルダ名を入力してください".to_string(),
        ));
    }
    if name.chars().count() > 100 {
        return Err(AppError::Validation(
            "フォルダ名は100文字以内で入力してください".to_string(),
        ));
    }
    if name.contains('\0') {
        return Err(AppError::Validation(
            "不正なフォルダ名: Null文字が含まれています".to_string(),
        ));
    }
    Ok(())
}

/// プレイリストの説明の長さの上限（文字数）
pub const MAX_PLAYLIST_DESCRIPTION_LENGTH: usize = 1000;

/// プレイリストの説明をバリデーション
pub fn validate_playlist_description(description: &str) -> AppResult<()> {
    if description.chars().count() > MAX_PLAYLIST_DESCRIPTION_LENGTH {
        return Err(AppError::Validation(
            "説明は1000文字以内で入力してください".to_string(),
        ));
    }
    if description.contains('\0') {
        return Err(AppError::Validation(
            "不正な説明: Null文字が含まれています".to_string(),
        ));
    }
    Ok(())
}

/// 検索クエリをサニタイズ（SQLインジェクション対策）
pub fn sanitize_search_query(query: &str) -> String {
    // 危険な文字を除去
    query
        .replace([';', '\'', '"', '\\'], "")
        .replace("--", "")
        .replace("/*", "")
        .replace("*/", "")
        .trim()
        .to_string()
}

/// トラックIDをバリデーション（UUID形式）
pub fn validate_track_id(id: &str) -> AppResult<()> {
    if id.is_empty() {
        return Err(AppError::Validation("トラックIDが空です".to_string()));
    }

    // UUID形式をチェック（簡易版）
    if id.len() != 36 {
        return Err(AppError::Validation("不正なトラックID形式です".to_string()));
    }

    // ハイフンの位置をチェック
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 5 {
        return Err(AppError::Validation("不正なトラックID形式です".to_string()));
    }

    // 各パートの長さをチェック
    if parts[0].len() != 8
        || parts[1].len() != 4
        || parts[2].len() != 4
        || parts[3].len() != 4
        || parts[4].len() != 12
    {
        return Err(AppError::Validation("不正なトラックID形式です".to_string()));
    }

    // 16進数文字のみを含むかチェック
    for part in parts {
        if !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AppError::Validation("不正なトラックID形式です".to_string()));
        }
    }

    Ok(())
}

/// プレイリストIDをバリデーション（UUID形式）
pub fn validate_playlist_id(id: &str) -> AppResult<()> {
    validate_track_id(id)
        .map_err(|_| AppError::Validation("不正なプレイリストID形式です".to_string()))
}

/// プレイリストのフォルダのIDをバリデーション（UUID形式）
pub fn validate_playlist_folder_id(id: &str) -> AppResult<()> {
    validate_track_id(id).map_err(|_| AppError::Validation("不正なフォルダID形式です".to_string()))
}

/// 文字列の長さをバリデーション
pub fn validate_string_length(
    value: &Option<String>,
    field_name: &str,
    max_length: usize,
) -> AppResult<()> {
    // 文字数で数える（バイト数で数えると、日本語は3分の1の長さで上限に達してしまう）
    if let Some(s) = value {
        let length = s.chars().count();
        if length > max_length {
            return Err(AppError::Validation(format!(
                "{}は{}文字以内で入力してください（現在: {}文字）",
                field_name, max_length, length
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_file_path_valid() {
        assert!(validate_file_path("/home/user/music/song.mp3").is_ok());
        assert!(validate_file_path("C:\\Users\\Music\\song.mp3").is_ok());
        // ファイル名に".."を含むケースは許可する
        assert!(validate_file_path("/home/user/music/Artist..Live.mp3").is_ok());
        assert!(validate_file_path("/home/user/music/track..2024.flac").is_ok());
    }

    #[test]
    fn test_validate_file_path_traversal() {
        assert!(validate_file_path("../../../etc/passwd").is_err());
        assert!(validate_file_path("/home/user/../../../etc/passwd").is_err());
        assert!(validate_file_path("/home/user/music/../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_file_path_null() {
        assert!(validate_file_path("/home/user/\0/file").is_err());
    }

    #[test]
    fn test_validate_playlist_name_valid() {
        assert!(validate_playlist_name("My Playlist").is_ok());
        assert!(validate_playlist_name("プレイリスト").is_ok());
    }

    #[test]
    fn test_validate_playlist_name_empty() {
        assert!(validate_playlist_name("").is_err());
        assert!(validate_playlist_name("   ").is_err());
    }

    #[test]
    fn test_validate_playlist_name_dangerous_chars() {
        assert!(validate_playlist_name("Playlist<script>").is_err());
        assert!(validate_playlist_name("Playlist/Name").is_err());
        assert!(validate_playlist_name("Playlist\\Name").is_err());
    }

    #[test]
    fn test_validate_playlist_folder_name() {
        assert!(validate_playlist_folder_name("外出").is_ok());
        // フォルダはファイルにしないため、記号も使える
        assert!(validate_playlist_folder_name("Rock / Pop").is_ok());
        assert!(validate_playlist_folder_name("あ".repeat(100).as_str()).is_ok());
        assert!(validate_playlist_folder_name("  ").is_err());
        assert!(validate_playlist_folder_name("あ".repeat(101).as_str()).is_err());
        assert!(validate_playlist_folder_name("a\0b").is_err());
    }

    #[test]
    fn test_validate_playlist_description() {
        assert!(validate_playlist_description("").is_ok());
        assert!(validate_playlist_description("1行目\n2行目").is_ok());
        assert!(validate_playlist_description(&"あ".repeat(1000)).is_ok());
        assert!(validate_playlist_description(&"あ".repeat(1001)).is_err());
        assert!(validate_playlist_description("a\0b").is_err());
    }

    #[test]
    fn test_sanitize_search_query() {
        assert_eq!(sanitize_search_query("normal query"), "normal query");
        assert_eq!(
            sanitize_search_query("query'; DROP TABLE--"),
            "query DROP TABLE"
        );
        assert_eq!(sanitize_search_query("query/*comment*/"), "querycomment");
    }

    #[test]
    fn test_validate_track_id_valid() {
        assert!(validate_track_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn test_validate_track_id_invalid() {
        assert!(validate_track_id("").is_err());
        assert!(validate_track_id("invalid-uuid").is_err());
        assert!(validate_track_id("550e8400-e29b-41d4-a716").is_err());
    }

    #[test]
    fn test_validate_string_length() {
        assert!(validate_string_length(&Some("short".to_string()), "field", 10).is_ok());
        assert!(validate_string_length(&Some("very long string".to_string()), "field", 5).is_err());
        assert!(validate_string_length(&None, "field", 10).is_ok());
        // 文字数で数える（日本語は1文字が3バイト）
        assert!(validate_string_length(&Some("あいうえお".to_string()), "field", 5).is_ok());
        assert!(validate_string_length(&Some("あいうえおか".to_string()), "field", 5).is_err());
    }
}
