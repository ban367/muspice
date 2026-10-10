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

/// 全文検索の表（`tracks_fts`）の定義の版（`PRAGMA user_version`に記録する）
///
/// - 1: 旧トリガー（直接DELETE/UPDATE方式）で壊れた可能性のあるインデックスを作り直した
/// - 2: 検索の対象にアルバムアーティストを加えた
/// - 3: 途中の一致を探せるよう、トークナイザーをtrigramにした。検索用に正規化した文字列
///   （`search_text`）を1つの列に入れ、表が内容を持つようにした（external contentをやめた）
///
/// 表の列・トークナイザーを変える時は値を上げる（古い版の表は、起動時に作り直す）。
const FTS_SCHEMA_VERSION: i32 = 3;

/// 全文検索の表に入れる文字列（タイトル・アーティスト・アルバム・ジャンル・アルバムアーティストを
/// 検索用に正規化してつなぐ）を求めるSQLの式。`prefix`は`new.`など
fn fts_text_expr(prefix: &str) -> String {
    format!(
        "{}({prefix}title, {prefix}artist, {prefix}album, {prefix}genre, {prefix}album_artist)",
        crate::search_text::SQL_FUNCTION
    )
}

/// データベースマイグレーションを実行
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // 全文検索の表を同期するトリガーが使う関数を登録する（`tracks`に書き込む前に必要）
    crate::search_text::register(conn)?;

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
    // お気に入りにした日時（お気に入りの一覧を、最近お気に入りにした順に並べる）
    add_column_if_not_exists(conn, "tracks", "favorited_at", "TEXT")?;
    // スキップ回数（再生回数に数える前に、別の曲へ移った回数）
    add_column_if_not_exists(conn, "tracks", "skip_count", "INTEGER DEFAULT 0")?;

    // トラック番号/ディスク番号のカラムを追加
    add_column_if_not_exists(conn, "tracks", "track_number", "INTEGER")?;
    add_column_if_not_exists(conn, "tracks", "disc_number", "INTEGER")?;

    // ファイルの更新日時（UNIX時間の秒）。ライブラリフォルダの再スキャンで変更を検出する
    // 追加前に登録したトラックはNULL（再スキャンで記録する）
    add_column_if_not_exists(conn, "tracks", "file_modified_at", "INTEGER")?;

    // 音量の正規化に使うゲイン（dB）とピーク（ReplayGain / EBU R128のタグから読み取る）
    // 追加前に登録したトラックは「メタデータを更新」か再スキャンで読み取る
    add_column_if_not_exists(conn, "tracks", "replay_gain_track_gain", "REAL")?;
    add_column_if_not_exists(conn, "tracks", "replay_gain_track_peak", "REAL")?;
    add_column_if_not_exists(conn, "tracks", "replay_gain_album_gain", "REAL")?;
    add_column_if_not_exists(conn, "tracks", "replay_gain_album_peak", "REAL")?;

    // アルバムアーティスト（ファイルのタグから読む）。アルバム・アーティストの一覧は
    // 「アルバムアーティスト（なければアーティスト）」でまとめる
    add_column_if_not_exists(conn, "tracks", "album_artist", "TEXT")?;
    // アルバムアーティストをファイルから読んだか。列を追加する前に登録したトラックは0で、
    // 起動時にバックグラウンドで読み込む（`tag_backfill`）
    add_column_if_not_exists(
        conn,
        "tracks",
        "album_artist_read",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    // 並び順に使う値（ソート用のタグ。読み仮名など）。値のない項目は、表示用の値で並べる
    add_column_if_not_exists(conn, "tracks", "title_sort", "TEXT")?;
    add_column_if_not_exists(conn, "tracks", "artist_sort", "TEXT")?;
    add_column_if_not_exists(conn, "tracks", "album_sort", "TEXT")?;
    add_column_if_not_exists(conn, "tracks", "album_artist_sort", "TEXT")?;
    // ソート用のタグをファイルから読んだか。列を追加する前に登録したトラックは0で、
    // 起動時にバックグラウンドで読み込む（`tag_backfill`）
    add_column_if_not_exists(
        conn,
        "tracks",
        "sort_tags_read",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    // ファイルが見つからなくなった日時（見つかる間はNULL）。再スキャンでファイルが見つからなく
    // なったトラックはすぐには外さず、「見つからない曲」として残す。移動・改名されたファイルが
    // 見つかれば同じ曲として結び付け、外すのは利用者の操作にする（`track_relink`）
    add_column_if_not_exists(conn, "tracks", "missing_since", "TEXT")?;

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
    // 自動プレイリストの条件（JSON。NULLは、曲を自分で選ぶ通常のプレイリスト）
    add_column_if_not_exists(conn, "playlists", "rules", "TEXT")?;

    // プレイリストのフォルダ（1階層。フォルダの中にフォルダは作らない）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS playlist_folders (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now'))
        )",
        [],
    )?;
    // プレイリストが入っているフォルダ（NULLは、フォルダの外）と、手動の並び順での位置
    // 外部キーは使わず、フォルダを削除する時に`playlist::delete_playlist_folder`が外へ出す
    add_column_if_not_exists(conn, "playlists", "folder_id", "TEXT")?;
    add_column_if_not_exists(conn, "playlists", "position", "INTEGER NOT NULL DEFAULT 0")?;

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

    // ライブラリフォルダ（インポートしたフォルダ。再スキャンの対象）
    // トラックとの対応はfile_pathの前方一致で判定する（フォルダ同士は入れ子にしない）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS library_folders (
            id TEXT PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            added_at TEXT DEFAULT (datetime('now')),
            last_scanned_at TEXT
        )",
        [],
    )?;

    // 転送先デバイス（SDカードなどのフォルダ。同期する対象をデバイスごとに記録する）
    // デバイス上のファイルの一覧は、デバイス側の管理ファイルに記録する（`device_manifest`）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_devices (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            sync_all INTEGER NOT NULL DEFAULT 0,
            remove_unselected INTEGER NOT NULL DEFAULT 1,
            created_at TEXT DEFAULT (datetime('now')),
            last_synced_at TEXT
        )",
        [],
    )?;

    // デバイスに同期するプレイリスト
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_device_playlists (
            device_id TEXT NOT NULL,
            playlist_id TEXT NOT NULL,
            PRIMARY KEY (device_id, playlist_id),
            FOREIGN KEY (device_id) REFERENCES sync_devices(id) ON DELETE CASCADE,
            FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
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

    // 見つからない曲（移動・改名されたファイルの対応付けの候補）の取得用
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_missing ON tracks(missing_since)
         WHERE missing_since IS NOT NULL",
        [],
    )?;

    // アーティストの詳細（アルバムアーティストでの絞り込み）用。式は`repository::ALBUM_ARTIST`と同じにする
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_album_artist
         ON tracks(COALESCE(album_artist, artist))",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_playlist_tracks_playlist_id ON playlist_tracks(playlist_id)",
        [],
    )?;

    // 全文検索の表の定義を変えた場合は、表を作り直す（user_versionで実行済みを管理）
    // 表の内容は、下の「既存データを同期」で`tracks`から作り直せる
    let user_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if user_version < FTS_SCHEMA_VERSION {
        conn.execute_batch("DROP TABLE IF EXISTS tracks_fts")?;
    }

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
        // trigram: 3文字の並びを索引にし、語の途中の一致（区切りのない日本語を含む）を探せる。
        // rowidは`tracks`のrowidと同じにする
        conn.execute(
            "CREATE VIRTUAL TABLE tracks_fts USING fts5(text, tokenize = 'trigram')",
            [],
        )?;

        // 既存データをFTSテーブルに同期
        conn.execute(
            &format!(
                "INSERT INTO tracks_fts(rowid, text) SELECT rowid, {} FROM tracks",
                fts_text_expr("")
            ),
            [],
        )?;
    }

    // 同期トリガーを作成（定義変更を反映できるよう毎回作り直す・冪等）
    //
    // 検索の対象の列を変えた時だけ索引を更新する（再生回数・評価などの更新では更新しない）。
    conn.execute_batch(&format!(
        "DROP TRIGGER IF EXISTS tracks_ai;
         DROP TRIGGER IF EXISTS tracks_ad;
         DROP TRIGGER IF EXISTS tracks_au;

         CREATE TRIGGER tracks_ai AFTER INSERT ON tracks BEGIN
             INSERT INTO tracks_fts(rowid, text) VALUES (new.rowid, {new_text});
         END;

         CREATE TRIGGER tracks_ad AFTER DELETE ON tracks BEGIN
             DELETE FROM tracks_fts WHERE rowid = old.rowid;
         END;

         CREATE TRIGGER tracks_au AFTER UPDATE OF title, artist, album, genre, album_artist
         ON tracks BEGIN
             UPDATE tracks_fts SET text = {new_text} WHERE rowid = old.rowid;
         END;",
        new_text = fts_text_expr("new.")
    ))?;

    if user_version < FTS_SCHEMA_VERSION {
        conn.execute_batch(&format!("PRAGMA user_version = {FTS_SCHEMA_VERSION}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// FTS5インデックスがトラックのINSERT/UPDATE/DELETEに追随することを検証
    #[test]
    fn test_fts_triggers_keep_index_in_sync() {
        let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
        run_migrations(&conn).expect("マイグレーション実行に失敗");

        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, title, artist, album, genre, year, format, file_size, created_at, updated_at)
             VALUES ('t1', '/test/t1.mp3', 't1.mp3', '夜に駆ける', 'YOASOBI', 'THE BOOK', 'JPOP', 2020, 'mp3', 1000, datetime('now'), datetime('now'))",
            [],
        )
        .expect("トラック挿入に失敗");

        let count_match = |query: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM tracks_fts WHERE tracks_fts MATCH ?1",
                [query],
                |row| row.get(0),
            )
            .expect("FTS検索に失敗")
        };

        // INSERT後に検索でヒットする
        assert_eq!(count_match("\"YOASOBI\""), 1);

        // UPDATE後は新しい値でヒットし、古い値ではヒットしない
        conn.execute(
            "UPDATE tracks SET title = 'アイドル', artist = 'NEWARTIST' WHERE id = 't1'",
            [],
        )
        .expect("トラック更新に失敗");
        assert_eq!(count_match("\"NEWARTIST\""), 1);
        assert_eq!(count_match("\"YOASOBI\""), 0);

        // DELETE後はヒットしない
        conn.execute("DELETE FROM tracks WHERE id = 't1'", [])
            .expect("トラック削除に失敗");
        assert_eq!(count_match("\"NEWARTIST\""), 0);

        // FTS5インデックスと実データの整合性チェック
        conn.execute(
            "INSERT INTO tracks_fts(tracks_fts) VALUES('integrity-check')",
            [],
        )
        .expect("FTS5インデックスが破損しています");
    }

    /// アルバムアーティストの列を追加する前のDB（全文検索の表が旧定義）を、起動時に移行できること
    #[test]
    fn test_migrates_database_without_album_artist() {
        let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
        // 旧バージョンが作ったDB: album_artistの列がなく、全文検索の表は4列、user_versionは1
        conn.execute_batch(
            "CREATE TABLE tracks (
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
            );
            CREATE VIRTUAL TABLE tracks_fts USING fts5(
                id UNINDEXED, title, artist, album, genre,
                content=tracks, content_rowid=rowid
            );
            CREATE TRIGGER tracks_ai AFTER INSERT ON tracks BEGIN
                INSERT INTO tracks_fts(rowid, id, title, artist, album, genre)
                VALUES (new.rowid, new.id, new.title, new.artist, new.album, new.genre);
            END;
            INSERT INTO tracks (id, file_path, file_name, title, artist, album, format, file_size)
            VALUES ('t1', '/test/t1.mp3', 't1.mp3', '曲', 'ArtistA', 'Compilation', 'mp3', 1);
            PRAGMA user_version = 1;",
        )
        .expect("旧スキーマの作成に失敗");

        run_migrations(&conn).expect("マイグレーション実行に失敗");

        let count_match = |query: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM tracks_fts WHERE tracks_fts MATCH ?1",
                [query],
                |row| row.get(0),
            )
            .expect("FTS検索に失敗")
        };

        // 既存のトラックは、ソート用のタグも未読の状態になる
        let (artist_sort, sort_tags_read): (Option<String>, bool) = conn
            .query_row(
                "SELECT artist_sort, sort_tags_read FROM tracks WHERE id = 't1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ソート用のタグの列の取得に失敗");
        assert_eq!((artist_sort, sort_tags_read), (None, false));

        // 既存のトラックは、アルバムアーティストが未読の状態になる。これまでの項目は検索できる
        let (album_artist, read): (Option<String>, bool) = conn
            .query_row(
                "SELECT album_artist, album_artist_read FROM tracks WHERE id = 't1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(album_artist, None);
        assert!(!read);
        assert_eq!(count_match("\"ArtistA\""), 1);

        // アルバムアーティストを記録すると、検索できる
        conn.execute(
            "UPDATE tracks SET album_artist = 'VariousArtists' WHERE id = 't1'",
            [],
        )
        .unwrap();
        assert_eq!(count_match("\"VariousArtists\""), 1);
        conn.execute(
            "INSERT INTO tracks_fts(tracks_fts) VALUES('integrity-check')",
            [],
        )
        .expect("FTS5インデックスが破損しています");

        // 2回目の起動では作り直さない（user_versionに記録している）
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, FTS_SCHEMA_VERSION);
        run_migrations(&conn).expect("2回目のマイグレーション実行に失敗");
        assert_eq!(count_match("\"VariousArtists\""), 1);
    }

    /// 条件・フォルダ・位置の列を追加する前のDBのプレイリストは、移行したあとも
    /// フォルダの外にある通常のプレイリストのままになること
    #[test]
    fn test_migrates_playlists_without_rules_and_folders() {
        let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
        // 旧バージョンが作ったDB: playlistsにrules・folder_id・positionの列がない
        conn.execute_batch(
            "CREATE TABLE playlists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );
            INSERT INTO playlists (id, name) VALUES ('p1', '通勤');",
        )
        .expect("旧スキーマの作成に失敗");

        run_migrations(&conn).expect("マイグレーション実行に失敗");
        run_migrations(&conn).expect("2回目のマイグレーション実行に失敗");

        let playlists = crate::playlist::get_all_playlists(&conn).unwrap();
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name, "通勤");
        assert!(playlists[0].rules.is_none());
        assert!(crate::playlist::ensure_manual_playlist(&conn, "p1").is_ok());
        assert_eq!(
            (playlists[0].folder_id.as_deref(), playlists[0].position),
            (None, 0)
        );
        assert!(
            crate::playlist::get_playlist_folders(&conn)
                .unwrap()
                .is_empty()
        );

        // 移行したあとに作ったプレイリストは、手動の並び順でいちばん後ろになる
        let created = crate::playlist::create_playlist(&conn, "ドライブ").unwrap();
        assert_eq!(created.position, 1);
    }

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
        assert!(tables.contains(&"library_folders".to_string()));
        assert!(tables.contains(&"sync_devices".to_string()));
        assert!(tables.contains(&"sync_device_playlists".to_string()));

        // テスト後にクリーンアップ
        drop(conn);
        fs::remove_file(test_db_path).ok();
    }
}
