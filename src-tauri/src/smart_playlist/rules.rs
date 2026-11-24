//! オートプレイリストの条件（ADR-042）
//!
//! 条件（`SmartRules`）は、項目・演算子・値の組の一覧と、その組み合わせ方（すべて / いずれか）、
//! 並び順、曲数の上限からなる。条件は`smart_playlists.rules`にJSONで保存し、曲の一覧は開くたびに
//! 条件から求め直す（曲の評価・再生回数などの変更に追随する）。
//!
//! 条件からSQLを作るが、項目・演算子は列挙型から決まった式を選び、値はすべてプレースホルダーで
//! 渡す（入力した文字列をSQLへ埋め込まない）。
//!
//! 通常のプレイリスト・ほかのオートプレイリストを条件にできる（「入っている / 入っていない」。
//! ADR-116・ADR-127）。通常のプレイリストは入れてある曲、オートプレイリストはその条件に合う曲
//! （上限・並び順も含めて求めた曲）で比べる。そのオートプレイリスト自身や、互いを条件にする条件は、
//! 保存する時に断る（`validate_playlist_references`）。消したプレイリストは、曲のないプレイリストとして
//! 扱う。
//!
//! ランダムな並びは、種（`EvalContext::shuffle_seed`）と曲のIDから決める。種が同じ間は同じ並びに
//! なるため、曲の評価を変えた・1曲聴き終えたなどで一覧を取り直しても、並びは変わらない
//! （種は起動のたびに変わり、「選び直す」操作でも変わる。`AppState::reshuffle_smart_playlists`）。

use super::find_rules;
use crate::error::{AppError, AppResult};
use crate::models::Track;
use crate::multi_value::MatchOp;
use crate::repository::{ALBUM_ARTIST, TRACK_COLUMNS, query_tracks};
use jiff::{SignedDuration, Timestamp};
use rusqlite::{Connection, ToSql};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
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

/// キーの条件の値の上限（主音の音名（0〜11）+ 12 × 調（0は長調・1は短調））
const MAX_KEY: i32 = 23;

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
    Composer,
    Grouping,
    Comment,
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
    /// 評価（半分の星の単位。0は評価なし、7は星3.5。ADR-082）
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
    /// タグのBPM
    Bpm,
    /// 曲の解析で求めたテンポ（四捨五入した整数で比べる。ADR-104）
    AnalyzedBpm,
    /// サンプルレート（Hz）
    SampleRate,
    /// ビット深度（bit）
    BitDepth,
    Channels,
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

/// キー（曲の解析の結果）の比べ方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum KeyOp {
    Is,
    /// そのキーではない（解析していない曲を含む）
    IsNot,
    /// キーがない（解析していない・解析できなかった・キーを決められなかった曲）
    IsEmpty,
    IsNotEmpty,
}

/// プレイリストに入っているかの比べ方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PlaylistOp {
    /// 入っている
    Is,
    /// 入っていない
    IsNot,
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
    /// 音楽フォルダの中にあるか（音楽フォルダが未設定なら、どの曲も中にない。ADR-084）
    InMusicFolder { value: bool },
    /// 曲の解析で求めたキー（`value`は、主音の音名 + 12 × 調（0は長調・1は短調）。0はハ長調、
    /// 21はイ短調。`IsEmpty`・`IsNotEmpty`では使わない）
    Key { op: KeyOp, value: i32 },
    /// 通常のプレイリストに入っているか（入れてある曲。ADR-116）
    Playlist {
        op: PlaylistOp,
        #[serde(rename = "playlistId")]
        playlist_id: String,
    },
    /// ほかのオートプレイリストの曲か（その条件に合う曲。ADR-116・ADR-127）
    SmartPlaylist {
        op: PlaylistOp,
        #[serde(rename = "playlistId")]
        playlist_id: String,
    },
    /// ファイルが見つからない曲か（ADR-023）
    Missing { value: bool },
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

/// オートプレイリストの条件
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
    pub now: Timestamp,
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
                    *crate::rating_tag::RATING_RANGE.end()
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
            SmartRule::Key { op, value } => {
                let needs_value = matches!(op, KeyOp::Is | KeyOp::IsNot);
                if needs_value && !(0..=MAX_KEY).contains(value) {
                    return invalid("キーの条件の値が範囲の外です");
                }
            }
            SmartRule::Playlist { playlist_id, .. } => {
                crate::validation::validate_playlist_id(playlist_id)?;
            }
            SmartRule::SmartPlaylist { playlist_id, .. } => {
                crate::validation::validate_smart_playlist_id(playlist_id)?;
            }
            SmartRule::Favorite { .. }
            | SmartRule::InMusicFolder { .. }
            | SmartRule::Missing { .. } => {}
        }
    }
    Ok(())
}

/// 条件にしたオートプレイリスト（同じものを2回条件にした場合は、2回返す）
fn referenced_smart_playlists(rules: &SmartRules) -> impl Iterator<Item = &str> {
    rules.rules.iter().filter_map(|rule| match rule {
        SmartRule::SmartPlaylist { playlist_id, .. } => Some(playlist_id.as_str()),
        _ => None,
    })
}

/// 条件にしたプレイリスト・オートプレイリストを検証する（保存する時に呼ぶ。`validate_rules`のあとで呼ぶ）
///
/// 条件にしたプレイリスト・オートプレイリストがあることと、`smart_playlist_id`（条件を変える
/// オートプレイリスト。新しく作る時はNone）自身を、直接も、ほかのオートプレイリストを通しても条件に
/// しないこと（循環すると、曲を決められない）を確かめる。
pub fn validate_playlist_references(
    conn: &Connection,
    smart_playlist_id: Option<&str>,
    rules: &SmartRules,
) -> AppResult<()> {
    let invalid = |message: &str| Err(AppError::Validation(message.to_string()));
    for rule in &rules.rules {
        if let SmartRule::Playlist { playlist_id, .. } = rule
            && !crate::playlist::playlist_exists(conn, playlist_id)?
        {
            return invalid("条件にしたプレイリストが見つかりません");
        }
    }

    let mut visited = HashSet::new();
    for target in referenced_smart_playlists(rules) {
        if Some(target) == smart_playlist_id {
            return invalid("オートプレイリストの条件に、そのオートプレイリスト自身は使えません");
        }
        let Some(target_rules) = find_rules(conn, target)? else {
            return invalid("条件にしたオートプレイリストが見つかりません");
        };
        if let Some(smart_playlist_id) = smart_playlist_id
            && refers_to(conn, &target_rules, smart_playlist_id, &mut visited)?
        {
            return invalid(
                "条件にしたオートプレイリストが、このオートプレイリストを条件にしています（互いを条件にはできません）",
            );
        }
    }
    Ok(())
}

/// 条件が、（ほかのオートプレイリストを通してでも）オートプレイリストを条件にしているか
fn refers_to(
    conn: &Connection,
    rules: &SmartRules,
    smart_playlist_id: &str,
    visited: &mut HashSet<String>,
) -> AppResult<bool> {
    for target in referenced_smart_playlists(rules) {
        if target == smart_playlist_id {
            return Ok(true);
        }
        if !visited.insert(target.to_string()) {
            continue;
        }
        if let Some(target_rules) = find_rules(conn, target)?
            && refers_to(conn, &target_rules, smart_playlist_id, visited)?
        {
            return Ok(true);
        }
    }
    Ok(false)
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
        TextField::Composer => "composer",
        TextField::Grouping => "grouping",
        TextField::Comment => "comment",
        TextField::Format => "format",
        TextField::Path => "file_path",
    }
}

fn number_column(field: NumberField) -> &'static str {
    match field {
        NumberField::Rating => "rating",
        NumberField::Year => "year",
        NumberField::PlayCount => "play_count",
        NumberField::SkipCount => "skip_count",
        NumberField::Duration => "duration",
        NumberField::TrackNumber => "track_number",
        NumberField::DiscNumber => "disc_number",
        NumberField::Bitrate => "bitrate",
        NumberField::Bpm => "bpm",
        NumberField::AnalyzedBpm => concat!("ROUND(", crate::repository::analyzed_bpm_sql!(), ")"),
        NumberField::SampleRate => "sample_rate",
        NumberField::BitDepth => "bit_depth",
        NumberField::Channels => "channels",
    }
}

fn date_column(field: DateField) -> &'static str {
    match field {
        DateField::CreatedAt => "created_at",
        DateField::LastPlayedAt => "last_played_at",
    }
}

/// 文字列の比べ方を、複数の値の比べ方にする（合わない時に成り立つ条件は、2つ目がtrue）
///
/// 「空」「空でない」は、値ごとに比べる必要がないためNone。
fn multi_value_op(op: TextOp) -> Option<(MatchOp, bool)> {
    match op {
        TextOp::Contains => Some((MatchOp::Contains, false)),
        TextOp::NotContains => Some((MatchOp::Contains, true)),
        TextOp::StartsWith => Some((MatchOp::StartsWith, false)),
        TextOp::EndsWith => Some((MatchOp::EndsWith, false)),
        TextOp::Is => Some((MatchOp::Is, false)),
        TextOp::IsNot => Some((MatchOp::Is, true)),
        TextOp::IsEmpty | TextOp::IsNotEmpty => None,
    }
}

/// 条件から曲を求める間の状態（ほかのプレイリストを条件にした時に、その曲を求める）
struct Resolver<'a> {
    conn: &'a Connection,
    context: EvalContext,
    /// 曲を求めている途中のオートプレイリスト（保存してある条件が循環していても、止まるようにする）
    visiting: Vec<String>,
    /// 求めたオートプレイリストの曲のID（JSONの配列）。同じものを何度条件にしても、1回だけ求める
    members: HashMap<String, String>,
}

impl<'a> Resolver<'a> {
    fn new(conn: &'a Connection, context: EvalContext) -> Self {
        Self {
            conn,
            context,
            visiting: Vec::new(),
            members: HashMap::new(),
        }
    }

    /// 1つの条件を、SQLの条件式にする
    fn rule_condition(&mut self, rule: &SmartRule) -> AppResult<Condition> {
        let condition = match rule {
            SmartRule::Text { field, op, value } => {
                let column = text_column(*field);
                // 検索と同じ正規化（大文字と小文字・全角と半角・ひらがなとカタカナをそろえる）をして比べる
                let normalized = format!("search_text({column})");
                let needle = crate::search_text::normalize(value.trim());
                // アーティスト・ジャンルは複数の値を持てるため、値のどれかが条件に合うかで比べる
                // （「ジャンルがRock」は、`Rock; Pop`の曲にも合う。`multi_value`）。作曲者は、
                // 1つの文字列として比べる（一覧・検索と同じ。ADR-115）
                if matches!(field, TextField::Artist | TextField::Genre)
                    && let Some((match_op, negate)) = multi_value_op(*op)
                {
                    return Ok(Condition::new(
                        format!(
                            "{}multi_value_match({column}, '{}', ?)",
                            if negate { "NOT " } else { "" },
                            match_op.as_sql()
                        ),
                        vec![Box::new(needle)],
                    ));
                }
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
                    TextOp::Is => {
                        Condition::new(format!("{normalized} = ?"), vec![Box::new(needle)])
                    }
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
                // `days`は`IsEmpty`・`IsNotEmpty`では検証しない（使わない）ため、桁があふれても止めずに端の日時にする
                let since = SignedDuration::from_hours(i64::from(*days) * 24);
                let cutoff = crate::datetime::to_rfc3339(
                    self.context
                        .now
                        .saturating_sub(since)
                        .unwrap_or(Timestamp::MIN),
                );
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
            SmartRule::Favorite { value } => {
                Condition::new("is_favorite = ?", vec![Box::new(i32::from(*value))])
            }
            // 曲の場所が、音楽フォルダのパス＋区切り文字で始まるか（印はDBに持たず、場所から求める）
            SmartRule::InMusicFolder { value } => Condition::new(
                format!("{IN_MUSIC_FOLDER_SQL} = ?"),
                vec![Box::new(i32::from(*value))],
            ),
            SmartRule::Key { op, value } => {
                let key = crate::repository::analyzed_key_sql!();
                match op {
                    KeyOp::Is => Condition::new(format!("{key} = ?"), vec![Box::new(*value)]),
                    // 解析していない曲は、「そのキーではない」に含める（数値の条件と同じ）
                    KeyOp::IsNot => {
                        Condition::new(format!("{key} IS NOT ?"), vec![Box::new(*value)])
                    }
                    KeyOp::IsEmpty => Condition::new(format!("{key} IS NULL"), vec![]),
                    KeyOp::IsNotEmpty => Condition::new(format!("{key} IS NOT NULL"), vec![]),
                }
            }
            // 通常のプレイリストは、入れてある曲（消したプレイリストは、曲がない）
            SmartRule::Playlist { op, playlist_id } => with_playlist_op(
                *op,
                Condition::new(
                    "EXISTS (SELECT 1 FROM playlist_tracks AS member
                     WHERE member.playlist_id = ? AND member.track_id = tracks.id)",
                    vec![Box::new(playlist_id.clone())],
                ),
            ),
            SmartRule::SmartPlaylist { op, playlist_id } => {
                with_playlist_op(*op, self.smart_playlist_members(playlist_id)?)
            }
            SmartRule::Missing { value } => Condition::new(
                "(missing_since IS NOT NULL) = ?",
                vec![Box::new(i32::from(*value))],
            ),
        };
        Ok(condition)
    }

    /// オートプレイリストの曲を表すSQLの条件式
    ///
    /// その条件に合う曲（上限があれば、並び順の先頭からその曲数まで）。消したオートプレイリストは、
    /// 曲のないオートプレイリストとして扱う。
    fn smart_playlist_members(&mut self, smart_playlist_id: &str) -> AppResult<Condition> {
        let Some(rules) = find_rules(self.conn, smart_playlist_id)? else {
            return Ok(Condition::new("0", vec![]));
        };

        let ids = match self.members.get(smart_playlist_id) {
            Some(ids) => ids.clone(),
            None => {
                if self.visiting.iter().any(|id| id == smart_playlist_id) {
                    log::warn!("Smart playlist rules refer to each other: {smart_playlist_id}");
                    return Ok(Condition::new("0", vec![]));
                }
                self.visiting.push(smart_playlist_id.to_string());
                let ids = self.matching_ids(&rules);
                self.visiting.pop();
                let ids = serde_json::to_string(&ids?).map_err(|e| {
                    AppError::Database(format!("プレイリストの曲の取得に失敗しました: {}", e))
                })?;
                self.members
                    .insert(smart_playlist_id.to_string(), ids.clone());
                ids
            }
        };
        Ok(Condition::new(
            "tracks.id IN (SELECT value FROM json_each(?))",
            vec![Box::new(ids)],
        ))
    }

    /// 条件の一覧を、1つのSQLの条件式にまとめる
    fn where_condition(&mut self, rules: &SmartRules) -> AppResult<Condition> {
        if rules.rules.is_empty() {
            // 「すべてを満たす」で条件がなければすべての曲、「いずれかを満たす」で条件がなければ曲なし
            let sql = match rules.match_mode {
                MatchMode::All => "1",
                MatchMode::Any => "0",
            };
            return Ok(Condition::new(sql, vec![]));
        }

        let separator = match rules.match_mode {
            MatchMode::All => " AND ",
            MatchMode::Any => " OR ",
        };
        let mut parts = Vec::with_capacity(rules.rules.len());
        let mut params = Vec::new();
        for rule in &rules.rules {
            let condition = self.rule_condition(rule)?;
            parts.push(format!("({})", condition.sql));
            params.extend(condition.params);
        }
        Ok(Condition::new(parts.join(separator), params))
    }

    /// 条件に合う曲を、並び順のとおりに（上限があれば、その曲数まで）取得するSQL
    ///
    /// ランダムな並びでは、上限を付けない（取得したあとで並べて、上限の曲数にする。`limit_randomly`）。
    fn matching_query(&mut self, rules: &SmartRules, columns: &str) -> AppResult<Condition> {
        let condition = self.where_condition(rules)?;
        let mut sql = format!(
            "SELECT {columns} FROM tracks WHERE {} ORDER BY {}",
            condition.sql,
            order_sql(rules.order)
        );
        let mut params = condition.params;
        if let Some(limit) = rules.limit.filter(|_| !is_random(rules)) {
            sql.push_str(" LIMIT ?");
            params.push(Box::new(limit));
        }
        Ok(Condition::new(sql, params))
    }

    /// 条件に合う曲のID（`matching_query`と同じ曲）
    fn matching_ids(&mut self, rules: &SmartRules) -> AppResult<Vec<String>> {
        let query = self.matching_query(rules, "id")?;
        let params: Vec<&dyn ToSql> = query.params.iter().map(|param| param.as_ref()).collect();
        let database = |e: rusqlite::Error| {
            AppError::Database(format!("プレイリストの曲の取得に失敗しました: {}", e))
        };
        let mut stmt = self.conn.prepare(&query.sql).map_err(database)?;
        let mut ids = stmt
            .query_map(params.as_slice(), |row| row.get::<_, String>(0))
            .map_err(database)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(database)?;
        limit_randomly(&mut ids, rules, self.context.shuffle_seed, |id| id);
        Ok(ids)
    }
}

/// 「入っている」はそのまま、「入っていない」は否定した条件式にする
fn with_playlist_op(op: PlaylistOp, members: Condition) -> Condition {
    match op {
        PlaylistOp::Is => members,
        PlaylistOp::IsNot => Condition::new(format!("NOT ({})", members.sql), members.params),
    }
}

/// 曲（`tracks`の行）が音楽フォルダの中にあるか（1か0）を求めるSQLの式（ADR-084）
const IN_MUSIC_FOLDER_SQL: &str = "EXISTS (SELECT 1 FROM library_folders AS music
     WHERE music.is_music_folder = 1
       AND substr(tracks.file_path, 1, length(music.path) + 1) = music.path || '/')";

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
        SmartOrderField::PlayCount => "play_count",
        SmartOrderField::Rating => "rating",
        SmartOrderField::Year => "year",
        SmartOrderField::Duration => "duration",
    };
    let direction = if order.descending { "DESC" } else { "ASC" };
    format!(
        "{expression} {direction}, {ALBUM_ARTIST}, album,
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

fn is_random(rules: &SmartRules) -> bool {
    rules.order.field == SmartOrderField::Random
}

/// ランダムな並びの条件なら、取得した曲を種の並びにして、上限の曲数にする
fn limit_randomly<T>(
    items: &mut Vec<T>,
    rules: &SmartRules,
    seed: u64,
    id_of: impl Fn(&T) -> &str,
) {
    if !is_random(rules) {
        return;
    }
    items.sort_by_cached_key(|item| shuffle_key(seed, id_of(item)));
    if let Some(limit) = rules.limit {
        items.truncate(limit as usize);
    }
}

/// 条件に合う曲を、並び順のとおりに（上限があれば、その曲数まで）取得する
pub fn find_tracks(
    conn: &Connection,
    rules: &SmartRules,
    context: EvalContext,
) -> AppResult<Vec<Track>> {
    let query = Resolver::new(conn, context).matching_query(rules, TRACK_COLUMNS)?;
    let params: Vec<&dyn ToSql> = query.params.iter().map(|param| param.as_ref()).collect();
    let mut tracks = query_tracks(conn, &query.sql, &params)?;
    limit_randomly(&mut tracks, rules, context.shuffle_seed, |track| &track.id);
    Ok(tracks)
}

/// 条件に合う曲数（上限があれば、それを超えない）を数える（条件の編集画面で、入力のたびに出す）
pub fn count_tracks(conn: &Connection, rules: &SmartRules, context: EvalContext) -> AppResult<u32> {
    let condition = Resolver::new(conn, context).where_condition(rules)?;
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
        crate::db::open_in_memory()
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

    fn now() -> Timestamp {
        "2026-10-10T00:00:00+00:00".parse().unwrap()
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

    /// アーティスト・ジャンルが複数ある曲は、値のどれかが条件に合えば合う
    #[test]
    fn test_text_rules_match_each_value_of_artists_and_genres() {
        let conn = setup();
        insert(&conn, "a", "宇多田ヒカル; ZARD", "X", Some("Rock; J-Pop"));
        insert(&conn, "b", "ZARD", "Y", Some("J-Pop"));
        insert(&conn, "c", "Miles Davis", "Z", Some("Jazz"));
        insert(&conn, "d", "Popper", "W", None);
        let find = |field, op, value: &str| {
            ids(&conn, &rules(MatchMode::All, vec![text(field, op, value)]))
        };

        assert_eq!(find(TextField::Genre, TextOp::Is, "rock"), ["a"]);
        assert_eq!(find(TextField::Genre, TextOp::Is, "j-pop"), ["a", "b"]);
        // 値の一部だけでは「である」に合わない。分ける前の文字列そのものには合う
        assert!(find(TextField::Genre, TextOp::Is, "pop").is_empty());
        assert_eq!(find(TextField::Genre, TextOp::Is, "Rock; J-Pop"), ["a"]);
        // 「ではない」は、どの値も合わない曲（値のない曲を含む）
        assert_eq!(find(TextField::Genre, TextOp::IsNot, "j-pop"), ["c", "d"]);
        assert_eq!(
            find(TextField::Genre, TextOp::IsNot, "rock"),
            ["b", "c", "d"]
        );
        // 「で始まる」「で終わる」も、値ごとに比べる
        assert_eq!(
            find(TextField::Artist, TextOp::StartsWith, "zard"),
            ["a", "b"]
        );
        assert_eq!(find(TextField::Artist, TextOp::EndsWith, "ひかる"), ["a"]);
        assert_eq!(find(TextField::Artist, TextOp::Contains, "ar"), ["a", "b"]);
        assert_eq!(
            find(TextField::Artist, TextOp::NotContains, "zard"),
            ["c", "d"]
        );
        // アルバムなど、ほかの項目は分けない
        insert(&conn, "e", "Solo", "Live; Remastered", None);
        assert!(find(TextField::Album, TextOp::Is, "Live").is_empty());
        assert_eq!(
            find(TextField::Album, TextOp::Is, "Live; Remastered"),
            ["e"]
        );
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
        // 評価は半分の星の単位（bは星3.5）
        set(&conn, "a", "rating", &10);
        set(&conn, "b", "rating", &7);
        set(&conn, "c", "rating", &4);
        set(&conn, "a", "year", &1999);
        set(&conn, "c", "year", &1959);
        set(&conn, "a", "play_count", &30);
        let find = |rule| ids(&conn, &rules(MatchMode::All, vec![rule]));

        assert_eq!(
            find(number(NumberField::Rating, NumberOp::AtLeast, 7)),
            ["a", "b"]
        );
        // 星3.5と星4を分ける
        assert_eq!(
            find(number(NumberField::Rating, NumberOp::AtLeast, 8)),
            ["a"]
        );
        // 評価のない曲は、0として扱う
        assert_eq!(
            find(number(NumberField::Rating, NumberOp::AtMost, 4)),
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
        // 日数を使わない比べ方では日数を検証しないため、どんな値でも止まらない
        assert_eq!(
            find(DateField::LastPlayedAt, DateOp::IsEmpty, u32::MAX),
            ["b", "d"]
        );
    }

    /// 音楽フォルダの中にあるか（音楽フォルダが未設定なら、どの曲も中にない）
    #[test]
    fn test_in_music_folder_rule() {
        let conn = library();
        let inside = |value| {
            ids(
                &conn,
                &rules(MatchMode::All, vec![SmartRule::InMusicFolder { value }]),
            )
        };
        assert!(inside(true).is_empty());
        assert_eq!(inside(false), ["a", "b", "c", "d"]);

        // 音楽フォルダ（`/music/ZARD`）と、名前の先頭が同じだけのフォルダ（`/music/ZARD2`）
        let folder_id = crate::library_folder::register_folder(&conn, "/music/ZARD", &[])
            .unwrap()
            .unwrap();
        crate::library_folder::set_music_folder(&conn, &folder_id).unwrap();
        set(&conn, "c", "file_path", &"/music/ZARD2/c.mp3");
        assert_eq!(inside(true), ["b"]);
        assert_eq!(inside(false), ["a", "c", "d"]);
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

        assert_eq!(count_tracks(&conn, &all, context(1)).unwrap(), 4);
        all.limit = Some(3);
        assert_eq!(count_tracks(&conn, &all, context(1)).unwrap(), 3);
        all.limit = Some(10);
        assert_eq!(count_tracks(&conn, &all, context(1)).unwrap(), 4);
        let jazz = rules(
            MatchMode::All,
            vec![text(TextField::Genre, TextOp::Is, "jazz")],
        );
        assert_eq!(count_tracks(&conn, &jazz, context(1)).unwrap(), 1);
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

        assert!(valid(number(NumberField::Rating, NumberOp::AtLeast, 10)));
        assert!(!valid(number(NumberField::Rating, NumberOp::AtLeast, 11)));
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
                SmartRule::SmartPlaylist {
                    op: PlaylistOp::IsNot,
                    playlist_id: "p1".to_string(),
                },
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
        assert_eq!(json["rules"][4]["kind"], "smartPlaylist");
        assert_eq!(json["rules"][4]["op"], "isNot");
        assert_eq!(json["rules"][4]["playlistId"], "p1");
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
        // 知らない項目のある条件は、読めない（保存してある条件なら、曲のないオートプレイリストにする）
        assert!(serde_json::from_str::<SmartRules>(&json.replace("trackNumber", "mood")).is_err());
    }

    /// 作曲者・グループ・コメントの条件
    #[test]
    fn test_composer_grouping_and_comment_rules() {
        let conn = library();
        set(&conn, "a", "composer", &"Bach");
        set(&conn, "b", "composer", &"Bachmann");
        set(&conn, "c", "grouping", &"Live");
        set(&conn, "d", "comment", &"Remastered 2020");

        let all = |rule| ids(&conn, &rules(MatchMode::All, vec![rule]));
        assert_eq!(all(text(TextField::Composer, TextOp::Is, "bach")), ["a"]);
        assert_eq!(
            all(text(TextField::Composer, TextOp::Contains, "bach")),
            ["a", "b"]
        );
        assert_eq!(
            all(text(TextField::Composer, TextOp::IsEmpty, "")),
            ["c", "d"]
        );
        assert_eq!(all(text(TextField::Grouping, TextOp::Is, "live")), ["c"]);
        assert_eq!(
            all(text(TextField::Comment, TextOp::Contains, "remaster")),
            ["d"]
        );
    }

    /// BPM・解析したテンポ・サンプルレート・ビット深度・チャンネル数の条件
    #[test]
    fn test_bpm_and_audio_property_rules() {
        let conn = library();
        set(&conn, "a", "bpm", &128);
        set(&conn, "a", "sample_rate", &96_000);
        set(&conn, "a", "bit_depth", &24);
        set(&conn, "a", "channels", &2);
        set(&conn, "b", "sample_rate", &44_100);
        set(&conn, "b", "bit_depth", &16);
        set(&conn, "b", "channels", &1);
        for id in ["a", "b", "c", "d"] {
            set(&conn, id, "duration", &200);
        }
        let analyze = |id: &str, bpm: f64, failed: bool, duration: i32| {
            conn.execute(
                "INSERT INTO track_analysis (track_id, duration, bpm, failed, analyzed_at)
                 VALUES (?1, ?2, ?3, ?4, '2026-01-01T00:00:00+00:00')",
                rusqlite::params![id, duration, bpm, failed],
            )
            .unwrap();
        };
        analyze("a", 124.5, false, 200);
        analyze("b", 90.2, false, 200);
        // 失敗した解析と、長さが変わる前の解析は使わない
        analyze("c", 125.0, true, 200);
        analyze("d", 125.0, false, 180);

        let all = |rule| ids(&conn, &rules(MatchMode::All, vec![rule]));
        assert_eq!(all(number(NumberField::Bpm, NumberOp::AtLeast, 120)), ["a"]);
        // 解析したテンポは、四捨五入した整数で比べる
        assert_eq!(
            all(number(NumberField::AnalyzedBpm, NumberOp::Is, 125)),
            ["a"]
        );
        assert_eq!(
            all(number(NumberField::AnalyzedBpm, NumberOp::AtMost, 90)),
            ["b"]
        );
        assert_eq!(
            all(number(NumberField::AnalyzedBpm, NumberOp::IsNot, 125)),
            ["b", "c", "d"]
        );
        assert_eq!(
            all(number(NumberField::SampleRate, NumberOp::AtLeast, 48_000)),
            ["a"]
        );
        assert_eq!(all(number(NumberField::BitDepth, NumberOp::Is, 16)), ["b"]);
        assert_eq!(all(number(NumberField::Channels, NumberOp::Is, 1)), ["b"]);
    }

    /// キー（曲の解析の結果）の条件
    #[test]
    fn test_key_rules() {
        let conn = library();
        let analyze = |id: &str, pitch_class: Option<i32>, mode: Option<i32>| {
            conn.execute(
                "INSERT INTO track_analysis (track_id, duration, key_pitch_class, key_mode,
                                             analyzed_at)
                 VALUES (?1, NULL, ?2, ?3, '2026-01-01T00:00:00+00:00')",
                rusqlite::params![id, pitch_class, mode],
            )
            .unwrap();
        };
        // イ短調（9 + 12）・ハ長調（0）・キーを決められなかった曲（dは解析していない）
        analyze("a", Some(9), Some(1));
        analyze("b", Some(0), Some(0));
        analyze("c", None, None);

        let key = |op, value| {
            ids(
                &conn,
                &rules(MatchMode::All, vec![SmartRule::Key { op, value }]),
            )
        };
        assert_eq!(key(KeyOp::Is, 21), ["a"]);
        assert_eq!(key(KeyOp::Is, 0), ["b"]);
        assert_eq!(key(KeyOp::IsNot, 21), ["b", "c", "d"]);
        assert_eq!(key(KeyOp::IsEmpty, 0), ["c", "d"]);
        assert_eq!(key(KeyOp::IsNotEmpty, 0), ["a", "b"]);
    }

    /// ファイルが見つからない曲の条件
    #[test]
    fn test_missing_rule() {
        let conn = library();
        set(&conn, "b", "missing_since", &"2026-01-02T00:00:00+00:00");

        let missing = |value| {
            ids(
                &conn,
                &rules(MatchMode::All, vec![SmartRule::Missing { value }]),
            )
        };
        assert_eq!(missing(true), ["b"]);
        assert_eq!(missing(false), ["a", "c", "d"]);
    }

    fn in_playlist(op: PlaylistOp, playlist_id: &str) -> SmartRule {
        SmartRule::Playlist {
            op,
            playlist_id: playlist_id.to_string(),
        }
    }

    fn in_smart_playlist(op: PlaylistOp, smart_playlist_id: &str) -> SmartRule {
        SmartRule::SmartPlaylist {
            op,
            playlist_id: smart_playlist_id.to_string(),
        }
    }

    /// ほかのプレイリストを条件にする（通常のプレイリストは入れてある曲、オートプレイリストは条件に合う曲）
    #[test]
    fn test_playlist_rules() {
        let conn = library();
        let manual = crate::playlist::create_playlist(&conn, "車では聴かない").unwrap();
        crate::playlist::add_tracks_to_playlist(&conn, &manual.id, &["a".into(), "c".into()])
            .unwrap();
        // J-Popをタイトル順に1曲まで（a）
        let mut jpop = rules(
            MatchMode::All,
            vec![text(TextField::Genre, TextOp::Is, "J-Pop")],
        );
        jpop.limit = Some(1);
        let smart = super::super::create_smart_playlist(&conn, "J-Pop", &jpop).unwrap();

        let all = |rules_: Vec<SmartRule>| ids(&conn, &rules(MatchMode::All, rules_));
        assert_eq!(
            all(vec![in_playlist(PlaylistOp::Is, &manual.id)]),
            ["a", "c"]
        );
        assert_eq!(
            all(vec![in_playlist(PlaylistOp::IsNot, &manual.id)]),
            ["b", "d"]
        );
        // オートプレイリストは、上限まで求めた曲で比べる
        assert_eq!(
            all(vec![in_smart_playlist(PlaylistOp::Is, &smart.id)]),
            ["a"]
        );
        assert_eq!(
            ids(
                &conn,
                &rules(
                    MatchMode::Any,
                    vec![
                        in_playlist(PlaylistOp::Is, &manual.id),
                        in_smart_playlist(PlaylistOp::Is, &smart.id)
                    ]
                )
            ),
            ["a", "c"]
        );
        let count = |rules_: Vec<SmartRule>| {
            count_tracks(&conn, &rules(MatchMode::All, rules_), context(1)).unwrap()
        };
        assert_eq!(
            count(vec![in_smart_playlist(PlaylistOp::IsNot, &smart.id)]),
            3
        );
        // 種類を取り違えたIDは、曲のないプレイリストとして扱う（通常のプレイリストとオートプレイリストは
        // 別の表）
        assert!(all(vec![in_playlist(PlaylistOp::Is, &smart.id)]).is_empty());
        assert!(all(vec![in_smart_playlist(PlaylistOp::Is, &manual.id)]).is_empty());

        // 消したプレイリストは、曲のないプレイリストとして扱う
        crate::playlist::delete_playlist(&conn, &manual.id).unwrap();
        assert!(all(vec![in_playlist(PlaylistOp::Is, &manual.id)]).is_empty());
        assert_eq!(
            all(vec![in_playlist(PlaylistOp::IsNot, &manual.id)]),
            ["a", "b", "c", "d"]
        );
        super::super::delete_smart_playlist(&conn, &smart.id).unwrap();
        assert!(all(vec![in_smart_playlist(PlaylistOp::Is, &smart.id)]).is_empty());
    }

    /// 保存してある条件が互いを条件にしていても（ほかの版で保存したなど）、求めるのが止まる
    #[test]
    fn test_playlist_rules_stop_at_cycles() {
        let conn = library();
        let first = super::super::create_smart_playlist(&conn, "A", &rules(MatchMode::All, vec![]))
            .unwrap();
        let second = super::super::create_smart_playlist(
            &conn,
            "B",
            &rules(
                MatchMode::All,
                vec![in_smart_playlist(PlaylistOp::Is, &first.id)],
            ),
        )
        .unwrap();
        // 検証を通さずに、AがBを条件にするように書き換える
        let cyclic = rules(
            MatchMode::All,
            vec![in_smart_playlist(PlaylistOp::Is, &second.id)],
        );
        conn.execute(
            "UPDATE smart_playlists SET rules = ?1 WHERE id = ?2",
            rusqlite::params![serde_json::to_string(&cyclic).unwrap(), first.id],
        )
        .unwrap();

        assert!(ids(&conn, &cyclic).is_empty());
    }

    #[test]
    fn test_validate_playlist_references() {
        let conn = library();
        let manual = crate::playlist::create_playlist(&conn, "通常").unwrap();
        let first = super::super::create_smart_playlist(&conn, "A", &rules(MatchMode::All, vec![]))
            .unwrap();
        let refers = |id: &str| rules(MatchMode::All, vec![in_smart_playlist(PlaylistOp::Is, id)]);
        let refers_manual = |id: &str| rules(MatchMode::All, vec![in_playlist(PlaylistOp::Is, id)]);
        let second = super::super::create_smart_playlist(&conn, "B", &refers(&first.id)).unwrap();

        let validate = |smart_playlist_id: Option<&str>, rules_: &SmartRules| {
            validate_playlist_references(&conn, smart_playlist_id, rules_)
        };
        assert!(validate(None, &refers_manual(&manual.id)).is_ok());
        assert!(validate(Some(&first.id), &refers_manual(&manual.id)).is_ok());
        // そのオートプレイリスト自身
        assert!(matches!(
            validate(Some(&first.id), &refers(&first.id)),
            Err(AppError::Validation(_))
        ));
        // ないプレイリスト・オートプレイリスト（種類を取り違えたIDを含む）
        let unknown = uuid::Uuid::new_v4().to_string();
        for rules_ in [
            refers(&unknown),
            refers_manual(&unknown),
            refers(&manual.id),
            refers_manual(&first.id),
        ] {
            assert!(matches!(
                validate(None, &rules_),
                Err(AppError::Validation(_))
            ));
        }
        // BはAを条件にしているため、AはBを条件にできない（保存もしない）
        assert!(matches!(
            validate(Some(&first.id), &refers(&second.id)),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            super::super::update_smart_playlist(&conn, &first.id, "A", &refers(&second.id)),
            Err(AppError::Validation(_))
        ));
        // ほかのオートプレイリストを条件にするのはよい
        let third = super::super::create_smart_playlist(&conn, "C", &refers(&second.id)).unwrap();
        assert!(validate(Some(&third.id), &refers(&first.id)).is_ok());
    }

    #[test]
    fn test_validate_key_and_playlist_rules() {
        let key = |op, value| rules(MatchMode::All, vec![SmartRule::Key { op, value }]);
        assert!(validate_rules(&key(KeyOp::Is, 23)).is_ok());
        assert!(validate_rules(&key(KeyOp::Is, 24)).is_err());
        assert!(validate_rules(&key(KeyOp::IsNot, -1)).is_err());
        // 値を使わない比べ方では、値を見ない
        assert!(validate_rules(&key(KeyOp::IsEmpty, 99)).is_ok());

        for rule in [in_playlist, in_smart_playlist] {
            let playlist = |id: &str| rules(MatchMode::All, vec![rule(PlaylistOp::Is, id)]);
            assert!(validate_rules(&playlist(&uuid::Uuid::new_v4().to_string())).is_ok());
            assert!(validate_rules(&playlist("../other")).is_err());
        }
    }
}
