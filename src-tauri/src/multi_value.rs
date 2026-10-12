//! 複数の値を持つ項目（アーティスト・ジャンル）の扱い
//!
//! 1曲に複数のアーティスト・ジャンルがある場合、タグには2通りの書き方がある（ADR-051）。
//!
//! - 同じ項目を複数持つ（Vorbisコメントの`ARTIST`が2つ、ID3v2.4の区切り付きの値など）
//! - 1つの値に、`;`で区切って書く（`Rock; Pop`）
//!
//! どちらも、値の一覧として扱う。データベース（`tracks.artist`・`tracks.genre`）には、値を`; `で
//! つないだ文字列で持ち（表示・検索・並べ替えはそのまま使える）、一覧の集計・絞り込みの時に、
//! ここで値に分ける。`/`・`,`・`&`では分けない（`AC/DC`・`Earth, Wind & Fire`は1つの値）。

use crate::search_text;
use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;

/// 値の区切り
pub const SEPARATOR: char = ';';

/// 値をつなぐ時の区切り（表示・保存用）
const JOINER: &str = "; ";

/// 値の一覧に、ある値があるかを調べるSQLの関数（`multi_value_contains(文字列, 値)`）
const CONTAINS_FUNCTION: &str = "multi_value_contains";

/// 値の一覧に、条件に合う値があるかを調べるSQLの関数（`multi_value_match(文字列, 比べ方, 語)`）
const MATCH_FUNCTION: &str = "multi_value_match";

/// 文字列を、値の一覧に分ける（前後の空白を除く。空の値と、同じ値の2つ目以降は除く）
pub fn split(text: &str) -> Vec<String> {
    let mut values: Vec<String> = Vec::new();
    for value in text.split(SEPARATOR).map(str::trim) {
        if !value.is_empty() && !values.iter().any(|known| known == value) {
            values.push(value.to_string());
        }
    }
    values
}

/// 同じ項目の値の一覧（タグに同じ項目が複数ある場合は、それぞれの値）を、値の一覧にする
pub fn values_of<'a>(items: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut values: Vec<String> = Vec::new();
    for value in items.into_iter().flat_map(split) {
        if !values.contains(&value) {
            values.push(value);
        }
    }
    values
}

/// 値の一覧を、1つの文字列にする（`; `でつなぐ）
pub fn join(values: &[String]) -> String {
    values.join(JOINER)
}

/// 同じ項目の値の一覧を、表示・保存用の文字列にする（値がなければNone）
///
/// 項目が1つで、区切りを含まない場合は、その値をそのまま返す（複数の値でない項目は、変えない）。
pub fn display(items: &[&str]) -> Option<String> {
    match items {
        [] => None,
        [single] if !single.contains(SEPARATOR) => Some((*single).to_string()),
        _ => {
            let values = values_of(items.iter().copied());
            (!values.is_empty()).then(|| join(&values))
        }
    }
}

/// 値の比べ方（`multi_value_match`の2つ目の引数）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatchOp {
    Is,
    Contains,
    StartsWith,
    EndsWith,
}

impl MatchOp {
    /// SQLに書く名前
    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Is => "is",
            Self::Contains => "contains",
            Self::StartsWith => "starts",
            Self::EndsWith => "ends",
        }
    }

    fn from_sql(name: &str) -> Option<Self> {
        match name {
            "is" => Some(Self::Is),
            "contains" => Some(Self::Contains),
            "starts" => Some(Self::StartsWith),
            "ends" => Some(Self::EndsWith),
            _ => None,
        }
    }

    fn matches(self, value: &str, needle: &str) -> bool {
        match self {
            Self::Is => value == needle,
            Self::Contains => value.contains(needle),
            Self::StartsWith => value.starts_with(needle),
            Self::EndsWith => value.ends_with(needle),
        }
    }
}

/// 値のどれかが、条件に合うか
///
/// `needle`は、検索と同じ正規化をした語（`search_text::normalize`）。値も、同じ正規化をして比べる。
/// 値に分ける前の文字列そのもの（`A; B`）が合う場合も、合うとみなす。
pub fn matches(text: &str, op: MatchOp, needle: &str) -> bool {
    op.matches(&search_text::normalize(text.trim()), needle)
        || split(text)
            .iter()
            .any(|value| op.matches(&search_text::normalize(value), needle))
}

/// SQLの関数を接続に登録する
///
/// - `multi_value_contains(文字列, 値)`: 値の一覧に、`値`と同じものがあるか（NULLは、ない）
/// - `multi_value_match(文字列, 比べ方, 語)`: 値のどれかが条件に合うか（`matches`を参照）
pub fn register(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function(CONTAINS_FUNCTION, 2, flags, |ctx| {
        let text = ctx.get::<Option<String>>(0)?;
        let value = ctx.get::<Option<String>>(1)?;
        Ok(match (text, value) {
            (Some(text), Some(value)) => split(&text).contains(&value),
            _ => false,
        })
    })?;
    conn.create_scalar_function(MATCH_FUNCTION, 3, flags, |ctx| {
        let text = ctx.get::<Option<String>>(0)?;
        let op = MatchOp::from_sql(&ctx.get::<String>(1)?);
        let needle = ctx.get::<Option<String>>(2)?;
        Ok(match (text, op, needle) {
            (Some(text), Some(op), Some(needle)) => matches(&text, op, &needle),
            _ => false,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_separates_on_semicolons_only() {
        assert_eq!(split("Rock; Pop"), ["Rock", "Pop"]);
        assert_eq!(split("Rock;Pop ;  Jazz"), ["Rock", "Pop", "Jazz"]);
        // `/`・`,`・`&`では分けない
        assert_eq!(split("AC/DC"), ["AC/DC"]);
        assert_eq!(
            split("Earth, Wind & Fire; Chicago"),
            ["Earth, Wind & Fire", "Chicago"]
        );
        // 空の値と、同じ値の2つ目以降は除く
        assert_eq!(split("Rock;; Rock ; "), ["Rock"]);
        assert!(split("").is_empty());
        assert!(split(" ; ").is_empty());
    }

    #[test]
    fn test_values_of_merges_repeated_items_and_separators() {
        assert_eq!(values_of(["A", "B"]), ["A", "B"]);
        assert_eq!(values_of(["A; B", "B", "C"]), ["A", "B", "C"]);
        assert!(values_of([]).is_empty());
    }

    #[test]
    fn test_display_joins_values_and_keeps_single_values() {
        assert_eq!(display(&[]), None);
        // 複数の値でない項目は、そのまま（前後の空白も変えない）
        assert_eq!(display(&[" AC/DC "]).as_deref(), Some(" AC/DC "));
        // 複数の値は、`; `でつなぐ
        assert_eq!(display(&["A", "B"]).as_deref(), Some("A; B"));
        assert_eq!(display(&["A;B"]).as_deref(), Some("A; B"));
        assert_eq!(display(&["A;"]).as_deref(), Some("A"));
        assert_eq!(display(&[";"]), None);
        assert_eq!(join(&split("A; B")), "A; B");
    }

    #[test]
    fn test_matches_compares_each_value_with_normalization() {
        let normalize = search_text::normalize;
        let text = "宇多田ヒカル; ZARD";

        assert!(matches(text, MatchOp::Is, &normalize("zard")));
        assert!(matches(text, MatchOp::Is, &normalize("宇多田ひかる")));
        assert!(!matches(text, MatchOp::Is, &normalize("宇多田")));
        // 分ける前の文字列そのものでも合う
        assert!(matches(text, MatchOp::Is, &normalize("宇多田ヒカル; ZARD")));
        assert!(matches(text, MatchOp::StartsWith, &normalize("za")));
        assert!(matches(text, MatchOp::EndsWith, &normalize("ヒカル")));
        assert!(matches(text, MatchOp::Contains, &normalize("AR")));
        assert!(!matches(text, MatchOp::Contains, &normalize("miles")));
    }

    #[test]
    fn test_sql_functions() {
        let conn = Connection::open_in_memory().unwrap();
        register(&conn).unwrap();
        let contains = |text: Option<&str>, value: &str| -> bool {
            conn.query_row(
                "SELECT multi_value_contains(?1, ?2)",
                rusqlite::params![text, value],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert!(contains(Some("Rock; Pop"), "Pop"));
        assert!(contains(Some("Rock"), "Rock"));
        // 大文字と小文字は区別し、値の一部では合わない
        assert!(!contains(Some("Rock; Pop"), "pop"));
        assert!(!contains(Some("Rock; Pop"), "Ro"));
        assert!(!contains(None, "Rock"));

        let matched = |text: Option<&str>, op: &str, needle: &str| -> bool {
            conn.query_row(
                "SELECT multi_value_match(?1, ?2, ?3)",
                rusqlite::params![text, op, needle],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert!(matched(Some("Rock; Pop"), "is", "pop"));
        assert!(matched(Some("Rock; Pop"), "starts", "po"));
        assert!(!matched(Some("Rock; Pop"), "is", "po"));
        assert!(!matched(None, "is", "pop"));
        assert!(!matched(Some("Rock"), "unknown", "rock"));
    }
}
