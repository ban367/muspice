//! AIシャッフルの選曲（ADR-104）
//!
//! AIシャッフルは、曲が始まるたびに、設定の選び方に合う曲をライブラリから1曲選び、今の曲の直後に足す
//! （キューへの追加はフロントエンドの`aiShuffle.svelte.ts`）。ここでは、1曲を選ぶ。
//!
//! - 選び方は3つ: 似た曲（曲の解析のテンポ・キーと、タグのジャンル・アーティスト・年）/ よく再生する曲 /
//!   あまり再生していない曲（iOSアプリのmusgroundのAIシャッフルと同じ）
//! - 同じ曲ばかりにならないよう、選び方に合う上位の曲（`CANDIDATE_LIMIT`）からランダムに選ぶ。
//!   選び方に合う曲がない（解析も再生履歴もない）時は、ライブラリからランダムに選ぶ
//! - 見つからない曲と、避ける曲（キューの前後の曲）は選ばない

use crate::error::{AppError, AppResult};
use crate::music_analysis::{MusicFeatures, MusicKey, load_features};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher, RandomState};

/// 選び方に合う上位の曲を、この件数まで候補にする
pub const CANDIDATE_LIMIT: usize = 20;

/// フロントエンドが避ける曲として渡す、キューの今の位置の前後の件数
pub const EXCLUSION_WINDOW: u32 = 100;

fn db_error(e: rusqlite::Error) -> AppError {
    AppError::Database(format!("AIシャッフルの曲の読み込みに失敗しました: {}", e))
}

/// AIシャッフルの選び方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiShuffleMode {
    /// 今の曲に、テンポ・キー（曲の解析）とジャンル・アーティスト・年が近い曲
    #[default]
    Similar,
    /// 再生回数が多い曲
    FrequentlyPlayed,
    /// 再生回数が少なく、しばらく再生していない曲
    RarelyPlayed,
}

/// 選ぶ候補の曲（選曲に使う項目だけ）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Candidate {
    pub id: String,
    /// アーティスト・ジャンルの値（小文字にしたもの。複数の値は分けてある）
    pub artists: Vec<String>,
    pub genres: Vec<String>,
    pub year: Option<i32>,
    pub play_count: i32,
    pub last_played_at: Option<String>,
    pub features: MusicFeatures,
}

/// 候補の曲（見つからない曲を除くライブラリの全曲）を読む
pub fn load_candidates(conn: &Connection) -> AppResult<Vec<Candidate>> {
    let mut features = load_features(conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, artist, genre, year, COALESCE(play_count, 0), last_played_at FROM tracks
         WHERE missing_since IS NULL",
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Candidate {
                id: row.get(0)?,
                artists: normalized_values(row.get::<_, Option<String>>(1)?.as_deref()),
                genres: normalized_values(row.get::<_, Option<String>>(2)?.as_deref()),
                year: row.get(3)?,
                play_count: row.get(4)?,
                last_played_at: row.get(5)?,
                features: MusicFeatures::default(),
            })
        })
        .map_err(db_error)?;
    let mut candidates = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_error)?;
    for candidate in &mut candidates {
        if let Some(found) = features.remove(&candidate.id) {
            candidate.features = found;
        }
    }
    Ok(candidates)
}

/// 項目の値を、比べるための形にする（複数の値に分け、小文字にする）
fn normalized_values(text: Option<&str>) -> Vec<String> {
    text.map(crate::multi_value::split)
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.to_lowercase())
        .collect()
}

/// 今の曲の次に足す曲を選ぶ
///
/// 避ける曲を除くと選べない（ライブラリが小さい）時は、今の曲だけを避けて選び直す。
/// @returns 選んだ曲のID（ほかに曲がなければNone）
pub fn pick(
    mode: AiShuffleMode,
    current_id: &str,
    candidates: &[Candidate],
    excluded: &HashSet<String>,
    random: &mut Random,
) -> Option<String> {
    let current = candidates
        .iter()
        .find(|candidate| candidate.id == current_id);
    pick_from(
        mode,
        current,
        candidates,
        |id| excluded.contains(id),
        random,
    )
    .or_else(|| pick_from(mode, current, candidates, |_| false, random))
}

fn pick_from(
    mode: AiShuffleMode,
    current: Option<&Candidate>,
    candidates: &[Candidate],
    is_excluded: impl Fn(&str) -> bool,
    random: &mut Random,
) -> Option<String> {
    let current_id = current.map(|candidate| candidate.id.as_str());
    let mut pool: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| {
            Some(candidate.id.as_str()) != current_id && !is_excluded(&candidate.id)
        })
        .collect();
    if pool.is_empty() {
        return None;
    }
    // 並びの同点を、ライブラリの順で決めないよう、先に混ぜておく（並べ替えは安定）
    random.shuffle(&mut pool);

    let ranked: Vec<&Candidate> = match mode {
        AiShuffleMode::Similar => match current {
            Some(current) => {
                let mut scored: Vec<(&Candidate, f64)> = pool
                    .iter()
                    .map(|candidate| (*candidate, similarity(current, candidate)))
                    .filter(|(_, score)| *score > 0.0)
                    .collect();
                scored.sort_by(|a, b| b.1.total_cmp(&a.1));
                scored.into_iter().map(|(candidate, _)| candidate).collect()
            }
            None => Vec::new(),
        },
        AiShuffleMode::FrequentlyPlayed => {
            let mut played: Vec<&Candidate> = pool
                .iter()
                .copied()
                .filter(|candidate| candidate.play_count > 0)
                .collect();
            played.sort_by_key(|candidate| std::cmp::Reverse(candidate.play_count));
            played
        }
        AiShuffleMode::RarelyPlayed => {
            let mut ranked = pool.clone();
            // 再生回数の少ない順。同じなら、最後に再生したのが古い順（一度も再生していない曲が先）
            ranked.sort_by(|a, b| {
                a.play_count
                    .cmp(&b.play_count)
                    .then_with(|| a.last_played_at.cmp(&b.last_played_at))
            });
            ranked
        }
    };

    // 選び方に合う曲がない（解析も再生履歴もない）時は、ライブラリからランダムに選ぶ
    let choices: &[&Candidate] = if ranked.is_empty() {
        &pool
    } else {
        &ranked[..ranked.len().min(CANDIDATE_LIMIT)]
    };
    Some(choices[random.below(choices.len())].id.clone())
}

/// 2曲の近さ。テンポとキーは曲の解析の結果、ジャンル・アーティスト・年はタグで比べる。
/// どちらかに値がない項目は数えない（解析していない曲も、タグで選べる）
pub fn similarity(a: &Candidate, b: &Candidate) -> f64 {
    let mut score = 0.0;
    if let (Some(a_bpm), Some(b_bpm)) = (a.features.bpm, b.features.bpm) {
        score += 2.0 * tempo_similarity(a_bpm, b_bpm);
    }
    if let (Some(a_key), Some(b_key)) = (a.features.key, b.features.key) {
        score += 1.5 * key_similarity(a_key, b_key);
    }
    if shares_value(&a.genres, &b.genres) {
        score += 1.5;
    }
    // アルバムアーティストではなく曲のアーティストで比べる（コンピレーションの「Various Artists」を
    // 同じとみなさない）
    if shares_value(&a.artists, &b.artists) {
        score += 1.0;
    }
    if let (Some(a_year), Some(b_year)) = (a.year, b.year)
        && a_year.abs_diff(b_year) <= 5
    {
        score += 0.5;
    }
    score
}

fn shares_value(a: &[String], b: &[String]) -> bool {
    a.iter().any(|value| b.contains(value))
}

/// テンポの近さ（0〜1）。倍・半分のテンポも同じノリとして扱い、10%以上離れたら0
pub fn tempo_similarity(a: f64, b: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 {
        return 0.0;
    }
    let difference = [b, b * 2.0, b / 2.0]
        .into_iter()
        .map(|value| (a - value).abs() / a)
        .fold(f64::INFINITY, f64::min);
    (1.0 - difference / 0.1).max(0.0)
}

/// キーの近さ（0〜1）。Camelotで同じ番号（同じキー・平行調）が1、隣が0.7、2つ隣が0.3
pub fn key_similarity(a: MusicKey, b: MusicKey) -> f64 {
    match a.camelot_distance(b) {
        0 => 1.0,
        1 => 0.7,
        2 => 0.3,
        _ => 0.0,
    }
}

/// 選曲の乱数（SplitMix64。暗号には使わない）
pub struct Random(u64);

impl Random {
    /// 実行のたびに違う種で作る
    pub fn new() -> Self {
        Self(RandomState::new().build_hasher().finish())
    }

    /// 決まった種で作る（テスト用）
    #[cfg(test)]
    pub fn with_seed(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 0以上`bound`未満の値（`bound`は1以上）
    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    /// 並びを混ぜる（Fisher-Yates）
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for index in (1..items.len()).rev() {
            let other = self.below(index + 1);
            items.swap(index, other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str) -> Candidate {
        Candidate {
            id: id.to_string(),
            ..Candidate::default()
        }
    }

    fn with_tags(id: &str, artist: &str, genre: &str, year: i32) -> Candidate {
        Candidate {
            artists: vec![artist.to_lowercase()],
            genres: vec![genre.to_lowercase()],
            year: Some(year),
            ..candidate(id)
        }
    }

    fn with_features(id: &str, bpm: f64, pitch_class: u8, minor: bool) -> Candidate {
        Candidate {
            features: MusicFeatures {
                bpm: Some(bpm),
                key: Some(MusicKey { pitch_class, minor }),
                pace: None,
            },
            ..candidate(id)
        }
    }

    fn excluded(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn test_tempo_similarity() {
        assert_eq!(tempo_similarity(120.0, 120.0), 1.0);
        // 倍・半分のテンポも同じノリ
        assert_eq!(tempo_similarity(120.0, 60.0), 1.0);
        assert_eq!(tempo_similarity(120.0, 240.0), 1.0);
        // 5%違うと0.5、10%以上違うと0
        assert!((tempo_similarity(100.0, 105.0) - 0.5).abs() < 1e-9);
        assert_eq!(tempo_similarity(100.0, 115.0), 0.0);
        assert_eq!(tempo_similarity(0.0, 120.0), 0.0);
    }

    #[test]
    fn test_key_similarity() {
        let c_major = MusicKey {
            pitch_class: 0,
            minor: false,
        };
        let a_minor = MusicKey {
            pitch_class: 9,
            minor: true,
        };
        let g_major = MusicKey {
            pitch_class: 7,
            minor: false,
        };
        let d_major = MusicKey {
            pitch_class: 2,
            minor: false,
        };
        let f_sharp_major = MusicKey {
            pitch_class: 6,
            minor: false,
        };
        assert_eq!(key_similarity(c_major, a_minor), 1.0);
        assert_eq!(key_similarity(c_major, g_major), 0.7);
        assert_eq!(key_similarity(c_major, d_major), 0.3);
        assert_eq!(key_similarity(c_major, f_sharp_major), 0.0);
    }

    #[test]
    fn test_similarity_uses_tags_and_features() {
        let current = Candidate {
            features: with_features("x", 120.0, 0, false).features,
            ..with_tags("current", "Artist", "Rock", 2000)
        };
        // 全部が一致: 2 + 1.5 + 1.5 + 1 + 0.5
        let same = Candidate {
            features: with_features("x", 120.0, 9, true).features,
            ..with_tags("same", "artist", "rock", 2004)
        };
        assert!((similarity(&current, &same) - 6.5).abs() < 1e-9);
        // 解析のない曲は、タグだけで比べる
        let tags_only = with_tags("tags", "Other", "Rock", 1980);
        assert!((similarity(&current, &tags_only) - 1.5).abs() < 1e-9);
        // 何も共通点がない
        let unrelated = with_tags("none", "Other", "Jazz", 1980);
        assert_eq!(similarity(&current, &unrelated), 0.0);
    }

    #[test]
    fn test_similarity_matches_any_of_multiple_values() {
        let current = Candidate {
            artists: vec!["a".to_string(), "b".to_string()],
            ..candidate("current")
        };
        let other = Candidate {
            artists: vec!["b".to_string()],
            ..candidate("other")
        };
        assert_eq!(similarity(&current, &other), 1.0);
    }

    #[test]
    fn test_pick_similar_prefers_close_tracks() {
        let mut candidates = vec![with_tags("current", "A", "Rock", 2000)];
        candidates.push(with_tags("close", "A", "Rock", 2001));
        // 共通点のない曲を、候補の上限より多く入れる
        for index in 0..50 {
            candidates.push(with_tags(&format!("far{index}"), "Z", "Jazz", 1950));
        }
        for seed in 0..20 {
            let picked = pick(
                AiShuffleMode::Similar,
                "current",
                &candidates,
                &HashSet::new(),
                &mut Random::with_seed(seed),
            );
            assert_eq!(picked.as_deref(), Some("close"));
        }
    }

    #[test]
    fn test_pick_does_not_choose_current_or_excluded() {
        let candidates = vec![
            with_tags("current", "A", "Rock", 2000),
            with_tags("queued", "A", "Rock", 2000),
            with_tags("other", "B", "Pop", 1990),
        ];
        for seed in 0..20 {
            let picked = pick(
                AiShuffleMode::Similar,
                "current",
                &candidates,
                &excluded(&["queued"]),
                &mut Random::with_seed(seed),
            );
            // 似た曲（queued）は避けるため、共通点のない曲からランダムに選ぶ
            assert_eq!(picked.as_deref(), Some("other"));
        }
    }

    #[test]
    fn test_pick_falls_back_when_everything_is_excluded() {
        let candidates = vec![candidate("current"), candidate("queued")];
        let picked = pick(
            AiShuffleMode::Similar,
            "current",
            &candidates,
            &excluded(&["queued"]),
            &mut Random::with_seed(1),
        );
        assert_eq!(picked.as_deref(), Some("queued"));
        // 今の曲しかなければ、選べない
        let only = vec![candidate("current")];
        assert_eq!(
            pick(
                AiShuffleMode::Similar,
                "current",
                &only,
                &HashSet::new(),
                &mut Random::with_seed(1)
            ),
            None
        );
    }

    #[test]
    fn test_pick_frequently_played() {
        let mut candidates = vec![candidate("current")];
        for index in 0..30 {
            candidates.push(Candidate {
                play_count: index,
                ..candidate(&format!("t{index}"))
            });
        }
        for seed in 0..50 {
            let picked = pick(
                AiShuffleMode::FrequentlyPlayed,
                "current",
                &candidates,
                &HashSet::new(),
                &mut Random::with_seed(seed),
            )
            .unwrap();
            // 再生回数の多い上位20曲（10〜29回）から選ぶ
            let count: i32 = picked[1..].parse().unwrap();
            assert!(count >= 10, "picked {picked}");
        }
    }

    #[test]
    fn test_pick_rarely_played() {
        let mut candidates = vec![candidate("current")];
        for index in 0..30 {
            candidates.push(Candidate {
                play_count: 1,
                last_played_at: Some(format!("2026-01-{:02} 00:00:00", index + 1)),
                ..candidate(&format!("t{index}"))
            });
        }
        candidates.push(Candidate {
            play_count: 5,
            ..candidate("often")
        });
        for seed in 0..50 {
            let picked = pick(
                AiShuffleMode::RarelyPlayed,
                "current",
                &candidates,
                &HashSet::new(),
                &mut Random::with_seed(seed),
            )
            .unwrap();
            // 最後に再生したのが古い20曲（1〜20日）から選ぶ
            assert_ne!(picked, "often");
            let index: usize = picked[1..].parse().unwrap();
            assert!(index < 20, "picked {picked}");
        }
    }

    #[test]
    fn test_pick_randomly_without_history() {
        let candidates = vec![candidate("current"), candidate("a"), candidate("b")];
        let mut seen = HashSet::new();
        for seed in 0..50 {
            seen.insert(
                pick(
                    AiShuffleMode::FrequentlyPlayed,
                    "current",
                    &candidates,
                    &HashSet::new(),
                    &mut Random::with_seed(seed),
                )
                .unwrap(),
            );
        }
        assert_eq!(seen, excluded(&["a", "b"]));
    }

    #[test]
    fn test_random_below_stays_in_range() {
        let mut random = Random::with_seed(42);
        for bound in 1..50 {
            assert!(random.below(bound) < bound);
        }
    }
}
