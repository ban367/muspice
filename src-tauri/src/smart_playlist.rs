//! 自動プレイリスト（条件で曲を集めるプレイリスト）
//!
//! 条件（`SmartRules`）は、項目・演算子・値の組の一覧と、その組み合わせ方（すべて / いずれか）、
//! 並び順、曲数の上限からなる。条件は`playlists.rules`にJSONで保存し、曲の一覧は開くたびに
//! 条件から求め直す（曲の評価・再生回数などの変更に追随する）。
//!
//! 条件からSQLを作るが、項目・演算子は列挙型から決まった式を選び、値はすべてプレースホルダーで
//! 渡す（入力した文字列をSQLへ埋め込まない）。
//!
//! ランダムな並びは、種（`EvalContext::shuffle_seed`）と曲のIDから決める。種が同じ間は同じ並びに
//! なるため、曲の評価を変えた・1曲聴き終えたなどで一覧を取り直しても、並びは変わらない
//! （種は起動のたびに変わり、「選び直す」操作でも変わる。`AppState::reshuffle_smart_playlists`）。

use crate::error::{AppError, AppResult};
use crate::models::Track;
use crate::repository::{TRACK_COLUMNS, query_tracks};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, ToSql};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::hash::{DefaultHasher, Hash, Hasher};

/// 条件の数の上限
const MAX_RULES: usize = 50;

/// 文字列の条件の値の長さの上限（文字数）
const MAX_TEXT_LENGTH: usize = 255;

/// 数値の条件の値の上限（長さは秒、ビットレートはkbps）
const MAX_NUMBER: i32 = 1_000_000;

/// 日付の条件の日数の上限（100年）
const MAX_DAYS: u32 = 36_500;

/// 曲数の上限として指定できる最大の値
const MAX_LIMIT: u32 = 100_000;

/// 条件の組み合わせ方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MatchMode {
    /// すべての条件を満たす曲（条件がなければ、すべての曲）
    All,
    /// いずれかの条件を満たす曲（条件がなければ、曲なし）
    Any,
}

/// 文字列の項目
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TextField {
    Title,
    Artist,
    AlbumArtist,
    Album,
    Genre,
    /// ファイルの種類（mp3・flacなど）
    Format,
    /// ファイルの場所（フォルダ名での絞り込みに使う）
    Path,
}

/// 文字列の比べ方（大文字と小文字・全角と半角・ひらがなとカタカナは区別しない）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TextOp {
    Contains,
    NotContains,
    Is,
    IsNot,
    StartsWith,
    EndsWith,
    /// 値がない（タグがない）
    IsEmpty,
    IsNotEmpty,
}

/// 数値の項目
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum NumberField {
    /// 評価（星の数。0は評価なし）
    Rating,
    Year,
    PlayCount,
    SkipCount,
    /// 長さ（秒）
    Duration,
    TrackNumber,
    DiscNumber,
    /// ビットレート（kbps）
    Bitrate,
}

/// 数値の比べ方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum NumberOp {
    Is,
    IsNot,
    AtLeast,
    AtMost,
    /// `value`以上・`valueTo`以下
    Between,
}

/// 日付の項目
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DateField {
    /// 追加した日時
    CreatedAt,
    /// 最後に再生した日時
    LastPlayedAt,
}

/// 日付の比べ方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DateOp {
    /// 過去`days`日以内
    InLast,
    /// 過去`days`日より前（日付のない曲を含む。「長く聴いていない曲」には、未再生の曲も入る）
    NotInLast,
    /// 日付がない（未再生）
    IsEmpty,
    IsNotEmpty,
}

/// 1つの条件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SmartRule {
    /// 文字列の項目の条件（`IsEmpty`・`IsNotEmpty`では、`value`を使わない）
    Text {
        field: TextField,
        op: TextOp,
        value: String,
    },
    /// 数値の項目の条件（`value_to`は、`Between`の上端）
    Number {
        field: NumberField,
        op: NumberOp,
        value: i32,
        #[serde(rename = "valueTo")]
        value_to: Option<i32>,
    },
    /// 日付の項目の条件（`IsEmpty`・`IsNotEmpty`では、`days`を使わない）
    Date {
        field: DateField,
        op: DateOp,
        days: u32,
    },
    /// お気に入りかどうか
    Favorite { value: bool },
}

/// 並び順に使う項目
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartOrderField {
    /// ランダム（選び直すまで、同じ並びになる）
    Random,
    Title,
    Artist,
    Album,
    CreatedAt,
    LastPlayedAt,
    PlayCount,
    Rating,
    Year,
    Duration,
}

/// 並び順（上限がある場合は、この順の先頭から選ぶ）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SmartOrder {
    pub field: SmartOrderField,
    /// 大きい方（新しい方）から並べるか（`Random`では使わない）
    pub descending: bool,
}

/// 自動プレイリストの条件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SmartRules {
    pub match_mode: MatchMode,
    pub rules: Vec<SmartRule>,
    pub order: SmartOrder,
    /// 曲数の上限（Noneは上限なし）
    pub limit: Option<u32>,
}

impl SmartRules {
    /// 保存してある条件を読めなかった場合の条件（どの曲にも合わない）
    ///
    /// 新しいバージョンで足した項目を含む条件を、前のバージョンで開いた場合など。
    /// 通常のプレイリストとして扱うと曲を追加できてしまうため、自動プレイリストのまま、
    /// 曲のない一覧にする（条件を編集し直せる）。
    pub fn unreadable() -> Self {
        Self {
            match_mode: MatchMode::Any,
            rules: Vec::new(),
            order: SmartOrder {
                field: SmartOrderField::CreatedAt,
                descending: true,
            },
            limit: None,
        }
    }
}

/// 条件から曲を求める時の状況
#[derive(Debug, Clone, Copy)]
pub struct EvalContext {
    /// 現在の日時（日付の条件「過去N日以内」の基準）
    pub now: DateTime<Utc>,
    /// ランダムな並びの種（同じ種なら、同じ並びになる）
    pub shuffle_seed: u64,
}

/// 条件を検証する
pub fn validate_rules(rules: &SmartRules) -> AppResult<()> {
    let invalid = |message: &str| Err(AppError::Validation(message.to_string()));

    if rules.rules.len() > MAX_RULES {
        return invalid("条件が多すぎます（50個まで）");
    }
    if rules
        .limit
        .is_some_and(|limit| limit == 0 || limit > MAX_LIMIT)
    {
        return invalid("曲数の上限は1から100000の範囲で指定してください");
    }

    for rule in &rules.rules {
        match rule {
            SmartRule::Text { op, value, .. } => {
                let needs_value = !matches!(op, TextOp::IsEmpty | TextOp::IsNotEmpty);
                if needs_value && value.trim().is_empty() {
                    return invalid("条件の値を入力してください");
                }
                if value.chars().count() > MAX_TEXT_LENGTH {
                    return invalid("条件の値が長すぎます（255文字まで）");
                }
            }
            SmartRule::Number {
                field,
                op,
                value,
                value_to,
            } => {
                let max = if *field == NumberField::Rating {
                    5
                } else {
                    MAX_NUMBER
                };
                let in_range = |number: i32| (0..=max).contains(&number);
                if !in_range(*value) {
                    return invalid("条件の数値が範囲の外です");
                }
                if *op == NumberOp::Between {
                    match value_to {
                        Some(value_to) if in_range(*value_to) && value_to >= value => {}
                        _ => {
                            return invalid(
                                "範囲の条件は、小さい値と大きい値の順に指定してください",
                            );
                        }
                    }
                }
            }
            SmartRule::Date { op, days, .. } => {
                let needs_days = matches!(op, DateOp::InLast | DateOp::NotInLast);
                if needs_days && !(1..=MAX_DAYS).contains(days) {
                    return invalid("日数は1から36500の範囲で指定してください");
                }
            }
            SmartRule::Favorite { .. } => {}
        }
    }
    Ok(())
}

/// SQLの条件式と、そのプレースホルダーに渡す値
struct Condition {
    sql: String,
    params: Vec<Box<dyn ToSql>>,
}

impl Condition {
    fn new(sql: impl Into<String>, params: Vec<Box<dyn ToSql>>) -> Self {
        Self {
            sql: sql.into(),
            params,
        }
    }
}

/// LIKEのパターンで特別な意味を持つ文字をエスケープする（`ESCAPE '\'`と組で使う）
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn text_column(field: TextField) -> &'static str {
    match field {
        TextField::Title => "title",
        TextField::Artist => "artist",
        TextField::AlbumArtist => "album_artist",
        TextField::Album => "album",
        TextField::Genre => "genre",
        TextField::Format => "format",
        TextField::Path => "file_path",
    }
}

fn number_column(field: NumberField) -> &'static str {
    match field {
        NumberField::Rating => "COALESCE(rating, 0)",
        NumberField::Year => "year",
        NumberField::PlayCount => "COALESCE(play_count, 0)",
        NumberField::SkipCount => "COALESCE(skip_count, 0)",
        NumberField::Duration => "duration",
        NumberField::TrackNumber => "track_number",
        NumberField::DiscNumber => "disc_number",
        NumberField::Bitrate => "bitrate",
    }
}

fn date_column(field: DateField) -> &'static str {
    match field {
        DateField::CreatedAt => "created_at",
        DateField::LastPlayedAt => "last_played_at",
    }
}

/// 1つの条件を、SQLの条件式にする
fn rule_condition(rule: &SmartRule, now: DateTime<Utc>) -> Condition {
    match rule {
        SmartRule::Text { field, op, value } => {
            let column = text_column(*field);
            // 検索と同じ正規化（大文字と小文字・全角と半角・ひらがなとカタカナをそろえる）をして比べる
            let normalized = format!("search_text({column})");
            let needle = crate::search_text::normalize(value.trim());
            let like = |pattern: String, negate: bool| {
                Condition::new(
                    format!(
                        "{normalized} {}LIKE ? ESCAPE '\\'",
                        if negate { "NOT " } else { "" }
                    ),
                    vec![Box::new(pattern)],
                )
            };
            match op {
                TextOp::Contains => like(format!("%{}%", escape_like(&needle)), false),
                TextOp::NotContains => like(format!("%{}%", escape_like(&needle)), true),
                TextOp::StartsWith => like(format!("{}%", escape_like(&needle)), false),
                TextOp::EndsWith => like(format!("%{}", escape_like(&needle)), false),
                TextOp::Is => Condition::new(format!("{normalized} = ?"), vec![Box::new(needle)]),
                TextOp::IsNot => {
                    Condition::new(format!("{normalized} != ?"), vec![Box::new(needle)])
                }
                TextOp::IsEmpty => {
                    Condition::new(format!("({column} IS NULL OR TRIM({column}) = '')"), vec![])
                }
                TextOp::IsNotEmpty => Condition::new(
                    format!("({column} IS NOT NULL AND TRIM({column}) != '')"),
                    vec![],
                ),
            }
        }
        SmartRule::Number {
            field,
            op,
            value,
            value_to,
        } => {
            let column = number_column(*field);
            match op {
                NumberOp::Is => Condition::new(format!("{column} = ?"), vec![Box::new(*value)]),
                // 値のない曲（年のない曲など）は、「その値ではない」に含める
                NumberOp::IsNot => {
                    Condition::new(format!("{column} IS NOT ?"), vec![Box::new(*value)])
                }
                NumberOp::AtLeast => {
                    Condition::new(format!("{column} >= ?"), vec![Box::new(*value)])
                }
                NumberOp::AtMost => {
                    Condition::new(format!("{column} <= ?"), vec![Box::new(*value)])
                }
                NumberOp::Between => Condition::new(
                    format!("{column} BETWEEN ? AND ?"),
                    vec![Box::new(*value), Box::new(value_to.unwrap_or(*value))],
                ),
            }
        }
        SmartRule::Date { field, op, days } => {
            let column = date_column(*field);
            // 日時は、書式（タイムゾーンの表記など）をそろえてから比べる
            let cutoff = (now - Duration::days(i64::from(*days))).to_rfc3339();
            match op {
                DateOp::InLast => Condition::new(
                    format!("datetime({column}) >= datetime(?)"),
                    vec![Box::new(cutoff)],
                ),
                DateOp::NotInLast => Condition::new(
                    format!("({column} IS NULL OR datetime({column}) < datetime(?))"),
                    vec![Box::new(cutoff)],
                ),
                DateOp::IsEmpty => Condition::new(format!("{column} IS NULL"), vec![]),
                DateOp::IsNotEmpty => Condition::new(format!("{column} IS NOT NULL"), vec![]),
            }
        }
        SmartRule::Favorite { value } => Condition::new(
            "COALESCE(is_favorite, 0) = ?",
            vec![Box::new(i32::from(*value))],
        ),
    }
}

/// 条件の一覧を、1つのSQLの条件式にまとめる
fn where_condition(rules: &SmartRules, now: DateTime<Utc>) -> Condition {
    if rules.rules.is_empty() {
        // 「すべてを満たす」で条件がなければすべての曲、「いずれかを満たす」で条件がなければ曲なし
        let sql = match rules.match_mode {
            MatchMode::All => "1",
            MatchMode::Any => "0",
        };
        return Condition::new(sql, vec![]);
    }

    let separator = match rules.match_mode {
        MatchMode::All => " AND ",
        MatchMode::Any => " OR ",
    };
    let mut parts = Vec::with_capacity(rules.rules.len());
    let mut params = Vec::new();
    for rule in &rules.rules {
        let condition = rule_condition(rule, now);
        parts.push(format!("({})", condition.sql));
        params.extend(condition.params);
    }
    Condition::new(parts.join(separator), params)
}

/// 並び順のSQL（同じ値の曲は、アーティスト → アルバム → アルバムの中の並びにする）
fn order_sql(order: SmartOrder) -> String {
    let expression = match order.field {
        // ランダムな並びは、取得したあとで決める（`find_tracks`）
        SmartOrderField::Random => return "id".to_string(),
        SmartOrderField::Title => "search_text(COALESCE(title_sort, title, file_name))",
        SmartOrderField::Artist => "search_text(COALESCE(artist_sort, artist))",
        SmartOrderField::Album => "search_text(COALESCE(album_sort, album))",
        SmartOrderField::CreatedAt => "datetime(created_at)",
        SmartOrderField::LastPlayedAt => "datetime(last_played_at)",
        SmartOrderField::PlayCount => "COALESCE(play_count, 0)",
        SmartOrderField::Rating => "COALESCE(rating, 0)",
        SmartOrderField::Year => "year",
        SmartOrderField::Duration => "duration",
    };
    let direction = if order.descending { "DESC" } else { "ASC" };
    format!(
        "{expression} {direction}, COALESCE(album_artist, artist), album,
         COALESCE(disc_number, 1), track_number, title, id"
    )
}

/// ランダムな並びでの、曲の位置を決める値（種と曲のIDから決まる）
fn shuffle_key(seed: u64, track_id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    track_id.hash(&mut hasher);
    hasher.finish()
}

/// 条件に合う曲を、並び順のとおりに（上限があれば、その曲数まで）取得する
pub fn find_tracks(
    conn: &Connection,
    rules: &SmartRules,
    context: EvalContext,
) -> AppResult<Vec<Track>> {
    let condition = where_condition(rules, context.now);
    let is_random = rules.order.field == SmartOrderField::Random;
    let mut sql = format!(
        "SELECT {TRACK_COLUMNS} FROM tracks WHERE {} ORDER BY {}",
        condition.sql,
        order_sql(rules.order)
    );
    let mut params = condition.params;
    // ランダムな並びでは、並べてから上限の曲数にする
    if let Some(limit) = rules.limit.filter(|_| !is_random) {
        sql.push_str(" LIMIT ?");
        params.push(Box::new(limit));
    }

    let params: Vec<&dyn ToSql> = params.iter().map(|param| param.as_ref()).collect();
    let mut tracks = query_tracks(conn, &sql, &params)?;
    if is_random {
        tracks.sort_by_cached_key(|track| shuffle_key(context.shuffle_seed, &track.id));
        if let Some(limit) = rules.limit {
            tracks.truncate(limit as usize);
        }
    }
    Ok(tracks)
}

/// 条件に合う曲数（上限があれば、それを超えない）を数える（条件の編集画面で、入力のたびに出す）
pub fn count_tracks(conn: &Connection, rules: &SmartRules, now: DateTime<Utc>) -> AppResult<u32> {
    let condition = where_condition(rules, now);
    let sql = format!("SELECT COUNT(*) FROM tracks WHERE {}", condition.sql);
    let params: Vec<&dyn ToSql> = condition
        .params
        .iter()
        .map(|param| param.as_ref())
        .collect();
    let count: u32 = conn
        .query_row(&sql, params.as_slice(), |row| row.get(0))
        .map_err(|e| AppError::Database(format!("曲数の取得に失敗しました: {}", e)))?;
    Ok(rules.limit.map_or(count, |limit| count.min(limit)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    /// 曲を入れる（指定しない項目は、既定の値）
    fn insert(conn: &Connection, id: &str, artist: &str, album: &str, genre: Option<&str>) {
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, title, artist, album, genre, format,
                                 file_size, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?1, ?4, ?5, ?6, 'mp3', 1,
                     '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
            rusqlite::params![
                id,
                format!("/music/{artist}/{album}/{id}.mp3"),
                format!("{id}.mp3"),
                artist,
                album,
                genre
            ],
        )
        .unwrap();
    }

    fn set(conn: &Connection, id: &str, column: &str, value: &dyn ToSql) {
        // テスト用のため、列の名前は固定の文字列だけを渡す
        conn.execute(
            &format!("UPDATE tracks SET {column} = ?2 WHERE id = ?1"),
            rusqlite::params![id, value],
        )
        .unwrap();
    }

    fn rules(match_mode: MatchMode, rules: Vec<SmartRule>) -> SmartRules {
        SmartRules {
            match_mode,
            rules,
            order: SmartOrder {
                field: SmartOrderField::Title,
                descending: false,
            },
            limit: None,
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-10T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn context(shuffle_seed: u64) -> EvalContext {
        EvalContext {
            now: now(),
            shuffle_seed,
        }
    }

    fn ids(conn: &Connection, rules: &SmartRules) -> Vec<String> {
        find_tracks(conn, rules, context(1))
            .unwrap()
            .into_iter()
            .map(|track| track.id)
            .collect()
    }

    fn text(field: TextField, op: TextOp, value: &str) -> SmartRule {
        SmartRule::Text {
            field,
            op,
            value: value.to_string(),
        }
    }

    fn number(field: NumberField, op: NumberOp, value: i32) -> SmartRule {
        SmartRule::Number {
            field,
            op,
            value,
            value_to: None,
        }
    }

    fn library() -> Connection {
        let conn = setup();
        insert(&conn, "a", "宇多田ヒカル", "First Love", Some("J-Pop"));
        insert(&conn, "b", "ZARD", "揺れる想い", Some("J-Pop"));
        insert(&conn, "c", "Miles Davis", "Kind of Blue", Some("Jazz"));
        insert(&conn, "d", "Bill Evans", "Waltz for Debby", None);
        conn
    }

    #[test]
    fn test_empty_rules_match_all_or_nothing() {
        let conn = library();

        assert_eq!(ids(&conn, &rules(MatchMode::All, vec![])).len(), 4);
        assert!(ids(&conn, &rules(MatchMode::Any, vec![])).is_empty());
        assert!(ids(&conn, &SmartRules::unreadable()).is_empty());
    }

    #[test]
    fn test_text_rules_ignore_case_width_and_kana() {
        let conn = library();
        let find = |field, op, value: &str| {
            ids(&conn, &rules(MatchMode::All, vec![text(field, op, value)]))
        };

        // ひらがな・全角・小文字で書いても一致する
        assert_eq!(find(TextField::Artist, TextOp::Contains, "ひかる"), ["a"]);
        assert_eq!(find(TextField::Artist, TextOp::Is, "ｚａｒｄ"), ["b"]);
        assert_eq!(find(TextField::Artist, TextOp::StartsWith, "miles"), ["c"]);
        assert_eq!(find(TextField::Album, TextOp::EndsWith, "BLUE"), ["c"]);
        assert_eq!(
            find(TextField::Genre, TextOp::IsNot, "j-pop"),
            // ジャンルのない曲は、「J-Popではない」に含める
            ["c", "d"]
        );
        assert_eq!(
            find(TextField::Genre, TextOp::NotContains, "pop"),
            ["c", "d"]
        );
        assert_eq!(find(TextField::Genre, TextOp::IsEmpty, ""), ["d"]);
        assert_eq!(find(TextField::Genre, TextOp::IsNotEmpty, "").len(), 3);
        // ファイルの場所（フォルダ名）・ファイルの種類でも絞り込める
        assert_eq!(find(TextField::Path, TextOp::Contains, "/zard/"), ["b"]);
        assert_eq!(find(TextField::Format, TextOp::Is, "MP3").len(), 4);
        assert!(find(TextField::Format, TextOp::Is, "flac").is_empty());
    }

    #[test]
    fn test_text_rules_treat_like_wildcards_as_plain_characters() {
        let conn = setup();
        insert(&conn, "a", "100% Pure", "A_B", None);
        insert(&conn, "b", "100 Proof", "AxB", None);

        assert_eq!(
            ids(
                &conn,
                &rules(
                    MatchMode::All,
                    vec![text(TextField::Artist, TextOp::Contains, "100%")]
                )
            ),
            ["a"]
        );
        assert_eq!(
            ids(
                &conn,
                &rules(
                    MatchMode::All,
                    vec![text(TextField::Album, TextOp::Is, "a_b")]
                )
            ),
            ["a"]
        );
    }

    #[test]
    fn test_number_rules() {
        let conn = library();
        set(&conn, "a", "rating", &5);
        set(&conn, "b", "rating", &4);
        set(&conn, "c", "rating", &2);
        set(&conn, "a", "year", &1999);
        set(&conn, "c", "year", &1959);
        set(&conn, "a", "play_count", &30);
        let find = |rule| ids(&conn, &rules(MatchMode::All, vec![rule]));

        assert_eq!(
            find(number(NumberField::Rating, NumberOp::AtLeast, 4)),
            ["a", "b"]
        );
        // 評価のない曲は、0として扱う
        assert_eq!(
            find(number(NumberField::Rating, NumberOp::AtMost, 2)),
            ["c", "d"]
        );
        assert_eq!(find(number(NumberField::Rating, NumberOp::Is, 0)), ["d"]);
        assert_eq!(find(number(NumberField::Year, NumberOp::Is, 1999)), ["a"]);
        // 年のない曲は、「その年ではない」に含め、「以上・以下・範囲」には含めない
        assert_eq!(
            find(number(NumberField::Year, NumberOp::IsNot, 1999)),
            ["b", "c", "d"]
        );
        assert_eq!(
            find(number(NumberField::Year, NumberOp::AtMost, 1999)),
            ["a", "c"]
        );
        assert_eq!(
            find(SmartRule::Number {
                field: NumberField::Year,
                op: NumberOp::Between,
                value: 1950,
                value_to: Some(1960),
            }),
            ["c"]
        );
        assert_eq!(
            find(number(NumberField::PlayCount, NumberOp::Is, 0)),
            ["b", "c", "d"]
        );
    }

    #[test]
    fn test_date_rules() {
        let conn = library();
        // 追加日: aは3日前、bは40日前（書式の違う日時も比べられる）
        set(&conn, "a", "created_at", &"2026-10-07T00:00:00+00:00");
        set(&conn, "b", "created_at", &"2026-08-31 00:00:00");
        // 最終再生日: aは昨日（タイムゾーン付き）、cは100日前、ほかは未再生
        set(&conn, "a", "last_played_at", &"2026-10-09T12:00:00+09:00");
        set(&conn, "c", "last_played_at", &"2026-07-02T00:00:00+00:00");
        let find = |field, op, days| {
            ids(
                &conn,
                &rules(MatchMode::All, vec![SmartRule::Date { field, op, days }]),
            )
        };

        assert_eq!(find(DateField::CreatedAt, DateOp::InLast, 7), ["a"]);
        assert_eq!(find(DateField::CreatedAt, DateOp::InLast, 60), ["a", "b"]);
        assert_eq!(
            find(DateField::CreatedAt, DateOp::NotInLast, 60),
            ["c", "d"]
        );
        assert_eq!(find(DateField::LastPlayedAt, DateOp::InLast, 7), ["a"]);
        // 「長く聴いていない曲」には、未再生の曲も入る
        assert_eq!(
            find(DateField::LastPlayedAt, DateOp::NotInLast, 30),
            ["b", "c", "d"]
        );
        assert_eq!(
            find(DateField::LastPlayedAt, DateOp::IsEmpty, 0),
            ["b", "d"]
        );
        assert_eq!(
            find(DateField::LastPlayedAt, DateOp::IsNotEmpty, 0),
            ["a", "c"]
        );
    }

    #[test]
    fn test_favorite_rule_and_match_modes() {
        let conn = library();
        set(&conn, "a", "is_favorite", &1);
        set(&conn, "c", "is_favorite", &1);
        let favorite = SmartRule::Favorite { value: true };
        let jazz = text(TextField::Genre, TextOp::Is, "Jazz");

        assert_eq!(
            ids(&conn, &rules(MatchMode::All, vec![favorite.clone()])),
            ["a", "c"]
        );
        assert_eq!(
            ids(
                &conn,
                &rules(MatchMode::All, vec![SmartRule::Favorite { value: false }])
            ),
            ["b", "d"]
        );
        // すべてを満たす / いずれかを満たす
        assert_eq!(
            ids(
                &conn,
                &rules(MatchMode::All, vec![favorite.clone(), jazz.clone()])
            ),
            ["c"]
        );
        let any = text(TextField::Artist, TextOp::Is, "ZARD");
        assert_eq!(
            ids(&conn, &rules(MatchMode::Any, vec![jazz, any])),
            ["b", "c"]
        );
    }

    #[test]
    fn test_order_and_limit() {
        let conn = library();
        set(&conn, "a", "play_count", &30);
        set(&conn, "b", "play_count", &10);
        set(&conn, "c", "play_count", &20);
        let ordered = |field, descending, limit| {
            ids(
                &conn,
                &SmartRules {
                    match_mode: MatchMode::All,
                    rules: vec![],
                    order: SmartOrder { field, descending },
                    limit,
                },
            )
        };

        assert_eq!(
            ordered(SmartOrderField::PlayCount, true, None),
            ["a", "c", "b", "d"]
        );
        // 上限は、並び順の先頭から選ぶ
        assert_eq!(
            ordered(SmartOrderField::PlayCount, true, Some(2)),
            ["a", "c"]
        );
        assert_eq!(ordered(SmartOrderField::PlayCount, false, Some(1)), ["d"]);
        // 名前の並びは、大文字と小文字を区別しない
        assert_eq!(
            ordered(SmartOrderField::Artist, false, None),
            ["d", "c", "b", "a"]
        );
    }

    /// ランダムな並びは、種が同じ間は変わらず、種を変えると変わる
    #[test]
    fn test_random_order_is_stable_for_a_seed() {
        let conn = setup();
        for index in 0..40 {
            insert(&conn, &format!("t{index:02}"), "Artist", "Album", None);
        }
        let random = |seed, limit| {
            let rules = SmartRules {
                match_mode: MatchMode::All,
                rules: vec![],
                order: SmartOrder {
                    field: SmartOrderField::Random,
                    descending: false,
                },
                limit,
            };
            find_tracks(&conn, &rules, context(seed))
                .unwrap()
                .into_iter()
                .map(|track| track.id)
                .collect::<Vec<_>>()
        };

        let first = random(1, None);
        // 合う曲がすべて入り、IDの順のままではない
        let mut sorted = first.clone();
        sorted.sort();
        assert_eq!(sorted.len(), 40);
        assert_ne!(first, sorted);
        // 同じ種なら同じ並び。上限は、その並びの先頭から選ぶ
        assert_eq!(random(1, None), first);
        assert_eq!(random(1, Some(5)), first[..5]);
        // 種を変えると、並びが変わる
        assert_ne!(random(2, None), first);

        // 曲が増えても、ほかの曲どうしの順は変わらない
        insert(&conn, "new", "Artist", "Album", None);
        let mut with_new = random(1, None);
        with_new.retain(|id| id != "new");
        assert_eq!(with_new, first);
    }

    #[test]
    fn test_count_tracks_respects_the_limit() {
        let conn = library();
        let mut all = rules(MatchMode::All, vec![]);

        assert_eq!(count_tracks(&conn, &all, now()).unwrap(), 4);
        all.limit = Some(3);
        assert_eq!(count_tracks(&conn, &all, now()).unwrap(), 3);
        all.limit = Some(10);
        assert_eq!(count_tracks(&conn, &all, now()).unwrap(), 4);
        let jazz = rules(
            MatchMode::All,
            vec![text(TextField::Genre, TextOp::Is, "jazz")],
        );
        assert_eq!(count_tracks(&conn, &jazz, now()).unwrap(), 1);
    }

    #[test]
    fn test_validate_rules() {
        let valid = |rule| validate_rules(&rules(MatchMode::All, vec![rule])).is_ok();

        assert!(validate_rules(&rules(MatchMode::All, vec![])).is_ok());
        assert!(valid(text(TextField::Title, TextOp::Contains, "a")));
        // 値のいらない演算子は、空でもよい
        assert!(valid(text(TextField::Genre, TextOp::IsEmpty, "")));
        assert!(!valid(text(TextField::Title, TextOp::Contains, "  ")));
        assert!(!valid(text(
            TextField::Title,
            TextOp::Contains,
            &"あ".repeat(256)
        )));

        assert!(valid(number(NumberField::Rating, NumberOp::AtLeast, 5)));
        assert!(!valid(number(NumberField::Rating, NumberOp::AtLeast, 6)));
        assert!(!valid(number(NumberField::Year, NumberOp::Is, -1)));
        let between = |value, value_to| SmartRule::Number {
            field: NumberField::Year,
            op: NumberOp::Between,
            value,
            value_to,
        };
        assert!(valid(between(1990, Some(1999))));
        assert!(!valid(between(1999, Some(1990))));
        assert!(!valid(between(1990, None)));

        let date = |op, days| SmartRule::Date {
            field: DateField::CreatedAt,
            op,
            days,
        };
        assert!(valid(date(DateOp::InLast, 30)));
        assert!(!valid(date(DateOp::InLast, 0)));
        assert!(valid(date(DateOp::IsEmpty, 0)));

        let mut limited = rules(MatchMode::All, vec![]);
        limited.limit = Some(0);
        assert!(validate_rules(&limited).is_err());
        limited.limit = Some(25);
        assert!(validate_rules(&limited).is_ok());
        let too_many = rules(
            MatchMode::All,
            vec![SmartRule::Favorite { value: true }; MAX_RULES + 1],
        );
        assert!(validate_rules(&too_many).is_err());
    }

    #[test]
    fn test_rules_serialize_as_tagged_camel_case_json() {
        let rules = SmartRules {
            match_mode: MatchMode::Any,
            rules: vec![
                text(TextField::AlbumArtist, TextOp::NotContains, "x"),
                SmartRule::Number {
                    field: NumberField::PlayCount,
                    op: NumberOp::Between,
                    value: 1,
                    value_to: Some(5),
                },
                SmartRule::Date {
                    field: DateField::LastPlayedAt,
                    op: DateOp::NotInLast,
                    days: 90,
                },
                SmartRule::Favorite { value: true },
            ],
            order: SmartOrder {
                field: SmartOrderField::LastPlayedAt,
                descending: true,
            },
            limit: Some(25),
        };

        let json = serde_json::to_value(&rules).unwrap();
        assert_eq!(json["matchMode"], "any");
        assert_eq!(json["rules"][0]["kind"], "text");
        assert_eq!(json["rules"][0]["field"], "albumArtist");
        assert_eq!(json["rules"][0]["op"], "notContains");
        assert_eq!(json["rules"][1]["valueTo"], 5);
        assert_eq!(json["rules"][2]["field"], "lastPlayedAt");
        assert_eq!(json["rules"][3]["kind"], "favorite");
        assert_eq!(json["order"]["field"], "lastPlayedAt");
        assert_eq!(serde_json::from_value::<SmartRules>(json).unwrap(), rules);
    }

    /// フロントエンドが送る形（`src/lib/bindings.ts`の`SmartRules`）を読めること
    #[test]
    fn test_rules_deserialize_from_the_frontend_format() {
        let json = r#"{
            "matchMode": "all",
            "rules": [
                { "kind": "text", "field": "path", "op": "startsWith", "value": "/music/live" },
                { "kind": "number", "field": "trackNumber", "op": "atLeast", "value": 2, "valueTo": null },
                { "kind": "number", "field": "duration", "op": "between", "value": 60, "valueTo": 600 },
                { "kind": "date", "field": "createdAt", "op": "inLast", "days": 30 },
                { "kind": "favorite", "value": false }
            ],
            "order": { "field": "random", "descending": false },
            "limit": null
        }"#;

        let rules: SmartRules = serde_json::from_str(json).unwrap();

        assert_eq!(rules.match_mode, MatchMode::All);
        assert_eq!(
            rules.rules,
            [
                text(TextField::Path, TextOp::StartsWith, "/music/live"),
                number(NumberField::TrackNumber, NumberOp::AtLeast, 2),
                SmartRule::Number {
                    field: NumberField::Duration,
                    op: NumberOp::Between,
                    value: 60,
                    value_to: Some(600),
                },
                SmartRule::Date {
                    field: DateField::CreatedAt,
                    op: DateOp::InLast,
                    days: 30,
                },
                SmartRule::Favorite { value: false },
            ]
        );
        assert_eq!(rules.order.field, SmartOrderField::Random);
        assert_eq!(rules.limit, None);
        assert!(validate_rules(&rules).is_ok());
        // 知らない項目のある条件は、読めない（保存してある条件なら、曲のない自動プレイリストにする）
        assert!(serde_json::from_str::<SmartRules>(&json.replace("trackNumber", "mood")).is_err());
    }
}
