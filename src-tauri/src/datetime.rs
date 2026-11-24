//! 日時の扱い（jiff。ADR-131）
//!
//! DBに書く日時は、UTCのRFC 3339の文字列にする（`2026-11-24T05:12:34.123456+00:00`）。
//! 秒未満の桁は値に合わせて0・3・6・9桁にし、オフセットは`Z`ではなく`+00:00`と書く
//! （置き換える前のchronoの`to_rfc3339`と同じ形。DBの日時は、SQLで文字列のまま比べ・並べるため、形をそろえる）。
//! 日時の値は`jiff::Timestamp`で持ち、手元の日付・時刻（バックアップのファイル名など）は`jiff::Zoned::now()`から求める。

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::fmt::temporal::DateTimePrinter;
use jiff::tz::{Offset, TimeZone};

/// 今の日時を、DBに書く形の文字列にする
pub fn now_rfc3339() -> String {
    to_rfc3339(Timestamp::now())
}

/// 日時を、DBに書く形の文字列（UTCのRFC 3339）にする
pub fn to_rfc3339(timestamp: Timestamp) -> String {
    DateTimePrinter::new()
        .precision(Some(subsec_digits(timestamp.subsec_nanosecond())))
        .timestamp_with_offset_to_string(&timestamp, Offset::UTC)
}

/// 秒未満の桁の数（0・3・6・9）。値を失わない、いちばん短い桁にする
fn subsec_digits(nanosecond: i32) -> u8 {
    if nanosecond == 0 {
        0
    } else if nanosecond % 1_000_000 == 0 {
        3
    } else if nanosecond % 1_000 == 0 {
        6
    } else {
        9
    }
}

/// RFC 3339（オフセット付き）の日時を読む。読めなければ`None`
pub fn parse_rfc3339(value: &str) -> Option<Timestamp> {
    value.parse().ok()
}

/// DBに保存してある日時を読む
///
/// RFC 3339（オフセット付き）のほか、SQLiteの`datetime('now')`が書く`YYYY-MM-DD HH:MM:SS`（UTC）も読む
/// （表の既定値で入る形）。読めなければ`None`
pub fn parse_stored(value: &str) -> Option<Timestamp> {
    parse_rfc3339(value).or_else(|| {
        DateTime::strptime("%Y-%m-%d %H:%M:%S", value)
            .ok()?
            .to_zoned(TimeZone::UTC)
            .ok()
            .map(|zoned| zoned.timestamp())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(nanosecond: i32) -> Timestamp {
        Timestamp::new(1_763_961_154, nanosecond).unwrap()
    }

    #[test]
    fn test_to_rfc3339_writes_utc_offset_and_shortest_subsec_digits() {
        // chronoの`to_rfc3339`（`SecondsFormat::AutoSi`・`Z`を使わない）と同じ形
        assert_eq!(to_rfc3339(timestamp(0)), "2025-11-24T05:12:34+00:00");
        assert_eq!(
            to_rfc3339(timestamp(120_000_000)),
            "2025-11-24T05:12:34.120+00:00"
        );
        assert_eq!(
            to_rfc3339(timestamp(123_456_000)),
            "2025-11-24T05:12:34.123456+00:00"
        );
        assert_eq!(
            to_rfc3339(timestamp(123_456_789)),
            "2025-11-24T05:12:34.123456789+00:00"
        );
        assert_eq!(
            to_rfc3339(timestamp(1_000)),
            "2025-11-24T05:12:34.000001+00:00"
        );
    }

    #[test]
    fn test_to_rfc3339_before_unix_epoch() {
        assert_eq!(
            to_rfc3339(Timestamp::new(-1, 500_000_000).unwrap()),
            "1969-12-31T23:59:59.500+00:00"
        );
    }

    #[test]
    fn test_now_rfc3339_round_trips() {
        let now = now_rfc3339();
        assert!(now.ends_with("+00:00"), "{now}");
        assert_eq!(to_rfc3339(parse_stored(&now).unwrap()), now);
    }

    #[test]
    fn test_parse_stored_reads_rfc3339_with_any_offset() {
        let expected = timestamp(0);
        assert_eq!(parse_stored("2025-11-24T05:12:34+00:00"), Some(expected));
        assert_eq!(parse_stored("2025-11-24T05:12:34Z"), Some(expected));
        assert_eq!(parse_stored("2025-11-24T14:12:34+09:00"), Some(expected));
        assert_eq!(
            parse_stored("2025-11-24T05:12:34.123456+00:00"),
            Some(timestamp(123_456_000))
        );
    }

    #[test]
    fn test_parse_stored_reads_sqlite_datetime_as_utc() {
        assert_eq!(parse_stored("2025-11-24 05:12:34"), Some(timestamp(0)));
        // RFC 3339だけを読む方は、オフセットのない日時を読まない
        assert_eq!(parse_rfc3339("2025-11-24 05:12:34"), None);
    }

    #[test]
    fn test_parse_stored_rejects_other_text() {
        assert_eq!(parse_stored(""), None);
        assert_eq!(parse_stored("2025-11-24"), None);
        assert_eq!(parse_stored("not a date"), None);
    }
}
