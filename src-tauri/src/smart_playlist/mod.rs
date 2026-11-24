//! オートプレイリスト（条件で曲を集めるプレイリスト。ADR-042・ADR-127）
//!
//! 通常のプレイリスト（`playlist.rs`）とは別の表（`smart_playlists`）に持つ。アプリの中だけの機能で、
//! 通常のプレイリスト・M3U8へ書き出すことはあっても、取り込むことはない（M3Uの読み込み・ライブラリの
//! XMLの取り込みは、通常のプレイリストだけを作る）。フォルダには入れない。
//!
//! - このモジュール: 表の読み書き（作成・条件の変更・名前と説明・並び順・削除・曲の取得・
//!   通常のプレイリストとしての保存）
//! - `rules`: 条件（`SmartRules`）の型・検証・SQLの組み立て・ランダムな並び
//!
//! 曲の一覧は保存せず、開くたび・転送するたび・書き出すたびに条件から求める。どの関数も`AppResult`を
//! 返す。オートプレイリストが見つからない場合は`NotFound`、SQLの失敗は`Database`。

mod rules;

pub use rules::*;

use crate::error::{AppError, AppResult};
use crate::models::{Playlist, SmartPlaylist, Track};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

/// オートプレイリストが見つからない時のメッセージ
const NOT_FOUND: &str = "オートプレイリストが見つかりません";

/// SQLの失敗を、操作の名前を付けた`Database`にする
fn database(operation: &'static str) -> impl Fn(rusqlite::Error) -> AppError + Copy {
    move |e| AppError::Database(format!("{operation}に失敗しました: {e}"))
}

fn not_found() -> AppError {
    AppError::NotFound(NOT_FOUND.to_string())
}

fn rules_to_json(rules: &SmartRules) -> rusqlite::Result<String> {
    serde_json::to_string(rules).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

/// 保存してある条件を読む
///
/// 読めない条件（新しいバージョンで足した項目を、前のバージョンで開いた場合など）は、どの曲にも
/// 合わない条件にする（`SmartRules::unreadable`。条件を編集し直せる）。
fn parse_rules(json: &str) -> SmartRules {
    serde_json::from_str(json).unwrap_or_else(|_| SmartRules::unreadable())
}

/// すべてのオートプレイリストを取得（手動の並び順。表示の並び順は画面が選ぶ）
pub fn get_all_smart_playlists(conn: &Connection) -> AppResult<Vec<SmartPlaylist>> {
    let database = database("オートプレイリストの取得");
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, rules, position, created_at, updated_at
             FROM smart_playlists ORDER BY position, created_at",
        )
        .map_err(database)?;
    stmt.query_map([], |row| {
        Ok(SmartPlaylist {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            rules: parse_rules(&row.get::<_, String>(3)?),
            position: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })
    .and_then(|rows| rows.collect())
    .map_err(database)
}

/// オートプレイリストの条件を取得する（Noneは、オートプレイリストがない場合）
pub(crate) fn find_rules(
    conn: &Connection,
    smart_playlist_id: &str,
) -> AppResult<Option<SmartRules>> {
    conn.query_row(
        "SELECT rules FROM smart_playlists WHERE id = ?1",
        [smart_playlist_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map(|found| found.as_deref().map(parse_rules))
    .map_err(database("オートプレイリストの取得"))
}

/// オートプレイリストの名前を取得する（M3U8の書き出しの、ファイル名の既定）
pub fn get_smart_playlist_name(conn: &Connection, smart_playlist_id: &str) -> AppResult<String> {
    conn.query_row(
        "SELECT name FROM smart_playlists WHERE id = ?1",
        [smart_playlist_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(database("オートプレイリストの取得"))?
    .ok_or_else(not_found)
}

/// オートプレイリストを作成（手動の並び順では、いちばん後ろに置く）
///
/// 条件にしたプレイリスト・オートプレイリストがない場合は断る（`validate_playlist_references`）。
pub fn create_smart_playlist(
    conn: &Connection,
    name: &str,
    rules: &SmartRules,
) -> AppResult<SmartPlaylist> {
    validate_playlist_references(conn, None, rules)?;

    let database = database("オートプレイリストの作成");
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let position = crate::playlist::next_position(conn, "smart_playlists").map_err(database)?;
    conn.execute(
        "INSERT INTO smart_playlists (id, name, rules, position, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        rusqlite::params![
            id,
            name,
            rules_to_json(rules).map_err(database)?,
            position,
            now
        ],
    )
    .map_err(database)?;

    Ok(SmartPlaylist {
        id,
        name: name.to_string(),
        description: None,
        rules: rules.clone(),
        position,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// オートプレイリストの名前と条件を変更（条件の編集画面の保存）
///
/// 条件にしたプレイリスト・オートプレイリストがない場合と、そのオートプレイリスト自身や互いを条件に
/// する場合は断る（`validate_playlist_references`）。
pub fn update_smart_playlist(
    conn: &Connection,
    smart_playlist_id: &str,
    name: &str,
    rules: &SmartRules,
) -> AppResult<()> {
    if find_rules(conn, smart_playlist_id)?.is_none() {
        return Err(not_found());
    }
    validate_playlist_references(conn, Some(smart_playlist_id), rules)?;

    let database = database("オートプレイリストの変更");
    conn.execute(
        "UPDATE smart_playlists SET name = ?1, rules = ?2, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![
            name,
            rules_to_json(rules).map_err(database)?,
            Utc::now().to_rfc3339(),
            smart_playlist_id
        ],
    )
    .map_err(database)?;
    Ok(())
}

/// オートプレイリストの名前と説明を変更（`description`がNone・空白だけなら、説明を消す）
pub fn update_smart_playlist_info(
    conn: &Connection,
    smart_playlist_id: &str,
    name: &str,
    description: Option<&str>,
) -> AppResult<()> {
    let description = description.map(str::trim).filter(|text| !text.is_empty());
    let changed = conn
        .execute(
            "UPDATE smart_playlists SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![
                name,
                description,
                Utc::now().to_rfc3339(),
                smart_playlist_id
            ],
        )
        .map_err(database("オートプレイリストの変更"))?;
    if changed == 0 {
        return Err(not_found());
    }
    Ok(())
}

/// オートプレイリストを、渡した順に並べる（手動の並び順。渡さなかったものの位置は変えない）
pub fn reorder_smart_playlists(conn: &Connection, smart_playlist_ids: &[String]) -> AppResult<()> {
    crate::playlist::set_positions(conn, "smart_playlists", smart_playlist_ids)
        .map_err(database("オートプレイリストの並び替え"))
}

/// オートプレイリストを削除（転送先デバイスの選択は、外部キーのCASCADEで消える）
///
/// ほかのオートプレイリストの条件にしていても、条件は残す（曲のないオートプレイリストとして扱う。ADR-116）。
pub fn delete_smart_playlist(conn: &Connection, smart_playlist_id: &str) -> AppResult<()> {
    let removed = conn
        .execute(
            "DELETE FROM smart_playlists WHERE id = ?1",
            [smart_playlist_id],
        )
        .map_err(database("オートプレイリストの削除"))?;
    if removed == 0 {
        return Err(not_found());
    }
    Ok(())
}

/// オートプレイリストの曲（条件に合う曲を、条件の並び順で。上限があれば、その曲数まで）
///
/// オートプレイリストが見つからない場合は、空の一覧を返す（通常のプレイリストと同じ）。
pub fn get_smart_playlist_tracks(
    conn: &Connection,
    smart_playlist_id: &str,
    context: EvalContext,
) -> AppResult<Vec<Track>> {
    match find_rules(conn, smart_playlist_id)? {
        Some(rules) => find_tracks(conn, &rules, context),
        None => Ok(Vec::new()),
    }
}

/// オートプレイリストの曲のID（`get_smart_playlist_tracks`と同じ曲。デバイスへの転送・書き出し用）
pub fn smart_playlist_track_ids(
    conn: &Connection,
    smart_playlist: &SmartPlaylist,
    context: EvalContext,
) -> AppResult<Vec<String>> {
    Ok(find_tracks(conn, &smart_playlist.rules, context)?
        .into_iter()
        .map(|track| track.id)
        .collect())
}

/// オートプレイリストの今の曲（並び順のまま）を入れた、通常のプレイリストを作る
///
/// 作ったプレイリストは、その後の条件の変更・曲の評価などの変更に追随しない（曲を自分で入れた
/// プレイリストと同じ）。条件に合う曲がなくても、空のプレイリストを作る。
pub fn save_as_playlist(
    conn: &mut Connection,
    smart_playlist_id: &str,
    name: &str,
    context: EvalContext,
) -> AppResult<Playlist> {
    let rules = find_rules(conn, smart_playlist_id)?.ok_or_else(not_found)?;
    let track_ids: Vec<String> = find_tracks(conn, &rules, context)?
        .into_iter()
        .map(|track| track.id)
        .collect();
    crate::playlist::create_playlist_from_tracks(conn, name, &track_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> EvalContext {
        EvalContext {
            now: Utc::now(),
            shuffle_seed: 1,
        }
    }

    fn jazz_rules() -> SmartRules {
        SmartRules {
            match_mode: MatchMode::All,
            rules: vec![SmartRule::Text {
                field: TextField::Genre,
                op: TextOp::Is,
                value: "Jazz".to_string(),
            }],
            order: SmartOrder {
                field: SmartOrderField::Title,
                descending: false,
            },
            limit: None,
        }
    }

    fn setup() -> Connection {
        let conn = crate::db::open_in_memory();
        for (id, genre) in [("track1", "Jazz"), ("track2", "Rock"), ("track3", "Jazz")] {
            conn.execute(
                "INSERT INTO tracks (id, file_path, file_name, title, genre, file_size, format)
                 VALUES (?1, ?2, ?3, ?1, ?4, 0, 'mp3')",
                rusqlite::params![id, format!("/test/{id}.mp3"), format!("{id}.mp3"), genre],
            )
            .unwrap();
        }
        conn
    }

    fn track_ids(conn: &Connection, smart_playlist_id: &str) -> Vec<String> {
        get_smart_playlist_tracks(conn, smart_playlist_id, context())
            .unwrap()
            .into_iter()
            .map(|track| track.id)
            .collect()
    }

    /// オートプレイリストの曲は、開くたびに条件から求める（曲のタグが変われば、一覧も変わる）
    #[test]
    fn test_tracks_follow_the_rules() {
        let conn = setup();
        let smart = create_smart_playlist(&conn, "ジャズ", &jazz_rules()).unwrap();
        assert_eq!(track_ids(&conn, &smart.id), ["track1", "track3"]);

        conn.execute("UPDATE tracks SET genre = 'Jazz' WHERE id = 'track2'", [])
            .unwrap();
        assert_eq!(track_ids(&conn, &smart.id), ["track1", "track2", "track3"]);

        let all = get_all_smart_playlists(&conn).unwrap();
        assert_eq!(all, [smart.clone()]);
        assert_eq!(
            smart_playlist_track_ids(&conn, &all[0], context()).unwrap(),
            ["track1", "track2", "track3"]
        );
        // 通常のプレイリストの一覧には出ない
        assert!(
            crate::playlist::get_all_playlists(&conn)
                .unwrap()
                .is_empty()
        );
        // ないオートプレイリストは、空の一覧
        assert!(track_ids(&conn, "missing").is_empty());
    }

    #[test]
    fn test_update_and_delete() {
        let conn = setup();
        let smart = create_smart_playlist(&conn, "ジャズ", &jazz_rules()).unwrap();
        let mut rules = jazz_rules();
        rules.limit = Some(1);

        update_smart_playlist(&conn, &smart.id, "ジャズ1曲", &rules).unwrap();
        update_smart_playlist_info(&conn, &smart.id, "ジャズ（1曲）", Some("メモ")).unwrap();
        let stored = &get_all_smart_playlists(&conn).unwrap()[0];
        assert_eq!(
            (stored.name.as_str(), stored.description.as_deref()),
            ("ジャズ（1曲）", Some("メモ"))
        );
        assert_eq!(stored.rules, rules);
        assert_eq!(track_ids(&conn, &smart.id), ["track1"]);

        // 通常のプレイリスト・ないオートプレイリストは、変えられない
        let manual = crate::playlist::create_playlist(&conn, "通勤").unwrap();
        for id in [manual.id.as_str(), "missing"] {
            assert!(matches!(
                update_smart_playlist(&conn, id, "x", &rules),
                Err(AppError::NotFound(_))
            ));
            assert!(matches!(
                update_smart_playlist_info(&conn, id, "x", None),
                Err(AppError::NotFound(_))
            ));
            assert!(matches!(
                delete_smart_playlist(&conn, id),
                Err(AppError::NotFound(_))
            ));
        }

        delete_smart_playlist(&conn, &smart.id).unwrap();
        assert!(get_all_smart_playlists(&conn).unwrap().is_empty());
        assert_eq!(crate::playlist::get_all_playlists(&conn).unwrap().len(), 1);
    }

    #[test]
    fn test_manual_order() {
        let conn = setup();
        let ids: Vec<String> = ["A", "B", "C"]
            .into_iter()
            .map(|name| {
                create_smart_playlist(&conn, name, &jazz_rules())
                    .unwrap()
                    .id
            })
            .collect();
        let names = |conn: &Connection| -> Vec<String> {
            get_all_smart_playlists(conn)
                .unwrap()
                .into_iter()
                .map(|playlist| playlist.name)
                .collect()
        };
        assert_eq!(names(&conn), ["A", "B", "C"]);

        reorder_smart_playlists(&conn, &[ids[2].clone(), ids[0].clone(), ids[1].clone()]).unwrap();
        assert_eq!(names(&conn), ["C", "A", "B"]);
    }

    /// 今の曲を、並び順のまま通常のプレイリストにする（その後の変更には追随しない）
    #[test]
    fn test_save_as_playlist() {
        let mut conn = setup();
        let mut rules = jazz_rules();
        rules.order.descending = true;
        let smart = create_smart_playlist(&conn, "ジャズ", &rules).unwrap();

        let playlist = save_as_playlist(&mut conn, &smart.id, "ジャズ（保存）", context()).unwrap();
        assert_eq!(playlist.name, "ジャズ（保存）");
        assert_eq!(
            crate::playlist::playlist_track_ids(&playlist),
            ["track3", "track1"]
        );

        conn.execute("UPDATE tracks SET genre = 'Jazz' WHERE id = 'track2'", [])
            .unwrap();
        let stored = crate::playlist::get_all_playlists(&conn).unwrap();
        assert_eq!(
            crate::playlist::playlist_track_ids(&stored[0]),
            ["track3", "track1"]
        );

        assert!(matches!(
            save_as_playlist(&mut conn, "missing", "x", context()),
            Err(AppError::NotFound(_))
        ));
    }

    /// 読めない条件（新しいバージョンで足した項目など）は、曲のないオートプレイリストとして扱う
    #[test]
    fn test_unreadable_rules_match_no_tracks() {
        let conn = setup();
        let smart = create_smart_playlist(&conn, "ジャズ", &jazz_rules()).unwrap();
        conn.execute(
            "UPDATE smart_playlists SET rules = '{\"matchMode\":\"future\"}' WHERE id = ?1",
            [&smart.id],
        )
        .unwrap();

        let all = get_all_smart_playlists(&conn).unwrap();
        assert_eq!(all[0].rules, SmartRules::unreadable());
        assert!(track_ids(&conn, &smart.id).is_empty());
    }
}
