use crate::error::{AppError, AppResult};
use crate::models::{Playlist, PlaylistTrack, Track};
use crate::repository::{TRACK_COLUMNS, query_tracks};
use crate::smart_playlist::{self, EvalContext, SmartRules};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Result};
use std::collections::HashMap;
use uuid::Uuid;

/// プレイリストを作成
pub fn create_playlist(conn: &Connection, name: &str) -> Result<Playlist> {
    insert_playlist(conn, name, None)
}

/// 自動プレイリスト（条件で曲を集めるプレイリスト）を作成
pub fn create_smart_playlist(
    conn: &Connection,
    name: &str,
    rules: &SmartRules,
) -> Result<Playlist> {
    insert_playlist(conn, name, Some(rules))
}

fn insert_playlist(conn: &Connection, name: &str, rules: Option<&SmartRules>) -> Result<Playlist> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO playlists (id, name, rules, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        rusqlite::params![id, name, rules.map(rules_to_json).transpose()?, now],
    )?;

    Ok(Playlist {
        id,
        name: name.to_string(),
        description: None,
        tracks: Vec::new(),
        rules: rules.cloned(),
        created_at: now.clone(),
        updated_at: now,
    })
}

fn rules_to_json(rules: &SmartRules) -> Result<String> {
    serde_json::to_string(rules).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

/// 保存してある条件（`playlists.rules`）を読む（NULLは、通常のプレイリスト）
///
/// 読めない条件は、どの曲にも合わない条件にする（`SmartRules::unreadable`）。
fn parse_rules(json: Option<String>) -> Option<SmartRules> {
    json.map(|json| serde_json::from_str(&json).unwrap_or_else(|_| SmartRules::unreadable()))
}

/// プレイリストの条件を取得する
///
/// 外側の`None`はプレイリストがない場合、内側の`None`は通常のプレイリスト。
fn find_rules(conn: &Connection, playlist_id: &str) -> Result<Option<Option<SmartRules>>> {
    conn.query_row(
        "SELECT rules FROM playlists WHERE id = ?1",
        [playlist_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|found| found.map(parse_rules))
}

/// 曲を自分で選ぶ通常のプレイリストであることを確かめる（曲の追加・削除・並べ替えの前に呼ぶ）
///
/// 自動プレイリストの曲は条件で決まるため、変更できない。プレイリストがない場合は、
/// このあとの操作が「見つからない」を返すため、ここでは通す。
pub fn ensure_manual_playlist(conn: &Connection, playlist_id: &str) -> AppResult<()> {
    let rules = find_rules(conn, playlist_id)
        .map_err(|e| AppError::Database(format!("プレイリストの取得に失敗しました: {}", e)))?;
    if matches!(rules, Some(Some(_))) {
        return Err(AppError::Validation(
            "自動プレイリストの曲は条件で決まるため、変更できません".to_string(),
        ));
    }
    Ok(())
}

/// 自動プレイリストの条件を変更
///
/// 通常のプレイリストは、自動プレイリストに変えない（入れてある曲が見えなくなるため）。
pub fn update_smart_playlist_rules(
    conn: &Connection,
    playlist_id: &str,
    rules: &SmartRules,
) -> AppResult<()> {
    let database = |e: rusqlite::Error| {
        AppError::Database(format!("プレイリストの条件の変更に失敗しました: {}", e))
    };
    match find_rules(conn, playlist_id).map_err(database)? {
        None => {
            return Err(AppError::NotFound(
                "プレイリストが見つかりません".to_string(),
            ));
        }
        Some(None) => {
            return Err(AppError::Validation(
                "通常のプレイリストには、条件を設定できません".to_string(),
            ));
        }
        Some(Some(_)) => {}
    }

    conn.execute(
        "UPDATE playlists SET rules = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![
            rules_to_json(rules).map_err(database)?,
            Utc::now().to_rfc3339(),
            playlist_id
        ],
    )
    .map_err(database)?;
    Ok(())
}

/// プレイリストの名前を取得する（プレイリストがなければ`QueryReturnedNoRows`）
pub fn get_playlist_name(conn: &Connection, playlist_id: &str) -> Result<String> {
    conn.query_row(
        "SELECT name FROM playlists WHERE id = ?1",
        [playlist_id],
        |row| row.get(0),
    )
}

/// 曲を入れた状態で、プレイリストを作る（M3Uの読み込み用）
///
/// 同じ名前のプレイリストがある場合は、番号を付けた名前にする（`m3u::unique_playlist_name`）。
/// 1つのトランザクションで行い、途中で失敗した場合は何も作らない。
/// `track_ids`には、同じトラックを2回含めない。
pub fn create_playlist_with_tracks(
    conn: &mut Connection,
    name: &str,
    track_ids: &[String],
) -> Result<Playlist> {
    let tx = conn.transaction()?;

    let existing: Vec<String> = tx
        .prepare("SELECT name FROM playlists")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_>>()?;
    let name = crate::m3u::unique_playlist_name(name, &existing);

    let mut playlist = create_playlist(&tx, &name)?;
    {
        let mut insert = tx.prepare(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (position, track_id) in track_ids.iter().enumerate() {
            insert.execute(rusqlite::params![
                playlist.id,
                track_id,
                position as i32,
                playlist.created_at
            ])?;
            playlist.tracks.push(PlaylistTrack {
                track_id: track_id.clone(),
                position: position as i32,
                added_at: playlist.created_at.clone(),
            });
        }
    }

    tx.commit()?;
    Ok(playlist)
}

/// すべてのプレイリストを取得
///
/// プレイリストごとにトラックを問い合わせるとN+1クエリになるため、
/// 全プレイリストのトラックを1クエリで取得してから割り当てる。
pub fn get_all_playlists(conn: &Connection) -> Result<Vec<Playlist>> {
    // 1. 全プレイリストのトラックをまとめて取得し、プレイリストIDごとに分配する
    let mut tracks_by_playlist: HashMap<String, Vec<PlaylistTrack>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT playlist_id, track_id, position, added_at FROM playlist_tracks
             ORDER BY playlist_id, position",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                PlaylistTrack {
                    track_id: row.get(1)?,
                    position: row.get(2)?,
                    added_at: row.get(3)?,
                },
            ))
        })?;

        for row in rows {
            let (playlist_id, track) = row?;
            tracks_by_playlist
                .entry(playlist_id)
                .or_default()
                .push(track);
        }
    }

    // 2. プレイリスト本体を取得し、対応するトラックを割り当てる
    let mut stmt = conn.prepare(
        "SELECT id, name, description, created_at, updated_at, rules
         FROM playlists ORDER BY created_at DESC",
    )?;

    let playlists = stmt
        .query_map([], |row| {
            let playlist_id: String = row.get(0)?;
            let tracks = tracks_by_playlist.remove(&playlist_id).unwrap_or_default();
            let rules = parse_rules(row.get(5)?);

            Ok(Playlist {
                id: playlist_id,
                name: row.get(1)?,
                description: row.get(2)?,
                // 自動プレイリストの曲は、開く時に条件から求める（ここでは持たない）
                tracks: if rules.is_some() { Vec::new() } else { tracks },
                rules,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(playlists)
}

/// プレイリストの曲を取得（プレイリストの中の並び順）
///
/// 自動プレイリストでは、条件に合う曲を、条件の並び順で返す。
/// プレイリストが見つからない場合は、空の一覧を返す。
pub fn get_playlist_tracks(
    conn: &Connection,
    playlist_id: &str,
    context: EvalContext,
) -> AppResult<Vec<Track>> {
    let rules = find_rules(conn, playlist_id)
        .map_err(|e| AppError::Database(format!("プレイリストの取得に失敗しました: {}", e)))?;
    if let Some(Some(rules)) = rules {
        return smart_playlist::find_tracks(conn, &rules, context);
    }

    let sql = format!(
        "SELECT {} FROM tracks
         JOIN playlist_tracks ON playlist_tracks.track_id = tracks.id
         WHERE playlist_tracks.playlist_id = ?1
         ORDER BY playlist_tracks.position",
        TRACK_COLUMNS
    );
    query_tracks(conn, &sql, &[&playlist_id])
}

/// プレイリストの曲のIDを取得（プレイリストの中の並び順。自動プレイリストは、条件に合う曲）
///
/// `get_all_playlists`で取得したプレイリストから求める（デバイスへの転送用）。
pub fn playlist_track_ids(
    conn: &Connection,
    playlist: &Playlist,
    context: EvalContext,
) -> AppResult<Vec<String>> {
    match &playlist.rules {
        Some(rules) => Ok(smart_playlist::find_tracks(conn, rules, context)?
            .into_iter()
            .map(|track| track.id)
            .collect()),
        None => Ok(playlist
            .tracks
            .iter()
            .map(|track| track.track_id.clone())
            .collect()),
    }
}

/// プレイリストにトラックを追加
///
/// 追加した場合はtrue、すでに入っていて何もしなかった場合はfalseを返す。
fn add_track_to_playlist(conn: &Connection, playlist_id: &str, track_id: &str) -> Result<bool> {
    // プレイリストの存在確認
    let playlist_exists: bool = conn
        .prepare("SELECT 1 FROM playlists WHERE id = ?1")?
        .exists([playlist_id])?;

    if !playlist_exists {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    // トラックの存在確認
    let track_exists: bool = conn
        .prepare("SELECT 1 FROM tracks WHERE id = ?1")?
        .exists([track_id])?;

    if !track_exists {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    // 既に追加されているか確認
    let already_exists: bool = conn
        .prepare("SELECT 1 FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2")?
        .exists(rusqlite::params![playlist_id, track_id])?;

    if already_exists {
        return Ok(false); // 既に存在する場合はスキップ
    }

    // 現在の最大position値を取得
    let max_position: Option<i32> = conn
        .query_row(
            "SELECT MAX(position) FROM playlist_tracks WHERE playlist_id = ?1",
            [playlist_id],
            |row| row.get(0),
        )
        .ok();

    let new_position = max_position.unwrap_or(-1) + 1;
    let now = Utc::now().to_rfc3339();

    // トラックを追加
    conn.execute(
        "INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at) 
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![playlist_id, track_id, new_position, now],
    )?;

    // プレイリストのupdated_atを更新
    conn.execute(
        "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now, playlist_id],
    )?;

    Ok(true)
}

/// プレイリストに複数のトラックを、渡した順に追加する
///
/// すでに入っているトラック（同じIDを2回渡した場合の2回目を含む）は飛ばす。
/// 1つのトランザクションで行い、途中で失敗した場合は1曲も追加しない。
/// 追加したトラック数を返す。
pub fn add_tracks_to_playlist(
    conn: &Connection,
    playlist_id: &str,
    track_ids: &[String],
) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;

    let mut added = 0;
    for track_id in track_ids {
        if add_track_to_playlist(&tx, playlist_id, track_id)? {
            added += 1;
        }
    }

    tx.commit()?;
    Ok(added)
}

/// プレイリストからトラックを削除
pub fn remove_track_from_playlist(
    conn: &Connection,
    playlist_id: &str,
    track_id: &str,
) -> Result<()> {
    // トラックを削除
    let rows_affected = conn.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
        rusqlite::params![playlist_id, track_id],
    )?;

    if rows_affected == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    // position値を再計算して連番にする
    reorder_positions_after_deletion(conn, playlist_id)?;

    // プレイリストのupdated_atを更新
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now, playlist_id],
    )?;

    Ok(())
}

/// 削除後にposition値を再計算
fn reorder_positions_after_deletion(conn: &Connection, playlist_id: &str) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")?;

    let track_ids: Vec<String> = stmt
        .query_map([playlist_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    for (index, track_id) in track_ids.iter().enumerate() {
        conn.execute(
            "UPDATE playlist_tracks SET position = ?1 WHERE playlist_id = ?2 AND track_id = ?3",
            rusqlite::params![index as i32, playlist_id, track_id],
        )?;
    }

    Ok(())
}

/// プレイリストの名前を変更
pub fn rename_playlist(conn: &Connection, playlist_id: &str, name: &str) -> Result<()> {
    // プレイリストの存在確認
    let playlist_exists: bool = conn
        .prepare("SELECT 1 FROM playlists WHERE id = ?1")?
        .exists([playlist_id])?;

    if !playlist_exists {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    let now = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE playlists SET name = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![name, now, playlist_id],
    )?;

    Ok(())
}

/// プレイリストを削除
pub fn delete_playlist(conn: &Connection, playlist_id: &str) -> Result<()> {
    // プレイリストの存在確認
    let playlist_exists: bool = conn
        .prepare("SELECT 1 FROM playlists WHERE id = ?1")?
        .exists([playlist_id])?;

    if !playlist_exists {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    // トランザクションを開始
    let tx = conn.unchecked_transaction()?;

    // まずプレイリスト内のトラックを削除
    tx.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
        [playlist_id],
    )?;

    // プレイリスト自体を削除
    tx.execute("DELETE FROM playlists WHERE id = ?1", [playlist_id])?;

    tx.commit()?;

    Ok(())
}

/// プレイリスト内のトラックを並び替え
pub fn reorder_playlist_tracks(
    conn: &Connection,
    playlist_id: &str,
    track_ids: &[String],
) -> Result<()> {
    // プレイリストの存在確認
    let playlist_exists: bool = conn
        .prepare("SELECT 1 FROM playlists WHERE id = ?1")?
        .exists([playlist_id])?;

    if !playlist_exists {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    // トランザクションを開始
    let tx = conn.unchecked_transaction()?;

    for (index, track_id) in track_ids.iter().enumerate() {
        tx.execute(
            "UPDATE playlist_tracks SET position = ?1 
             WHERE playlist_id = ?2 AND track_id = ?3",
            rusqlite::params![index as i32, playlist_id, track_id],
        )?;
    }

    // プレイリストのupdated_atを更新
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now, playlist_id],
    )?;

    tx.commit()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();

        // テーブルを作成
        conn.execute(
            "CREATE TABLE playlists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                rules TEXT,
                created_at TEXT,
                updated_at TEXT
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE tracks (
                id TEXT PRIMARY KEY,
                file_path TEXT,
                file_name TEXT,
                title TEXT,
                artist TEXT,
                album TEXT,
                genre TEXT,
                year INTEGER,
                duration INTEGER,
                file_size INTEGER,
                format TEXT,
                bitrate INTEGER,
                sample_rate INTEGER,
                created_at TEXT,
                updated_at TEXT
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE playlist_tracks (
                playlist_id TEXT,
                track_id TEXT,
                position INTEGER,
                added_at TEXT,
                PRIMARY KEY (playlist_id, track_id)
            )",
            [],
        )
        .unwrap();

        conn
    }

    fn insert_test_track(conn: &Connection, id: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, title, created_at, updated_at, file_size, format)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'mp3')",
            rusqlite::params![id, "/test/path", "test.mp3", "Test Track", now, now],
        )
        .unwrap();
    }

    #[test]
    fn test_create_playlist() {
        let conn = setup_test_db();
        let result = create_playlist(&conn, "My Playlist");

        assert!(result.is_ok());
        let playlist = result.unwrap();
        assert_eq!(playlist.name, "My Playlist");
        assert!(playlist.tracks.is_empty());
    }

    #[test]
    fn test_get_all_playlists() {
        let conn = setup_test_db();
        create_playlist(&conn, "Playlist 1").unwrap();
        create_playlist(&conn, "Playlist 2").unwrap();

        let playlists = get_all_playlists(&conn).unwrap();
        assert_eq!(playlists.len(), 2);
    }

    /// 複数プレイリストのトラックを一括取得しても、正しい所属先に割り当てられること
    #[test]
    fn test_get_all_playlists_assigns_tracks_to_correct_playlist() {
        let conn = setup_test_db();
        let playlist_a = create_playlist(&conn, "Playlist A").unwrap();
        let playlist_b = create_playlist(&conn, "Playlist B").unwrap();
        let playlist_empty = create_playlist(&conn, "Playlist Empty").unwrap();

        for id in ["track1", "track2", "track3"] {
            insert_test_track(&conn, id);
        }

        add_track_to_playlist(&conn, &playlist_a.id, "track1").unwrap();
        add_track_to_playlist(&conn, &playlist_a.id, "track2").unwrap();
        add_track_to_playlist(&conn, &playlist_b.id, "track3").unwrap();

        let playlists = get_all_playlists(&conn).unwrap();
        let find = |id: &str| {
            playlists
                .iter()
                .find(|p| p.id == id)
                .expect("プレイリストが見つかりません")
        };

        // それぞれ自分のトラックだけを持ち、position順に並ぶ
        let a = find(&playlist_a.id);
        assert_eq!(
            a.tracks
                .iter()
                .map(|t| t.track_id.as_str())
                .collect::<Vec<_>>(),
            vec!["track1", "track2"]
        );

        let b = find(&playlist_b.id);
        assert_eq!(
            b.tracks
                .iter()
                .map(|t| t.track_id.as_str())
                .collect::<Vec<_>>(),
            vec!["track3"]
        );

        // トラックを持たないプレイリストは空になる
        assert!(find(&playlist_empty.id).tracks.is_empty());
    }

    #[test]
    fn test_add_track_to_playlist() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        insert_test_track(&conn, "track1");

        let result = add_track_to_playlist(&conn, &playlist.id, "track1");
        assert!(result.unwrap());

        let playlists = get_all_playlists(&conn).unwrap();
        assert_eq!(playlists[0].tracks.len(), 1);
        assert_eq!(playlists[0].tracks[0].track_id, "track1");
    }

    /// 渡した順に追加し、すでに入っているトラックは飛ばして、追加した数を返す
    #[test]
    fn test_add_tracks_to_playlist_keeps_order_and_skips_existing() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        for id in ["track1", "track2", "track3", "track4"] {
            insert_test_track(&conn, id);
        }
        add_track_to_playlist(&conn, &playlist.id, "track2").unwrap();

        let ids = ["track3", "track2", "track1", "track3"].map(String::from);
        let added = add_tracks_to_playlist(&conn, &playlist.id, &ids).unwrap();
        assert_eq!(added, 2);

        let playlists = get_all_playlists(&conn).unwrap();
        let order: Vec<(&str, i32)> = playlists[0]
            .tracks
            .iter()
            .map(|t| (t.track_id.as_str(), t.position))
            .collect();
        assert_eq!(order, [("track2", 0), ("track3", 1), ("track1", 2)]);
    }

    /// 途中のトラックが見つからない場合は、1曲も追加しない
    #[test]
    fn test_add_tracks_to_playlist_rolls_back_on_missing_track() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        insert_test_track(&conn, "track1");

        let ids = ["track1", "missing"].map(String::from);
        let result = add_tracks_to_playlist(&conn, &playlist.id, &ids);
        assert!(matches!(result, Err(rusqlite::Error::QueryReturnedNoRows)));

        let playlists = get_all_playlists(&conn).unwrap();
        assert!(playlists[0].tracks.is_empty());
    }

    #[test]
    fn test_remove_track_from_playlist() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        insert_test_track(&conn, "track1");
        insert_test_track(&conn, "track2");

        add_track_to_playlist(&conn, &playlist.id, "track1").unwrap();
        add_track_to_playlist(&conn, &playlist.id, "track2").unwrap();

        let result = remove_track_from_playlist(&conn, &playlist.id, "track1");
        assert!(result.is_ok());

        let playlists = get_all_playlists(&conn).unwrap();
        assert_eq!(playlists[0].tracks.len(), 1);
        assert_eq!(playlists[0].tracks[0].track_id, "track2");
        assert_eq!(playlists[0].tracks[0].position, 0); // position再計算確認
    }

    #[test]
    fn test_reorder_playlist_tracks() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        insert_test_track(&conn, "track1");
        insert_test_track(&conn, "track2");
        insert_test_track(&conn, "track3");

        add_track_to_playlist(&conn, &playlist.id, "track1").unwrap();
        add_track_to_playlist(&conn, &playlist.id, "track2").unwrap();
        add_track_to_playlist(&conn, &playlist.id, "track3").unwrap();

        // 順序を変更: track3, track1, track2
        let new_order = vec![
            "track3".to_string(),
            "track1".to_string(),
            "track2".to_string(),
        ];
        let result = reorder_playlist_tracks(&conn, &playlist.id, &new_order);
        assert!(result.is_ok());

        let playlists = get_all_playlists(&conn).unwrap();
        assert_eq!(playlists[0].tracks[0].track_id, "track3");
        assert_eq!(playlists[0].tracks[1].track_id, "track1");
        assert_eq!(playlists[0].tracks[2].track_id, "track2");
    }

    /// プレイリストの曲を、曲の情報とあわせて並び順どおりに取得できること
    #[test]
    fn test_get_playlist_tracks_returns_tracks_in_playlist_order() {
        // 曲の全項目を読むため、実際のスキーマを使う
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        let other = create_playlist(&conn, "Other Playlist").unwrap();
        for id in ["track1", "track2", "track3"] {
            conn.execute(
                "INSERT INTO tracks (id, file_path, file_name, title, file_size, format)
                 VALUES (?1, ?2, ?3, ?1, 0, 'mp3')",
                rusqlite::params![id, format!("/test/{id}.mp3"), format!("{id}.mp3")],
            )
            .unwrap();
        }
        add_tracks_to_playlist(
            &conn,
            &playlist.id,
            &["track1".to_string(), "track2".to_string()],
        )
        .unwrap();
        add_tracks_to_playlist(&conn, &other.id, &["track3".to_string()]).unwrap();
        reorder_playlist_tracks(
            &conn,
            &playlist.id,
            &["track2".to_string(), "track1".to_string()],
        )
        .unwrap();

        let tracks = get_playlist_tracks(&conn, &playlist.id, context()).unwrap();

        let ids: Vec<&str> = tracks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["track2", "track1"]);
        assert_eq!(tracks[0].title.as_deref(), Some("track2"));
        assert_eq!(tracks[0].file_path, "/test/track2.mp3");

        // 見つからないプレイリストは、空の一覧
        assert!(
            get_playlist_tracks(&conn, "missing", context())
                .unwrap()
                .is_empty()
        );
    }

    fn context() -> EvalContext {
        EvalContext {
            now: Utc::now(),
            shuffle_seed: 1,
        }
    }

    fn jazz_rules() -> SmartRules {
        use crate::smart_playlist::{
            MatchMode, SmartOrder, SmartOrderField, SmartRule, TextField, TextOp,
        };
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

    fn smart_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
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

    /// 自動プレイリストの曲は、開くたびに条件から求める（曲のタグが変われば、一覧も変わる）
    #[test]
    fn test_smart_playlist_tracks_follow_the_rules() {
        let conn = smart_db();
        let playlist = create_smart_playlist(&conn, "ジャズ", &jazz_rules()).unwrap();
        let track_ids = |conn: &Connection| -> Vec<String> {
            get_playlist_tracks(conn, &playlist.id, context())
                .unwrap()
                .into_iter()
                .map(|track| track.id)
                .collect()
        };

        assert_eq!(track_ids(&conn), ["track1", "track3"]);

        conn.execute("UPDATE tracks SET genre = 'Jazz' WHERE id = 'track2'", [])
            .unwrap();
        assert_eq!(track_ids(&conn), ["track1", "track2", "track3"]);

        // 一覧には条件を持たせ、曲は持たせない（曲のIDは、条件から求める）
        let playlists = get_all_playlists(&conn).unwrap();
        assert_eq!(playlists[0].rules, Some(jazz_rules()));
        assert!(playlists[0].tracks.is_empty());
        assert_eq!(
            playlist_track_ids(&conn, &playlists[0], context()).unwrap(),
            ["track1", "track2", "track3"]
        );
    }

    #[test]
    fn test_playlist_track_ids_of_a_manual_playlist_keep_the_order() {
        let conn = smart_db();
        let manual = create_playlist(&conn, "通勤").unwrap();
        add_tracks_to_playlist(
            &conn,
            &manual.id,
            &["track3".to_string(), "track1".to_string()],
        )
        .unwrap();

        let playlists = get_all_playlists(&conn).unwrap();
        assert_eq!(
            playlist_track_ids(&conn, &playlists[0], context()).unwrap(),
            ["track3", "track1"]
        );
    }

    #[test]
    fn test_update_smart_playlist_rules() {
        let conn = smart_db();
        let smart = create_smart_playlist(&conn, "ジャズ", &jazz_rules()).unwrap();
        let manual = create_playlist(&conn, "通勤").unwrap();
        let mut rules = jazz_rules();
        rules.limit = Some(1);

        update_smart_playlist_rules(&conn, &smart.id, &rules).unwrap();
        assert_eq!(
            get_playlist_tracks(&conn, &smart.id, context())
                .unwrap()
                .len(),
            1
        );

        // 通常のプレイリスト・ないプレイリストには、条件を設定できない
        assert!(matches!(
            update_smart_playlist_rules(&conn, &manual.id, &rules),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            update_smart_playlist_rules(&conn, "missing", &rules),
            Err(AppError::NotFound(_))
        ));
        assert_eq!(get_all_playlists(&conn).unwrap().len(), 2);
    }

    /// 自動プレイリストの曲は、追加・削除・並べ替えの対象にしない
    #[test]
    fn test_ensure_manual_playlist() {
        let conn = smart_db();
        let smart = create_smart_playlist(&conn, "ジャズ", &jazz_rules()).unwrap();
        let manual = create_playlist(&conn, "通勤").unwrap();

        assert!(ensure_manual_playlist(&conn, &manual.id).is_ok());
        assert!(matches!(
            ensure_manual_playlist(&conn, &smart.id),
            Err(AppError::Validation(_))
        ));
        // ないプレイリストは、このあとの操作が「見つからない」を返す
        assert!(ensure_manual_playlist(&conn, "missing").is_ok());
    }

    /// 読めない条件（新しいバージョンで足した項目など）は、曲のない自動プレイリストとして扱う
    #[test]
    fn test_unreadable_rules_keep_the_playlist_smart_and_empty() {
        let conn = smart_db();
        let smart = create_smart_playlist(&conn, "ジャズ", &jazz_rules()).unwrap();
        conn.execute(
            "UPDATE playlists SET rules = '{\"matchMode\":\"future\"}' WHERE id = ?1",
            [&smart.id],
        )
        .unwrap();

        let playlists = get_all_playlists(&conn).unwrap();
        assert_eq!(playlists[0].rules, Some(SmartRules::unreadable()));
        assert!(
            get_playlist_tracks(&conn, &smart.id, context())
                .unwrap()
                .is_empty()
        );
        assert!(ensure_manual_playlist(&conn, &smart.id).is_err());
    }
}
