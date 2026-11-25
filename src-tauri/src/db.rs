use rusqlite::{Connection, OptionalExtension, Result};
use std::path::PathBuf;

/// DBの表の作りの版（`app_meta`の`schema_version`に記録する。ADR-088）
///
/// 前の版のアプリが、このDBを正しく扱えなくなる変更をした時に上げる（列・表を足すだけで、前の版が
/// そのまま読み書きできる変更では上げない）。記録が新しい版のDBは開かない（`DbOpenError::NewerSchema`）。
/// 全文検索の表の版（`FTS_SCHEMA_VERSION`。`PRAGMA user_version`）とは別に持つ。
///
/// リリースの前は移行を持たず、表の作りを変えたら、表を最終の形のまま書き直して版を上げる（ADR-129）。
/// 記録が古い版のDB（リリースの前の版のアプリが作ったDB）も開かない（`DbOpenError::OlderSchema`）。
///
/// - 3: リリースの前の移行をやめ、表を最終の形で作り直した（ADR-129）
pub const DB_SCHEMA_VERSION: i64 = 3;

/// DBの情報を持つ表（キーと値）
const APP_META_TABLE: &str = "app_meta";

/// DBを開けなかった理由
#[derive(Debug, thiserror::Error)]
pub enum DbOpenError {
    /// 新しい版のアプリが作ったDB（この版では扱えない。DBには触らない）
    #[error(
        "the database was written by a newer version of the app (schema version {found}, supported up to {supported})"
    )]
    NewerSchema { found: i64, supported: i64 },
    /// リリースの前の版のアプリが作ったDB（移行はしない。起動時は別の名前で残す）。版の記録が
    /// ないDBは`found`がNone
    #[error(
        "the database was written by a pre-release version of the app ({})",
        schema_version_label(*.found)
    )]
    OlderSchema { found: Option<i64> },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// ログに書く、DBの表の作りの版（記録がなければ、そう書く）
fn schema_version_label(found: Option<i64>) -> String {
    match found {
        Some(version) => format!("schema version {version}"),
        None => "no schema version".to_string(),
    }
}

/// データベース接続を初期化
///
/// 表の作りの版が今の版と違うDBは、表を作る前に断る（何も書き換えない）。表のないDB（新しく作った
/// ファイル）には、表を作る。
pub fn init_db(db_path: PathBuf) -> std::result::Result<Connection, DbOpenError> {
    let conn = Connection::open(db_path)?;
    match stored_schema_version(&conn)? {
        Some(DB_SCHEMA_VERSION) => {}
        Some(found) if found > DB_SCHEMA_VERSION => {
            return Err(DbOpenError::NewerSchema {
                found,
                supported: DB_SCHEMA_VERSION,
            });
        }
        found if has_table(&conn, "tracks")? => return Err(DbOpenError::OlderSchema { found }),
        _ => {}
    }
    setup_connection(&conn)?;
    Ok(conn)
}

/// 表（全文検索の仮想表を含む）があるか
fn has_table(conn: &Connection, name: &str) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |row| row.get(0),
    )
}

/// DBに記録してある表の作りの版（記録のないDBはNone）
///
/// バックアップのファイルを調べる時にも使う（版が今の版と違うDBは復元しない）。
pub fn stored_schema_version(conn: &Connection) -> Result<Option<i64>> {
    if !has_table(conn, APP_META_TABLE)? {
        return Ok(None);
    }
    let value: Option<String> = conn
        .query_row(
            &format!("SELECT value FROM {APP_META_TABLE} WHERE key = 'schema_version'"),
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value.and_then(|value| value.parse().ok()))
}

/// 開いた接続を使える状態にする（接続ごとの設定と、表の作成）
///
/// アプリの接続も、テストの接続（`open_in_memory`）も、これを通す。
pub fn setup_connection(conn: &Connection) -> Result<()> {
    // 外部キー（`ON DELETE CASCADE`）を有効にする。SQLiteの既定は無効で、接続ごとに設定する
    // （同梱のSQLiteのビルドの既定値に頼らない）。トランザクションの中では変えられないため、最初に行う
    conn.pragma_update(None, "foreign_keys", true)?;
    // 全文検索の表を同期するトリガーが使う関数（`tracks`に書き込む前に必要）と、複数の値を持つ項目
    // （アーティスト・ジャンル）を値ごとに比べる関数（一覧の絞り込みで使う）を登録する
    crate::search_text::register(conn)?;
    crate::multi_value::register(conn)?;

    conn.execute_batch(&schema_sql())?;
    setup_full_text_search(conn)?;
    conn.execute(
        &format!(
            "INSERT INTO {APP_META_TABLE} (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value"
        ),
        [DB_SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

/// テスト用の、メモリ上のDBを開く（アプリと同じ設定・スキーマにする）
#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
    setup_connection(&conn).expect("スキーマの作成に失敗");
    conn
}

/// 表と索引を作るSQL（すでにあれば何もしない）
///
/// アプリが必ず値を入れる列には`NOT NULL`を付ける（読む側でNULLを補わない）。
fn schema_sql() -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {APP_META_TABLE} (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );

        -- 曲。タグにある項目と、ファイルの情報と、アプリだけが持つ項目（お気に入り・再生回数など）
        CREATE TABLE IF NOT EXISTS tracks (
            id TEXT PRIMARY KEY NOT NULL,
            file_path TEXT UNIQUE NOT NULL,
            file_name TEXT NOT NULL,
            title TEXT,
            artist TEXT,
            album TEXT,
            -- アルバム・アーティストの一覧は「アルバムアーティスト（なければアーティスト）」でまとめる
            album_artist TEXT,
            genre TEXT,
            year INTEGER,
            track_number INTEGER,
            disc_number INTEGER,
            -- コンピレーションの印。アルバムアーティストのないコンピレーションの曲は、
            -- 「Various Artists」のアルバムにまとめる（ADR-093）
            compilation INTEGER NOT NULL DEFAULT 0,
            -- 作曲者・グループ・コメント・BPM（一覧の列・検索に使う。ADR-115）
            composer TEXT,
            grouping TEXT,
            comment TEXT,
            bpm INTEGER,
            -- 並び順に使う値（ソート用のタグ。読み仮名など）。値のない項目は、表示用の値で並べる
            title_sort TEXT,
            artist_sort TEXT,
            album_sort TEXT,
            album_artist_sort TEXT,
            -- 評価（半分の星の単位。0〜10。ADR-082）
            rating INTEGER NOT NULL DEFAULT 0,
            -- 音量の正規化に使うゲイン（dB）とピーク（ReplayGain / EBU R128のタグから読む）
            replay_gain_track_gain REAL,
            replay_gain_track_peak REAL,
            replay_gain_album_gain REAL,
            replay_gain_album_peak REAL,
            duration INTEGER,
            file_size INTEGER NOT NULL DEFAULT 0,
            format TEXT NOT NULL DEFAULT '',
            bitrate INTEGER,
            sample_rate INTEGER,
            bit_depth INTEGER,
            channels INTEGER,
            -- ファイルの更新日時（UNIX時間の秒）。ライブラリフォルダの再スキャンで変更を検出する
            file_modified_at INTEGER,
            -- ファイルが見つからなくなった日時（見つかる間はNULL。`track_relink`）
            missing_since TEXT,
            -- CUEシートの曲（アルバム1ファイルの中の1曲）の、元の音声ファイルと範囲（1/75秒。ADR-123）
            cue_source TEXT,
            cue_start INTEGER,
            cue_end INTEGER,
            is_favorite INTEGER NOT NULL DEFAULT 0,
            -- お気に入りにした日時（お気に入りの一覧を、最近お気に入りにした順に並べる）
            favorited_at TEXT,
            play_count INTEGER NOT NULL DEFAULT 0,
            -- スキップ回数（再生回数に数える前に、別の曲へ移った回数）
            skip_count INTEGER NOT NULL DEFAULT 0,
            last_played_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS play_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            track_id TEXT NOT NULL,
            played_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
        );

        -- プレイリストのフォルダ（1階層。フォルダの中にフォルダは作らない）
        CREATE TABLE IF NOT EXISTS playlist_folders (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- `folder_id`は、入っているフォルダ（NULLは、フォルダの外）。`position`は手動の並び順での位置。
        -- 外部キーは使わず、フォルダを削除する時に`playlist::delete_playlist_folder`が外へ出す
        CREATE TABLE IF NOT EXISTS playlists (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            folder_id TEXT,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS playlist_tracks (
            playlist_id TEXT NOT NULL,
            track_id TEXT NOT NULL,
            position INTEGER NOT NULL,
            added_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (playlist_id, track_id),
            FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
            FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
        );

        -- オートプレイリスト（条件で曲を集めるプレイリスト。ADR-042・ADR-127）。曲の一覧は持たず、
        -- 開くたびに条件（`rules`。JSON）から求める。フォルダには入れない
        CREATE TABLE IF NOT EXISTS smart_playlists (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            rules TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- ライブラリフォルダ（インポートしたフォルダ。再スキャンの対象）。トラックとの対応は
        -- file_pathの前方一致で判定する（フォルダ同士は入れ子にしない）。`is_music_folder`は、
        -- 音楽フォルダ（新しい曲を置く宛先）の印で、付けるフォルダは多くても1つ（ADR-068）
        CREATE TABLE IF NOT EXISTS library_folders (
            id TEXT PRIMARY KEY NOT NULL,
            path TEXT UNIQUE NOT NULL,
            is_music_folder INTEGER NOT NULL DEFAULT 0,
            added_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_scanned_at TEXT
        );

        -- ライブラリフォルダの、読み込まないフォルダ（除外。pathはライブラリフォルダからの相対パス）。
        -- 隠しフォルダ・ゴミ箱・パッケージは、ここに入れなくても読み飛ばす（`library::is_always_skipped`）
        CREATE TABLE IF NOT EXISTS library_folder_exclusions (
            id TEXT PRIMARY KEY NOT NULL,
            folder_id TEXT NOT NULL,
            path TEXT NOT NULL,
            added_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE (folder_id, path),
            FOREIGN KEY (folder_id) REFERENCES library_folders(id) ON DELETE CASCADE
        );

        -- 転送先デバイス（SDカードなどのフォルダ。同期する対象をデバイスごとに記録する）。
        -- デバイス上のファイルの一覧は、デバイス側の管理ファイルに記録する（`device_manifest`）。
        -- 列の既定値は、新しく登録するデバイスの設定（配置の書式はADR-111、歌詞ファイル・フォルダの
        -- 画像のコピーはADR-112、転送する時の変換はADR-118）
        CREATE TABLE IF NOT EXISTS sync_devices (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            sync_all INTEGER NOT NULL DEFAULT 0,
            remove_unselected INTEGER NOT NULL DEFAULT 1,
            path_template TEXT NOT NULL DEFAULT '{device_template}',
            copy_extras INTEGER NOT NULL DEFAULT 1,
            transcode_mode TEXT NOT NULL DEFAULT 'off',
            transcode_bitrate INTEGER NOT NULL DEFAULT {transcode_bitrate},
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_synced_at TEXT
        );

        CREATE TABLE IF NOT EXISTS sync_device_playlists (
            device_id TEXT NOT NULL,
            playlist_id TEXT NOT NULL,
            PRIMARY KEY (device_id, playlist_id),
            FOREIGN KEY (device_id) REFERENCES sync_devices(id) ON DELETE CASCADE,
            FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS sync_device_smart_playlists (
            device_id TEXT NOT NULL,
            smart_playlist_id TEXT NOT NULL,
            PRIMARY KEY (device_id, smart_playlist_id),
            FOREIGN KEY (device_id) REFERENCES sync_devices(id) ON DELETE CASCADE,
            FOREIGN KEY (smart_playlist_id) REFERENCES smart_playlists(id) ON DELETE CASCADE
        );

        -- 曲の解析（テンポ・キー・ペース）の結果（`music_analysis`。ADR-104）。解析した時の曲の長さを持ち、
        -- 長さが変わった曲（ファイルを差し替えた）は解析し直す。解析できなかった曲は`failed`を1にする
        -- （値のない行。解析し直さない）。キーは、主音のピッチクラス（0〜11）と長調（0）・短調（1）
        CREATE TABLE IF NOT EXISTS track_analysis (
            track_id TEXT PRIMARY KEY NOT NULL,
            duration INTEGER,
            bpm REAL,
            key_pitch_class INTEGER,
            key_mode INTEGER,
            pace REAL,
            failed INTEGER NOT NULL DEFAULT 0,
            analyzed_at TEXT NOT NULL,
            FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
        CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
        CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre);
        CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
        -- 見つからない曲（移動・改名されたファイルの対応付けの候補）の取得用
        CREATE INDEX IF NOT EXISTS idx_tracks_missing ON tracks(missing_since)
            WHERE missing_since IS NOT NULL;
        -- アルバムの曲・アルバムアーティストのアルバムの取得（まとめるアーティストでの絞り込み）用。
        -- 式は`repository::ALBUM_ARTIST`と同じにする（違うと使われない）
        CREATE INDEX IF NOT EXISTS idx_tracks_group_artist ON tracks({album_artist});
        CREATE INDEX IF NOT EXISTS idx_play_history_track_id ON play_history(track_id);
        CREATE INDEX IF NOT EXISTS idx_play_history_played_at ON play_history(played_at);
        CREATE INDEX IF NOT EXISTS idx_playlist_tracks_playlist_id ON playlist_tracks(playlist_id);
        -- 曲を削除する時に、外部キー（`ON DELETE CASCADE`）がその曲の行を探す
        -- （主キーの`(playlist_id, track_id)`は、`track_id`だけの検索に使えない）
        CREATE INDEX IF NOT EXISTS idx_playlist_tracks_track_id ON playlist_tracks(track_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_library_folders_music_folder
            ON library_folders(is_music_folder) WHERE is_music_folder = 1;",
        device_template = crate::device::DEFAULT_DEVICE_TEMPLATE,
        transcode_bitrate = crate::transcode::DEFAULT_AAC_BITRATE_KBPS,
        album_artist = crate::repository::ALBUM_ARTIST,
    )
}

/// 全文検索の表（`tracks_fts`）の定義の版（`PRAGMA user_version`に記録する）
///
/// 表の列・トークナイザーを変える時は値を上げる（古い版の表は、起動時に`tracks`から作り直す）。
const FTS_SCHEMA_VERSION: i32 = 1;

/// 全文検索の表に入れる文字列（タイトル・アーティスト・アルバム・ジャンル・アルバムアーティスト・
/// 作曲者を検索用に正規化してつなぐ）を求めるSQLの式。`prefix`は`new.`など
fn fts_text_expr(prefix: &str) -> String {
    format!(
        "{}({prefix}title, {prefix}artist, {prefix}album, {prefix}genre, {prefix}album_artist, \
         {prefix}composer)",
        crate::search_text::SQL_FUNCTION
    )
}

/// 全文検索の表と、`tracks`に合わせて表を更新するトリガーを用意する
///
/// trigram（3文字の並びを索引にする）で、語の途中の一致（区切りのない日本語を含む）を探せる。
/// 検索用に正規化した文字列（`search_text`）を1つの列に入れ、rowidは`tracks`のrowidと同じにする。
/// 表の定義の版が古い表と、作り直している途中で終了した表は、`tracks`から作り直す。
fn setup_full_text_search(conn: &Connection) -> Result<()> {
    let user_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if user_version < FTS_SCHEMA_VERSION {
        conn.execute_batch("DROP TABLE IF EXISTS tracks_fts")?;
    }
    if !has_table(conn, "tracks_fts")? {
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE tracks_fts USING fts5(text, tokenize = 'trigram');
             INSERT INTO tracks_fts(rowid, text) SELECT rowid, {} FROM tracks;",
            fts_text_expr("")
        ))?;
    }

    // 定義を変えても反映されるよう、毎回作り直す。検索の対象の列を変えた時だけ索引を更新する
    // （再生回数・評価などの更新では更新しない）
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

         CREATE TRIGGER tracks_au AFTER UPDATE OF title, artist, album, genre, album_artist, composer
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

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |row| row.get(0)).unwrap()
    }

    /// 接続の設定で外部キーが有効になり、曲・プレイリストを削除すると関連する行も消えること
    #[test]
    fn test_setup_connection_enables_foreign_keys() {
        let conn = Connection::open_in_memory().unwrap();
        // ビルドの既定値によらず、接続の設定で有効にする
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        setup_connection(&conn).unwrap();
        let enabled: bool = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert!(enabled);

        conn.execute_batch(
            "INSERT INTO tracks (id, file_path, file_name)
             VALUES ('t1', '/m/t1.mp3', 't1.mp3'), ('t2', '/m/t2.mp3', 't2.mp3');
             INSERT INTO playlists (id, name) VALUES ('p1', '通勤');
             INSERT INTO playlist_tracks (playlist_id, track_id, position)
             VALUES ('p1', 't1', 0), ('p1', 't2', 1);
             INSERT INTO play_history (track_id) VALUES ('t1'), ('t2');
             INSERT INTO sync_devices (id, name, path) VALUES ('d1', 'SD', '/Volumes/SD');
             INSERT INTO sync_device_playlists (device_id, playlist_id) VALUES ('d1', 'p1');",
        )
        .unwrap();
        // ない曲は、プレイリストに入れられない
        assert!(
            conn.execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position)
                 VALUES ('p1', 'missing', 2)",
                [],
            )
            .is_err()
        );

        conn.execute("DELETE FROM tracks WHERE id = 't1'", [])
            .unwrap();
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM playlist_tracks WHERE track_id = 't1'"
            ),
            0
        );
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM play_history WHERE track_id = 't1'"
            ),
            0
        );

        conn.execute("DELETE FROM playlists WHERE id = 'p1'", [])
            .unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM playlist_tracks"), 0);
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM sync_device_playlists"),
            0
        );
        // 曲・デバイスは残る
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM play_history"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sync_devices"), 1);
    }

    /// 曲の削除で、プレイリストの行を曲のIDの索引で探すこと
    #[test]
    fn test_playlist_tracks_has_an_index_on_track_id() {
        let conn = open_in_memory();
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT 1 FROM playlist_tracks WHERE track_id = 't1'",
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("idx_playlist_tracks_track_id"),
            "索引を使っていない: {plan}"
        );
    }

    /// アルバムの曲の取得で、まとめるアーティストの式の索引を使うこと（式が`repository::ALBUM_ARTIST`と同じ）
    #[test]
    fn test_tracks_have_an_index_on_the_grouping_artist() {
        let conn = open_in_memory();
        let plan: String = conn
            .query_row(
                &format!(
                    "EXPLAIN QUERY PLAN SELECT 1 FROM tracks WHERE {} IS 'Various Artists'",
                    crate::repository::ALBUM_ARTIST
                ),
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("idx_tracks_group_artist"),
            "索引を使っていない: {plan}"
        );
    }

    /// FTS5インデックスがトラックのINSERT/UPDATE/DELETEに追随することを検証
    #[test]
    fn test_fts_triggers_keep_index_in_sync() {
        let conn = open_in_memory();

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

    /// 全文検索の表を作り直している途中で落ちたDB（表を消した後・作った直後）も、次の起動で作り直す
    #[test]
    fn test_recovers_from_an_interrupted_fts_rebuild() {
        let count_match = |conn: &Connection, query: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM tracks_fts WHERE tracks_fts MATCH ?1",
                [query],
                |row| row.get(0),
            )
            .expect("FTS検索に失敗")
        };
        let interrupted = [
            // 表を消した後（トリガーは、消した表を指したまま残る）
            "DROP TABLE tracks_fts;",
            // 表を作った後、中身を入れる前
            "DROP TABLE tracks_fts;
             CREATE VIRTUAL TABLE tracks_fts USING fts5(text, tokenize = 'trigram');",
        ];
        for state in interrupted {
            let conn = open_in_memory();
            conn.execute(
                "INSERT INTO tracks (id, file_path, file_name, title, artist, album, format, file_size)
                 VALUES ('t1', '/test/t1.mp3', 't1.mp3', '夜明けの歌', 'ArtistA', 'Album', 'mp3', 1)",
                [],
            )
            .unwrap();
            conn.execute_batch(&format!(
                "{state} PRAGMA user_version = {};",
                FTS_SCHEMA_VERSION - 1
            ))
            .unwrap();

            setup_connection(&conn).expect("作り直しの続きに失敗");

            assert_eq!(count_match(&conn, "\"夜明け\""), 1, "{state}");
            conn.execute(
                "INSERT INTO tracks_fts(tracks_fts) VALUES('integrity-check')",
                [],
            )
            .expect("FTS5インデックスが破損しています");
            let version: i32 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap();
            assert_eq!(version, FTS_SCHEMA_VERSION);
        }
    }

    /// 表の作りの版を記録し、新しい版のアプリが作ったDBは、何も書き換えずに断る
    #[test]
    fn test_newer_schema_is_refused_without_changes() {
        let dir = crate::device_manifest::test_support::TempDir::new();
        let path = dir.path().join("muspice.db");

        // 初めて開いたDBには、今の版を記録する
        let conn = init_db(path.clone()).unwrap();
        assert_eq!(
            stored_schema_version(&conn).unwrap(),
            Some(DB_SCHEMA_VERSION)
        );
        // 新しい版のアプリが、版を上げて、この版の知らない形にした（ここでは、表を1つなくす）
        conn.execute_batch(&format!(
            "UPDATE app_meta SET value = '{}' WHERE key = 'schema_version';
             DROP TABLE play_history;",
            DB_SCHEMA_VERSION + 1
        ))
        .unwrap();
        drop(conn);

        let error = init_db(path.clone()).unwrap_err();
        assert!(
            matches!(error, DbOpenError::NewerSchema { found, supported }
                if found == DB_SCHEMA_VERSION + 1 && supported == DB_SCHEMA_VERSION),
            "{error:?}"
        );
        // 表を作り直していない
        let conn = Connection::open(&path).unwrap();
        let has_history: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'play_history')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!has_history);
    }

    /// リリースの前の版のアプリが作ったDB（版の記録がない・古い版）は、何も書き換えずに断る
    #[test]
    fn test_older_schema_is_refused_without_changes() {
        let dir = crate::device_manifest::test_support::TempDir::new();
        let path = dir.path().join("muspice.db");
        let conn = init_db(path.clone()).unwrap();
        conn.execute_batch(
            "UPDATE app_meta SET value = '2' WHERE key = 'schema_version';
             DROP TABLE play_history;",
        )
        .unwrap();
        drop(conn);

        let error = init_db(path.clone()).unwrap_err();
        assert!(
            matches!(error, DbOpenError::OlderSchema { found: Some(2) }),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "the database was written by a pre-release version of the app (schema version 2)"
        );
        let conn = Connection::open(&path).unwrap();
        assert!(!has_table(&conn, "play_history").unwrap());

        // 版を記録する前の版のアプリが作ったDB（版の表がない）
        conn.execute_batch("DROP TABLE app_meta").unwrap();
        drop(conn);
        let error = init_db(path).unwrap_err();
        assert!(
            matches!(error, DbOpenError::OlderSchema { found: None }),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "the database was written by a pre-release version of the app (no schema version)"
        );
    }

    /// アプリが必ず値を入れる列には、NULLを入れられない（読む側でNULLを補わない）
    #[test]
    fn test_required_columns_reject_null() {
        let conn = open_in_memory();
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name) VALUES ('t1', '/m/t1.mp3', 't1.mp3')",
            [],
        )
        .unwrap();
        for column in [
            "file_size",
            "format",
            "is_favorite",
            "rating",
            "play_count",
            "skip_count",
            "compilation",
            "created_at",
            "updated_at",
        ] {
            assert!(
                conn.execute(&format!("UPDATE tracks SET {column} = NULL"), [])
                    .is_err(),
                "{column}"
            );
        }
        conn.execute("INSERT INTO playlists (id, name) VALUES ('p1', 'p')", [])
            .unwrap();
        assert!(
            conn.execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES ('p1', NULL, 0)",
                [],
            )
            .is_err()
        );
    }

    /// ファイルのDBを作ると、すべての表ができる（一時フォルダに作る。作業フォルダにファイルを残さない）。
    /// 作ったDBは、次の起動でもそのまま開ける
    #[test]
    fn test_init_db() {
        let dir = crate::device_manifest::test_support::TempDir::new();
        let test_db_path = dir.path().join("test_music.db");

        let conn = init_db(test_db_path.clone()).expect("DBを作れること");
        assert!(test_db_path.exists());

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'view') ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for table in [
            "tracks",
            "playlists",
            "playlist_folders",
            "playlist_tracks",
            "play_history",
            "library_folders",
            "library_folder_exclusions",
            "sync_devices",
            "sync_device_playlists",
            "smart_playlists",
            "sync_device_smart_playlists",
            "track_analysis",
            "app_meta",
            "tracks_fts",
        ] {
            assert!(
                tables.iter().any(|name| name == table),
                "{table}: {tables:?}"
            );
        }

        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, title) VALUES ('t1', '/m/t1.mp3', 't1.mp3', '朝の歌')",
            [],
        )
        .unwrap();
        drop(conn);
        let conn = init_db(test_db_path).expect("作ったDBを開き直せること");
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tracks_fts WHERE tracks_fts MATCH '\"朝の歌\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1);
    }
}
