use crate::models::{Playlist, PlaylistTrack};
use chrono::Utc;
use rusqlite::{Connection, Result};
use uuid::Uuid;

/// プレイリストを作成
pub fn create_playlist(conn: &Connection, name: &str) -> Result<Playlist> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO playlists (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, name, now, now],
    )?;

    Ok(Playlist {
        id,
        name: name.to_string(),
        description: None,
        tracks: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    })
}

/// すべてのプレイリストを取得
pub fn get_all_playlists(conn: &Connection) -> Result<Vec<Playlist>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, created_at, updated_at FROM playlists ORDER BY created_at DESC",
    )?;

    let playlists = stmt
        .query_map([], |row| {
            let playlist_id: String = row.get(0)?;
            let tracks = get_playlist_tracks(conn, &playlist_id).unwrap_or_default();

            Ok(Playlist {
                id: playlist_id,
                name: row.get(1)?,
                description: row.get(2)?,
                tracks,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(playlists)
}

/// プレイリストのトラックを取得
fn get_playlist_tracks(conn: &Connection, playlist_id: &str) -> Result<Vec<PlaylistTrack>> {
    let mut stmt = conn.prepare(
        "SELECT track_id, position, added_at FROM playlist_tracks 
         WHERE playlist_id = ?1 ORDER BY position",
    )?;

    let tracks = stmt
        .query_map([playlist_id], |row| {
            Ok(PlaylistTrack {
                track_id: row.get(0)?,
                position: row.get(1)?,
                added_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tracks)
}

/// プレイリストにトラックを追加
pub fn add_track_to_playlist(conn: &Connection, playlist_id: &str, track_id: &str) -> Result<()> {
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
        return Ok(()); // 既に存在する場合はスキップ
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

    Ok(())
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

    #[test]
    fn test_add_track_to_playlist() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        insert_test_track(&conn, "track1");

        let result = add_track_to_playlist(&conn, &playlist.id, "track1");
        assert!(result.is_ok());

        let playlists = get_all_playlists(&conn).unwrap();
        assert_eq!(playlists[0].tracks.len(), 1);
        assert_eq!(playlists[0].tracks[0].track_id, "track1");
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
}
