use rusqlite::{Connection, Result};
use std::path::PathBuf;

/// データベース接続を初期化
pub fn init_db(db_path: PathBuf) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    run_migrations(&conn)?;
    Ok(conn)
}

/// データベースマイグレーションを実行
fn run_migrations(conn: &Connection) -> Result<()> {
    // tracksテーブルの作成
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tracks (
            id TEXT PRIMARY KEY,
            file_path TEXT UNIQUE NOT NULL,
            file_name TEXT NOT NULL,
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
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        )",
        [],
    )?;

    // playlistsテーブルの作成
    conn.execute(
        "CREATE TABLE IF NOT EXISTS playlists (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        )",
        [],
    )?;

    // playlist_tracksテーブルの作成
    conn.execute(
        "CREATE TABLE IF NOT EXISTS playlist_tracks (
            playlist_id TEXT,
            track_id TEXT,
            position INTEGER,
            added_at TEXT DEFAULT (datetime('now')),
            PRIMARY KEY (playlist_id, track_id),
            FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
            FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // パフォーマンス向上のためのインデックス作成
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_playlist_tracks_playlist_id ON playlist_tracks(playlist_id)",
        [],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_init_db() {
        let test_db_path = PathBuf::from("test_music.db");

        // テスト用データベースを作成
        let result = init_db(test_db_path.clone());
        assert!(result.is_ok());

        let conn = result.unwrap();

        // テーブルが作成されたことを確認
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"tracks".to_string()));
        assert!(tables.contains(&"playlists".to_string()));
        assert!(tables.contains(&"playlist_tracks".to_string()));

        // テスト後にクリーンアップ
        drop(conn);
        fs::remove_file(test_db_path).ok();
    }
}
