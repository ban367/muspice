use rusqlite::{Connection, OptionalExtension, Result};
use std::path::PathBuf;

/// DBの表の作りの版（`app_meta`の`schema_version`に記録する。ADR-088）
///
/// 前の版のアプリが、このDBを正しく扱えなくなる変更をした時に上げる（列・表を足すだけで、前の版が
/// そのまま読み書きできる変更では上げない）。記録が新しい版のDBは開かない（`DbOpenError::NewerSchema`）。
/// 全文検索の表の版（`FTS_SCHEMA_VERSION`。`PRAGMA user_version`）とは別に持つ。
pub const DB_SCHEMA_VERSION: i64 = 1;

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
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// データベース接続を初期化
///
/// 新しい版のアプリが作ったDBは、移行を始める前に断る（何も書き換えない）。
pub fn init_db(db_path: PathBuf) -> std::result::Result<Connection, DbOpenError> {
    let conn = Connection::open(db_path)?;
    if let Some(found) = stored_schema_version(&conn)?
        && found > DB_SCHEMA_VERSION
    {
        return Err(DbOpenError::NewerSchema {
            found,
            supported: DB_SCHEMA_VERSION,
        });
    }
    setup_connection(&conn)?;
    Ok(conn)
}

/// DBに記録してある表の作りの版（記録する前の版のアプリが作ったDB・ほかのDBはNone）
///
/// バックアップのファイルを調べる時にも使う（新しい版のDBは復元しない）。
pub fn stored_schema_version(conn: &Connection) -> Result<Option<i64>> {
    let has_table: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [APP_META_TABLE],
        |row| row.get(0),
    )?;
    if !has_table {
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

/// 表の作りの版を記録する（移行の最後に呼ぶ）
fn record_schema_version(conn: &Connection) -> Result<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {APP_META_TABLE} (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO {APP_META_TABLE} (key, value) VALUES ('schema_version', '{DB_SCHEMA_VERSION}')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value;"
    ))
}

/// 開いた接続を使える状態にする（接続ごとの設定と、スキーマの作成・移行）
///
/// アプリの接続も、テストの接続（`open_in_memory`）も、これを通す。
pub fn setup_connection(conn: &Connection) -> Result<()> {
    // 外部キー（`ON DELETE CASCADE`）を有効にする。SQLiteの既定は無効で、接続ごとに設定する
    // （同梱のSQLiteのビルドの既定値に頼らない）。トランザクションの中では変えられないため、最初に行う
    conn.pragma_update(None, "foreign_keys", true)?;
    run_migrations(conn)
}

/// テスト用の、メモリ上のDBを開く（アプリと同じ設定・スキーマにする）
#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
    setup_connection(&conn).expect("マイグレーション実行に失敗");
    conn
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

/// データベースマイグレーションを実行（`setup_connection`から呼ぶ）
fn run_migrations(conn: &Connection) -> Result<()> {
    // 全文検索の表を同期するトリガーが使う関数を登録する（`tracks`に書き込む前に必要）
    crate::search_text::register(conn)?;
    // 複数の値を持つ項目（アーティスト・ジャンル）を、値ごとに比べる関数（一覧の絞り込みで使う）
    crate::multi_value::register(conn)?;

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
    // 再生回数・スキップ回数を、読める範囲（`MAX_PLAY_COUNT`まで）に収める。上限を確かめずに
    // 書いていた版（ライブラリのXMLの取り込み）が入れた値があると、その曲を含む一覧を読めない
    conn.execute(
        "UPDATE tracks SET
            play_count = MIN(play_count, ?1),
            skip_count = MIN(skip_count, ?1)
         WHERE play_count > ?1 OR skip_count > ?1",
        [crate::models::MAX_PLAY_COUNT],
    )?;

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

    // アーティスト・ジャンルを、複数の値として読んだか。複数の値に対応する前に登録したトラックは0で
    // （同じ項目が複数あるタグの、最初の値だけを読んでいる）、起動時にバックグラウンドで読み直す
    // （`tag_backfill`）
    add_column_if_not_exists(
        conn,
        "tracks",
        "multi_values_read",
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
    // 主キーの列に`NOT NULL`がない（SQLiteのrowidの表は、主キーにNULLを許す）。アプリはNULLを
    // 入れないため今は付けない。表を作り直す時に付ける
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
    // 音楽フォルダ（新しい曲を置く宛先）の印。印を付けるフォルダは、多くても1つ（ADR-068）
    add_column_if_not_exists(
        conn,
        "library_folders",
        "is_music_folder",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_library_folders_music_folder
         ON library_folders(is_music_folder) WHERE is_music_folder = 1",
        [],
    )?;

    // ライブラリフォルダの、読み込まないフォルダ（除外。pathはライブラリフォルダからの相対パス）
    // 隠しフォルダ・ゴミ箱・パッケージは、ここに入れなくても読み飛ばす（`library::is_always_skipped`）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS library_folder_exclusions (
            id TEXT PRIMARY KEY,
            folder_id TEXT NOT NULL,
            path TEXT NOT NULL,
            added_at TEXT DEFAULT (datetime('now')),
            UNIQUE (folder_id, path),
            FOREIGN KEY (folder_id) REFERENCES library_folders(id) ON DELETE CASCADE
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

    // 曲を削除する時に、外部キー（`ON DELETE CASCADE`）がその曲の行を探す
    // （主キーの`(playlist_id, track_id)`は、`track_id`だけの検索に使えない）
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_playlist_tracks_track_id ON playlist_tracks(track_id)",
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

    rename_formats(conn)?;
    migrate_to_half_star_ratings(conn)?;
    record_schema_version(conn)?;

    Ok(())
}

/// カラムがあるか
fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let found = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .any(|name| name.map(|n| n == column).unwrap_or(false));
    Ok(found)
}

/// 評価を、半分の星の単位（0〜10）にする（ADR-082）
///
/// 以前の版は評価を星の数（0〜5）で持っていた。`tracks.rating_read`がないDB（以前の版のDB・
/// 以前の版のバックアップから復元したDB）だけ、曲の評価と、自動プレイリストの評価の条件の値を
/// 2倍にし、`rating_read`を足す（1回だけ行う）。足した時の`rating_read`は0で、起動時に
/// バックグラウンドでファイルの評価を読み直し、半分の星を読む（`tag_backfill`）。
fn migrate_to_half_star_ratings(conn: &Connection) -> Result<()> {
    if has_column(conn, "tracks", "rating_read")? {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE tracks SET rating = MIN(rating, 5) * 2 WHERE rating > 0",
        [],
    )?;

    let playlists: Vec<(String, String)> = tx
        .prepare("SELECT id, rules FROM playlists WHERE rules LIKE '%\"rating\"%'")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_>>()?;
    for (id, rules) in playlists {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&rules) else {
            continue;
        };
        if double_rating_rules(&mut value) {
            tx.execute(
                "UPDATE playlists SET rules = ?1 WHERE id = ?2",
                [value.to_string(), id],
            )?;
        }
    }

    // 評価を半分の星として読んだか。足す前に登録した曲は0（`tag_backfill`が読み直す）
    tx.execute(
        "ALTER TABLE tracks ADD COLUMN rating_read INTEGER NOT NULL DEFAULT 0",
        [],
    )?;
    tx.commit()
}

/// 自動プレイリストの条件のうち、評価の数値の条件の値（`value`・`valueTo`）を2倍にする
/// @returns 直したか
fn double_rating_rules(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= double_rating_rules(item);
            }
            changed
        }
        serde_json::Value::Object(fields) => {
            let is_rating_rule = fields.get("kind").and_then(|kind| kind.as_str())
                == Some("number")
                && fields.get("field").and_then(|field| field.as_str()) == Some("rating");
            if is_rating_rule {
                for key in ["value", "valueTo"] {
                    if let Some(number) = fields.get(key).and_then(serde_json::Value::as_i64) {
                        fields.insert(key.to_string(), (number.clamp(0, 5) * 2).into());
                    }
                }
                return true;
            }
            let mut changed = false;
            for item in fields.values_mut() {
                changed |= double_rating_rules(item);
            }
            changed
        }
        _ => false,
    }
}

/// 以前の版が記録した形式の名前（`AIF`・`WV`）を、今の名前（`library::FORMAT_RENAMES`）に直す
///
/// 曲の形式と、自動プレイリストの「形式」の条件の値を直す（「形式がWV」の条件が、`WavPack`の曲に
/// 合うように）。直すものがなければ、何も書かない。
fn rename_formats(conn: &Connection) -> Result<()> {
    for (from, to) in crate::library::FORMAT_RENAMES {
        conn.execute(
            "UPDATE tracks SET format = ?2 WHERE format = ?1",
            [from, to],
        )?;
    }

    let playlists: Vec<(String, String)> = conn
        .prepare("SELECT id, rules FROM playlists WHERE rules LIKE '%\"format\"%'")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_>>()?;
    for (id, rules) in playlists {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&rules) else {
            continue;
        };
        if rename_format_rules(&mut value) {
            conn.execute(
                "UPDATE playlists SET rules = ?1 WHERE id = ?2",
                [value.to_string(), id],
            )?;
        }
    }
    Ok(())
}

/// 自動プレイリストの条件のうち、「形式」の文字列の条件の値を、今の名前にする
/// @returns 直したか
fn rename_format_rules(value: &mut serde_json::Value) -> bool {
    match value {
        // 途中で止めずに、すべての条件を直す（`any`は、最初に直した所で止まる）
        serde_json::Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= rename_format_rules(item);
            }
            changed
        }
        serde_json::Value::Object(fields) => {
            let is_format_rule = fields.get("kind").and_then(|kind| kind.as_str()) == Some("text")
                && fields.get("field").and_then(|field| field.as_str()) == Some("format");
            if is_format_rule
                && let Some(serde_json::Value::String(text)) = fields.get_mut("value")
                && let Some((_, to)) = crate::library::FORMAT_RENAMES
                    .iter()
                    .find(|(from, _)| from.eq_ignore_ascii_case(text.trim()))
            {
                *text = (*to).to_string();
                return true;
            }
            let mut changed = false;
            for item in fields.values_mut() {
                changed |= rename_format_rules(item);
            }
            changed
        }
        _ => false,
    }
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

        setup_connection(&conn).expect("マイグレーション実行に失敗");

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
        setup_connection(&conn).expect("2回目のマイグレーション実行に失敗");
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

        setup_connection(&conn).expect("マイグレーション実行に失敗");
        setup_connection(&conn).expect("2回目のマイグレーション実行に失敗");

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

    /// 音楽フォルダの印の列がない（追加する前の版の）DBも、移行して読めるようにすること
    #[test]
    fn test_migrates_library_folders_without_music_folder_flag() {
        let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
        conn.execute_batch(
            "CREATE TABLE library_folders (
                id TEXT PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                added_at TEXT DEFAULT (datetime('now')),
                last_scanned_at TEXT
            );
            INSERT INTO library_folders (id, path) VALUES ('f1', '/music');",
        )
        .expect("旧スキーマの作成に失敗");

        setup_connection(&conn).expect("マイグレーション実行に失敗");
        setup_connection(&conn).expect("2回目のマイグレーション実行に失敗");

        let folders = crate::library_folder::find_all_folders(&conn).unwrap();
        assert_eq!(folders.len(), 1);
        assert!(!folders[0].is_music_folder);
        crate::library_folder::set_music_folder(&conn, "f1").unwrap();
        assert_eq!(
            crate::library_folder::find_music_folder(&conn)
                .unwrap()
                .map(|folder| folder.id),
            Some("f1".to_string())
        );
    }

    /// 以前の版が記録した形式の名前（`AIF`・`WV`）と、自動プレイリストの形式の条件を、起動時に直す
    #[test]
    fn test_migration_renames_formats_and_format_rules() {
        let conn = open_in_memory();
        conn.execute_batch(
            "INSERT INTO tracks (id, file_path, file_name, format, file_size) VALUES
                ('t1', '/m/a.aif', 'a.aif', 'AIF', 1),
                ('t2', '/m/b.wv', 'b.wv', 'WV', 1),
                ('t3', '/m/c.mp3', 'c.mp3', 'MP3', 1);",
        )
        .unwrap();
        let rules = r#"{"match":"all","rules":[
            {"kind":"text","field":"format","op":"is","value":"wv"},
            {"kind":"text","field":"title","op":"is","value":"WV"},
            {"kind":"text","field":"format","op":"is","value":"FLAC"}
        ],"order":null,"limit":null}"#;
        conn.execute(
            "INSERT INTO playlists (id, name, rules) VALUES ('p1', '可逆', ?1)",
            [rules],
        )
        .unwrap();

        setup_connection(&conn).unwrap();
        setup_connection(&conn).unwrap();

        let formats: Vec<String> = conn
            .prepare("SELECT format FROM tracks ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(formats, ["AIFF", "WavPack", "MP3"]);
        let stored: String = conn
            .query_row("SELECT rules FROM playlists WHERE id = 'p1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let stored: serde_json::Value = serde_json::from_str(&stored).unwrap();
        let values: Vec<&str> = stored["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rule| rule["value"].as_str().unwrap())
            .collect();
        // 形式の条件だけを直す（タイトルの「WV」は、そのまま）
        assert_eq!(values, ["WavPack", "WV", "FLAC"]);
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
        // 移行を始めていない（消した表を作り直していない）
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

    /// 版を記録する前の版のアプリが作ったDBは、そのまま開いて、今の版を記録する
    #[test]
    fn test_database_without_schema_version_is_opened() {
        let conn = open_in_memory();
        conn.execute_batch("DROP TABLE app_meta").unwrap();
        assert_eq!(stored_schema_version(&conn).unwrap(), None);

        setup_connection(&conn).unwrap();
        assert_eq!(
            stored_schema_version(&conn).unwrap(),
            Some(DB_SCHEMA_VERSION)
        );
    }

    /// 評価を星の数（0〜5）で持っていたDBは、評価と自動プレイリストの評価の条件を、1回だけ2倍にする
    #[test]
    fn test_migration_doubles_ratings_once() {
        let conn = open_in_memory();
        // 評価を半分の星で持つ前の版のDB（`rating_read`がない）
        conn.execute_batch(
            "ALTER TABLE tracks DROP COLUMN rating_read;
             INSERT INTO tracks (id, file_path, file_name, format, file_size, rating) VALUES
                ('none', '/m/a.mp3', 'a.mp3', 'MP3', 1, 0),
                ('four', '/m/b.mp3', 'b.mp3', 'MP3', 1, 4),
                ('five', '/m/c.mp3', 'c.mp3', 'MP3', 1, 5);",
        )
        .unwrap();
        let rules = r#"{"matchMode":"all","rules":[
            {"kind":"number","field":"rating","op":"atLeast","value":4,"valueTo":null},
            {"kind":"number","field":"rating","op":"between","value":2,"valueTo":3},
            {"kind":"number","field":"year","op":"atLeast","value":4,"valueTo":null}
        ],"order":{"field":"rating","descending":true},"limit":null}"#;
        conn.execute(
            "INSERT INTO playlists (id, name, rules) VALUES ('p1', '星4以上', ?1)",
            [rules],
        )
        .unwrap();

        setup_connection(&conn).unwrap();
        // 次の起動では、もう2倍にしない
        setup_connection(&conn).unwrap();

        let ratings: Vec<(i32, i32)> = conn
            .prepare("SELECT rating, rating_read FROM tracks ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_>>()
            .unwrap();
        // 並びはid順（five・four・none）。足す前に登録した曲は、ファイルから読み直す（0）
        assert_eq!(ratings, [(10, 0), (8, 0), (0, 0)]);

        let stored: String = conn
            .query_row("SELECT rules FROM playlists WHERE id = 'p1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let stored: crate::smart_playlist::SmartRules = serde_json::from_str(&stored).unwrap();
        let values: Vec<(i32, Option<i32>)> = stored
            .rules
            .iter()
            .map(|rule| match rule {
                crate::smart_playlist::SmartRule::Number {
                    value, value_to, ..
                } => (*value, *value_to),
                _ => unreachable!(),
            })
            .collect();
        // 評価の条件だけを2倍にする（年の条件は、そのまま）
        assert_eq!(values, [(8, None), (4, Some(6)), (4, None)]);
    }

    /// 上限を超える再生回数・スキップ回数が入ったDBも、起動時に直して読めるようにすること
    #[test]
    fn test_migration_limits_counts_that_cannot_be_read() {
        let conn = open_in_memory();
        // 上限を確かめずにライブラリのXMLを取り込んでいた版が書いた値
        conn.execute_batch(
            "INSERT INTO tracks (id, file_path, file_name, format, file_size, play_count, skip_count)
             VALUES ('big', '/m/big.mp3', 'big.mp3', 'mp3', 1, 3000000000, NULL),
                    ('both', '/m/both.mp3', 'both.mp3', 'mp3', 1, 2147483648, 9000000000),
                    ('small', '/m/small.mp3', 'small.mp3', 'mp3', 1, 7, 2);",
        )
        .expect("トラックの挿入に失敗");
        assert!(crate::repository::find_all_tracks(&conn).is_err());

        // 次の起動
        setup_connection(&conn).expect("2回目のマイグレーション実行に失敗");

        let counts: Vec<(String, i32, i32)> = crate::repository::find_all_tracks(&conn)
            .expect("一覧を読めること")
            .into_iter()
            .map(|track| (track.id, track.play_count, track.skip_count))
            .collect();
        let max = i32::MAX;
        assert_eq!(counts.len(), 3);
        assert!(counts.contains(&("big".to_string(), max, 0)));
        assert!(counts.contains(&("both".to_string(), max, max)));
        assert!(counts.contains(&("small".to_string(), 7, 2)));
        // 値のなかったスキップ回数は、値のないまま
        let skip_count: Option<i64> = conn
            .query_row(
                "SELECT skip_count FROM tracks WHERE id = 'big'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(skip_count, None);
    }

    /// ファイルのDBを作ると、すべての表ができる（一時フォルダに作る。作業フォルダにファイルを残さない）
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
            "tracks_fts",
        ] {
            assert!(
                tables.iter().any(|name| name == table),
                "{table}: {tables:?}"
            );
        }
    }
}
