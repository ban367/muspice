//! 検索用の文字列の正規化
//!
//! 検索では、次の違いを同じとみなす（表示するタグの内容は変えず、検索用の文字列だけを正規化する）。
//!
//! - 大文字と小文字
//! - 全角と半角（「ａｉｋｏ」と「aiko」、「ｶﾌﾞﾄﾑｼ」と「カブトムシ」）
//! - ひらがなとカタカナ（「ひかる」と「ヒカル」）
//!
//! 全文検索の表（`tracks_fts`）には、タイトル・アーティスト・アルバム・ジャンル・アルバムアーティストを
//! 正規化してつないだ文字列を入れる。表の同期はトリガーで行い、トリガーからはSQLの関数
//! （`search_text`。`register`で接続に登録する）としてこの正規化を呼ぶ。検索語も同じ正規化をする。

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;
use unicode_normalization::UnicodeNormalization;

/// 項目をつなぐ区切り（検索語には入らない文字にし、項目をまたいだ一致を起こさない）
const FIELD_SEPARATOR: &str = "\n";

/// 全文検索の表に入れる文字列を作るSQLの関数の名前
pub const SQL_FUNCTION: &str = "search_text";

/// ひらがなとカタカナの文字コードの差
const KANA_OFFSET: u32 = 0x60;

/// カタカナをひらがなにする（対応するひらがながない文字はそのまま）
fn katakana_to_hiragana(c: char) -> char {
    match c {
        // ァ〜ヶ → ぁ〜ゖ、ヽヾ → ゝゞ
        'ァ'..='ヶ' | 'ヽ' | 'ヾ' => char::from_u32(u32::from(c) - KANA_OFFSET).unwrap_or(c),
        _ => c,
    }
}

/// 文字列を検索用に正規化する
///
/// 互換の文字をそろえ（NFKC。全角の英数字・半角のカタカナなど）、小文字にし、カタカナを
/// ひらがなにする。
pub fn normalize(text: &str) -> String {
    text.nfkc()
        .flat_map(char::to_lowercase)
        .map(katakana_to_hiragana)
        .collect()
}

/// 検索語を、空白で区切った語（正規化済み）にする
///
/// 検索では、すべての語を含む曲を探す。
pub fn query_terms(query: &str) -> Vec<String> {
    normalize(query)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// 項目（値のないものは除く）を正規化してつなぐ
fn join_fields<'a>(fields: impl Iterator<Item = Option<&'a str>>) -> String {
    fields
        .flatten()
        .map(normalize)
        .collect::<Vec<_>>()
        .join(FIELD_SEPARATOR)
}

/// SQLの関数`search_text(項目, ...)`を接続に登録する
///
/// 引数の文字列（NULLは除く）を正規化してつないだ文字列を返す。`tracks_fts`を同期する
/// トリガーが使うため、`tracks`に書き込む接続には必ず登録する（`db::run_migrations`が呼ぶ）。
pub fn register(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        SQL_FUNCTION,
        -1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let fields = (0..ctx.len())
                .map(|index| ctx.get::<Option<String>>(index))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(join_fields(fields.iter().map(Option::as_deref)))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_folds_case_width_and_kana() {
        // 大文字と小文字
        assert_eq!(normalize("The Beatles"), "the beatles");
        // 全角の英数字と、半角のカタカナ（濁点の合成を含む）
        assert_eq!(normalize("ａｉｋｏ　２０２４"), "aiko 2024");
        assert_eq!(normalize("ｶﾌﾞﾄﾑｼ"), normalize("カブトムシ"));
        // カタカナとひらがな（長音・漢字はそのまま）
        assert_eq!(normalize("宇多田ヒカル"), "宇多田ひかる");
        assert_eq!(normalize("スーパーカー"), "すーぱーかー");
        assert_eq!(normalize("ヴァイオリン"), "ゔぁいおりん");
        // すでに正規化されている文字列は変わらない
        assert_eq!(normalize("ひかる"), "ひかる");
    }

    #[test]
    fn test_query_terms_splits_on_whitespace() {
        assert_eq!(query_terms("Beatles  HELP"), ["beatles", "help"]);
        // 全角の空白でも区切る
        assert_eq!(query_terms("宇多田　ヒカル"), ["宇多田", "ひかる"]);
        assert!(query_terms("  　 ").is_empty());
    }

    #[test]
    fn test_sql_function_joins_normalized_fields() {
        let conn = Connection::open_in_memory().unwrap();
        register(&conn).unwrap();

        let text: String = conn
            .query_row(
                "SELECT search_text('First Love', '宇多田ヒカル', NULL, 'Ｊ－ＰＯＰ')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // 値のない項目は除き、項目は改行で区切る
        assert_eq!(text, "first love\n宇多田ひかる\nj-pop");
    }
}
