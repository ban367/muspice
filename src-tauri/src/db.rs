use rusqlite::{Connection, Result};
use std::path::PathBuf;

/// データベース接続を初期化
pub fn init_db(db_path: PathBuf) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    run_migrations(&conn)?;
    Ok(conn)
}

/// カラムが存在しない場合のみ追加するヘルパー関数
fn add_column_if_not_exists(
    conn: &Connection,
    table: &str,
    column: &str,
    column_def: &str,
) -> Result<()> {
    // PRAGMA table_infoでカラムの存在をチェック
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let column_exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .any(|name| name.map(|n| n == column).unwrap_or(false));

    if !column_exists {
        conn.execute(
            &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, column_def),
            [],
        )?;
    }

    Ok(())
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

    // お気に入り/レーティング/再生統計のカラムを追加（既存テーブルへのマイグレーション）
    // カラムが存在しない場合のみ追加
    add_column_if_not_exists(conn, "tracks", "is_favorite", "INTEGER DEFAULT 0")?;
    add_column_if_not_exists(conn, "tracks", "rating", "INTEGER DEFAULT 0")?;
    add_column_if_not_exists(conn, "tracks", "play_count", "INTEGER DEFAULT 0")?;
    add_column_if_not_exists(conn, "tracks", "last_played_at", "TEXT")?;

    // トラック番号/ディスク番号のカラムを追加
    add_column_if_not_exists(conn, "tracks", "track_number", "INTEGER")?;
    add_column_if_not_exists(conn, "tracks", "disc_number", "INTEGER")?;

    // 再生履歴テーブルの作成
    conn.execute(
        "CREATE TABLE IF NOT EXISTS play_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            track_id TEXT NOT NULL,
            played_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_play_history_track_id ON play_history(track_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_play_history_played_at ON play_history(played_at)",
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

    // 全文検索用の仮想テーブルを作成（FTS5）
    // 既存のテーブルがある場合はスキップ
    let fts_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tracks_fts'",
            [],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )
        .unwrap_or(false);

    if !fts_exists {
        conn.execute(
            "CREATE VIRTUAL TABLE tracks_fts USING fts5(
                id UNINDEXED,
                title,
                artist,
                album,
                genre,
                content=tracks,
                content_rowid=rowid
            )",
            [],
        )?;

        // 既存データをFTSテーブルに同期
        conn.execute(
            "INSERT INTO tracks_fts(rowid, id, title, artist, album, genre)
             SELECT rowid, id, title, artist, album, genre FROM tracks",
            [],
        )?;

        // トリガーを作成してデータの同期を自動化
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
                INSERT INTO tracks_fts(rowid, id, title, artist, album, genre)
                VALUES (new.rowid, new.id, new.title, new.artist, new.album, new.genre);
             END",
            [],
        )?;

        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
                DELETE FROM tracks_fts WHERE rowid = old.rowid;
             END",
            [],
        )?;

        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE ON tracks BEGIN
                UPDATE tracks_fts SET 
                    title = new.title,
                    artist = new.artist,
                    album = new.album,
                    genre = new.genre
                WHERE rowid = old.rowid;
             END",
            [],
        )?;
    }

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
