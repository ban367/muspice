//! データアクセス層
//!
//! commands.rsで重複していたTrackマッピングコードとSQLクエリを集約する。
//! 全てのデータベース読み取り操作はこのモジュールを経由する。

use crate::error::{AppError, AppResult};
use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, Row};

use crate::models::{AlbumGroup, ArtistGroup, GenreGroup, Metadata, Track};

/// クエリ結果の最大取得件数
///
/// パフォーマンスとメモリ使用量のバランスを考慮した設計上の制限。
/// 仮想スクロール（100曲以上のリスト）と組み合わせて使用する。
const DEFAULT_QUERY_LIMIT: usize = 1000;

/// SELECTで使用するトラックカラム列挙（21列）
///
/// is_favorite, rating, play_countはCOALESCEでNULL安全にしている。
pub const TRACK_COLUMNS: &str = "id, file_path, file_name, title, artist, album, genre, year,
    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
    created_at, updated_at";

/// SQLiteの行からTrack構造体にマッピングする
///
/// TRACK_COLUMNSの順序に依存する。is_favoriteはi32→bool変換を行う。
pub fn map_track_row(row: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        file_path: row.get(1)?,
        file_name: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        album: row.get(5)?,
        genre: row.get(6)?,
        year: row.get(7)?,
        track_number: row.get(8)?,
        disc_number: row.get(9)?,
        duration: row.get(10)?,
        file_size: row.get(11)?,
        format: row.get(12)?,
        bitrate: row.get(13)?,
        sample_rate: row.get(14)?,
        is_favorite: row.get::<_, i32>(15)? != 0,
        rating: row.get(16)?,
        play_count: row.get(17)?,
        last_played_at: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
    })
}

/// 全トラックを取得（作成日時の降順、最大1000件）
pub fn find_all_tracks(conn: &Connection) -> AppResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks ORDER BY created_at DESC LIMIT {}",
        TRACK_COLUMNS, DEFAULT_QUERY_LIMIT
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let tracks = stmt
        .query_map([], map_track_row)
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    Ok(tracks)
}

/// IDでトラックを1件取得
pub fn find_track_by_id(conn: &Connection, track_id: &str) -> AppResult<Track> {
    let sql = format!("SELECT {} FROM tracks WHERE id = ?1", TRACK_COLUMNS);
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    stmt.query_row([track_id], map_track_row)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("指定されたトラックが見つかりません".to_string())
            }
            _ => AppError::Database(format!("トラックの取得に失敗しました: {}", e)),
        })
}

/// テキスト検索（FTS5 + LIKEフォールバック）
///
/// FTS5テーブルが利用可能であればMATCH検索を行い、
/// 利用不可の場合はLIKE検索にフォールバックする。
pub fn search_tracks_by_query(conn: &Connection, query: &str) -> AppResult<Vec<Track>> {
    // FTS5テーブルの存在確認
    let fts_available: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tracks_fts'",
            [],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )
        .unwrap_or(false);

    if fts_available {
        // FTS5検索を試行
        match search_tracks_fts(conn, query) {
            Ok(tracks) => return Ok(tracks),
            Err(_) => {
                // FTS5検索失敗時はLIKEフォールバック
            }
        }
    }

    // LIKEフォールバック
    search_tracks_like(conn, query)
}

/// FTS5を使用した全文検索
fn search_tracks_fts(conn: &Connection, query: &str) -> AppResult<Vec<Track>> {
    // FTS5用のクエリ文字列を構築（特殊文字のエスケープ）
    let fts_query = query.replace([';', '\'', '"'], "");
    let fts_query = format!("\"{}\"", fts_query);

    let sql = format!(
        "SELECT {} FROM tracks
         WHERE id IN (
             SELECT id FROM tracks_fts WHERE tracks_fts MATCH ?1
         )
         ORDER BY created_at DESC LIMIT {}",
        TRACK_COLUMNS, DEFAULT_QUERY_LIMIT
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(format!("FTS5クエリの準備に失敗しました: {}", e)))?;

    let tracks = stmt
        .query_map([&fts_query], map_track_row)
        .map_err(|e| AppError::Database(format!("FTS5クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("FTS5結果の取得に失敗しました: {}", e)))?;

    Ok(tracks)
}

/// LIKEパターンのワイルドカード（`%`・`_`）をエスケープする
///
/// エスケープ文字は`\`とし、SQL側で`ESCAPE '\'`を指定する。
/// これがないと「50%」のような検索語で`%`が任意文字列として解釈される。
fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// LIKE検索によるフォールバック
fn search_tracks_like(conn: &Connection, query: &str) -> AppResult<Vec<Track>> {
    let like_pattern = format!("%{}%", escape_like_pattern(query));
    let sql = format!(
        "SELECT {} FROM tracks
         WHERE title LIKE ?1 ESCAPE '\\' OR artist LIKE ?1 ESCAPE '\\'
            OR album LIKE ?1 ESCAPE '\\' OR genre LIKE ?1 ESCAPE '\\'
         ORDER BY created_at DESC LIMIT {}",
        TRACK_COLUMNS, DEFAULT_QUERY_LIMIT
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let tracks = stmt
        .query_map([&like_pattern], map_track_row)
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    Ok(tracks)
}

/// フィルタオプション
#[derive(Debug, serde::Deserialize, specta::Type)]
pub struct FilterOptions {
    #[specta(optional)]
    pub artist: Option<String>,
    #[specta(optional)]
    pub album: Option<String>,
    #[specta(optional)]
    pub genre: Option<String>,
}

/// フィルタ条件に基づいてトラックを検索
pub fn find_tracks_by_filter(conn: &Connection, filters: &FilterOptions) -> AppResult<Vec<Track>> {
    let mut sql = format!("SELECT {} FROM tracks WHERE 1=1", TRACK_COLUMNS);
    let mut params: Vec<String> = Vec::new();

    if let Some(ref artist) = filters.artist {
        sql.push_str(" AND artist = ?");
        params.push(artist.clone());
    }

    if let Some(ref album) = filters.album {
        sql.push_str(" AND album = ?");
        params.push(album.clone());
    }

    if let Some(ref genre) = filters.genre {
        sql.push_str(" AND genre = ?");
        params.push(genre.clone());
    }

    sql.push_str(&format!(
        " ORDER BY created_at DESC LIMIT {}",
        DEFAULT_QUERY_LIMIT
    ));

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let params_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let tracks = stmt
        .query_map(params_refs.as_slice(), map_track_row)
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    Ok(tracks)
}

/// トラックIDからファイルパスを取得
pub fn find_file_path_by_track_id(conn: &Connection, track_id: &str) -> AppResult<String> {
    try_find_file_path_by_track_id(conn, track_id)?
        .ok_or_else(|| AppError::NotFound("指定されたトラックが見つかりません".to_string()))
}

/// トラックIDからファイルパスを取得（存在しない場合はNone）
pub fn try_find_file_path_by_track_id(
    conn: &Connection,
    track_id: &str,
) -> AppResult<Option<String>> {
    let mut stmt = conn
        .prepare("SELECT file_path FROM tracks WHERE id = ?1")
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    match stmt.query_row([track_id], |row| row.get(0)) {
        Ok(path) => Ok(Some(path)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(format!(
            "トラックの取得に失敗しました: {}",
            e
        ))),
    }
}

/// お気に入りトラックを取得
pub fn find_favorite_tracks(conn: &Connection) -> AppResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE is_favorite = 1 ORDER BY updated_at DESC LIMIT {}",
        TRACK_COLUMNS, DEFAULT_QUERY_LIMIT
    );
    query_tracks(conn, &sql, &[])
}

/// 最も再生されたトラックを取得
pub fn find_most_played_tracks(conn: &Connection, limit: i32) -> AppResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE play_count > 0 ORDER BY play_count DESC LIMIT ?1",
        TRACK_COLUMNS
    );
    query_tracks(conn, &sql, &[&limit])
}

/// 最近再生されたトラックを取得
pub fn find_recently_played_tracks(conn: &Connection, limit: i32) -> AppResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE last_played_at IS NOT NULL ORDER BY last_played_at DESC LIMIT ?1",
        TRACK_COLUMNS
    );
    query_tracks(conn, &sql, &[&limit])
}

/// 共通のトラッククエリ実行ヘルパー
fn query_tracks(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> AppResult<Vec<Track>> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let tracks = stmt
        .query_map(params, map_track_row)
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    Ok(tracks)
}

/// アルバム別にグループ化されたトラックを取得
pub fn find_albums_grouped(conn: &Connection) -> AppResult<Vec<AlbumGroup>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE album IS NOT NULL ORDER BY album, track_number, title",
        TRACK_COLUMNS
    );
    let tracks = query_tracks(conn, &sql, &[])?;

    // アルバムごとにグループ化
    let mut album_map: HashMap<String, Vec<Track>> = HashMap::new();
    for track in tracks {
        if let Some(ref album) = track.album {
            album_map.entry(album.clone()).or_default().push(track);
        }
    }

    let mut albums: Vec<AlbumGroup> = album_map
        .into_iter()
        .map(|(album_name, tracks)| {
            let artist = tracks.first().and_then(|t| t.artist.clone());
            let total_duration = tracks.iter().filter_map(|t| t.duration).sum();
            let representative_track_id = tracks.first().map(|t| t.id.clone()).unwrap_or_default();
            let track_count = tracks.len() as i32;

            AlbumGroup {
                name: album_name,
                artist,
                track_count,
                total_duration,
                representative_track_id,
                tracks,
            }
        })
        .collect();

    albums.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(albums)
}

/// アーティスト別にグループ化されたトラックを取得
pub fn find_artists_grouped(conn: &Connection) -> AppResult<Vec<ArtistGroup>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE artist IS NOT NULL ORDER BY artist, album, track_number, title",
        TRACK_COLUMNS
    );
    let tracks = query_tracks(conn, &sql, &[])?;

    // アーティスト → アルバム → トラックのネスト構造を構築
    let mut artist_map: HashMap<String, HashMap<String, Vec<Track>>> = HashMap::new();
    for track in tracks {
        if let Some(ref artist) = track.artist {
            let album_name = track
                .album
                .clone()
                .unwrap_or_else(|| "不明なアルバム".to_string());
            artist_map
                .entry(artist.clone())
                .or_default()
                .entry(album_name)
                .or_default()
                .push(track);
        }
    }

    let mut artists: Vec<ArtistGroup> = artist_map
        .into_iter()
        .map(|(artist_name, album_map)| {
            let mut all_albums: Vec<AlbumGroup> = Vec::new();
            let mut total_track_count: i32 = 0;
            let mut total_duration: i32 = 0;
            let mut first_track_id = String::new();

            for (album_name, tracks) in album_map {
                let album_track_count = tracks.len() as i32;
                let album_duration: i32 = tracks.iter().filter_map(|t| t.duration).sum();
                let album_representative_id =
                    tracks.first().map(|t| t.id.clone()).unwrap_or_default();

                if first_track_id.is_empty() {
                    first_track_id = album_representative_id.clone();
                }

                total_track_count += album_track_count;
                total_duration += album_duration;

                all_albums.push(AlbumGroup {
                    name: album_name,
                    artist: Some(artist_name.clone()),
                    track_count: album_track_count,
                    total_duration: album_duration,
                    representative_track_id: album_representative_id,
                    tracks,
                });
            }

            all_albums.sort_by_cached_key(|a| a.name.to_lowercase());

            ArtistGroup {
                name: artist_name,
                album_count: all_albums.len() as i32,
                track_count: total_track_count,
                total_duration,
                representative_track_id: first_track_id,
                albums: all_albums,
            }
        })
        .collect();

    artists.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(artists)
}

/// ジャンル別にグループ化されたトラックを取得
pub fn find_genres_grouped(conn: &Connection) -> AppResult<Vec<GenreGroup>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE genre IS NOT NULL ORDER BY genre, artist, album, track_number, title",
        TRACK_COLUMNS
    );
    let tracks = query_tracks(conn, &sql, &[])?;

    // ジャンルごとにグループ化
    let mut genre_map: HashMap<String, Vec<Track>> = HashMap::new();
    for track in tracks {
        if let Some(ref genre) = track.genre {
            genre_map.entry(genre.clone()).or_default().push(track);
        }
    }

    let mut genres: Vec<GenreGroup> = genre_map
        .into_iter()
        .map(|(genre_name, tracks)| {
            let total_duration = tracks.iter().filter_map(|t| t.duration).sum();
            let representative_track_id = tracks.first().map(|t| t.id.clone()).unwrap_or_default();
            let track_count = tracks.len() as i32;

            GenreGroup {
                name: genre_name,
                track_count,
                total_duration,
                representative_track_id,
                tracks,
            }
        })
        .collect();

    genres.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(genres)
}

/// トラックのメタデータ（title/artist/album/genre/year）を更新
///
/// 対象トラックが存在しない場合はエラーを返す。
pub fn update_track_metadata(
    conn: &Connection,
    track_id: &str,
    metadata: &Metadata,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let rows_affected = conn
        .execute(
            "UPDATE tracks SET
                title = ?1,
                artist = ?2,
                album = ?3,
                genre = ?4,
                year = ?5,
                updated_at = ?6
             WHERE id = ?7",
            rusqlite::params![
                metadata.title,
                metadata.artist,
                metadata.album,
                metadata.genre,
                metadata.year,
                now,
                track_id,
            ],
        )
        .map_err(|e| AppError::Database(format!("メタデータの更新に失敗しました: {}", e)))?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(
            "指定されたトラックが見つかりません".to_string(),
        ));
    }

    Ok(())
}

/// トラックの存在を確認
pub fn track_exists(conn: &Connection, track_id: &str) -> AppResult<bool> {
    let mut stmt = conn
        .prepare("SELECT 1 FROM tracks WHERE id = ?1")
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    stmt.exists([track_id])
        .map_err(|e| AppError::Database(format!("トラックの確認に失敗しました: {}", e)))
}

/// トラックのメタデータを部分更新（Someのフィールドのみ・一括編集用）
///
/// 対象トラックが存在しない場合はエラーを返す。
/// 更新するフィールドがない場合は何もしない。
pub fn update_track_metadata_partial(
    conn: &Connection,
    track_id: &str,
    metadata: &Metadata,
    now: &str,
) -> AppResult<()> {
    let mut update_parts = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if metadata.title.is_some() {
        update_parts.push("title = ?");
        params.push(Box::new(metadata.title.clone()));
    }

    if metadata.artist.is_some() {
        update_parts.push("artist = ?");
        params.push(Box::new(metadata.artist.clone()));
    }

    if metadata.album.is_some() {
        update_parts.push("album = ?");
        params.push(Box::new(metadata.album.clone()));
    }

    if metadata.genre.is_some() {
        update_parts.push("genre = ?");
        params.push(Box::new(metadata.genre.clone()));
    }

    if metadata.year.is_some() {
        update_parts.push("year = ?");
        params.push(Box::new(metadata.year));
    }

    if update_parts.is_empty() {
        // 更新するフィールドがない場合も存在チェックのみ行う
        if !track_exists(conn, track_id)? {
            return Err(AppError::NotFound(format!(
                "トラックが見つかりません: {}",
                track_id
            )));
        }
        return Ok(());
    }

    update_parts.push("updated_at = ?");
    params.push(Box::new(now.to_string()));

    let sql = format!("UPDATE tracks SET {} WHERE id = ?", update_parts.join(", "));
    params.push(Box::new(track_id.to_string()));

    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    // 更新行数で存在判定する（事前チェックだと確認後の削除との競合を検出できない）
    let rows_affected = conn
        .execute(&sql, params_refs.as_slice())
        .map_err(|e| AppError::Database(format!("メタデータの更新に失敗しました: {}", e)))?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "トラックが見つかりません: {}",
            track_id
        )));
    }

    Ok(())
}

/// 全トラックの (id, file_path) 一覧を取得
pub fn find_all_track_file_paths(conn: &Connection) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare("SELECT id, file_path FROM tracks")
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let paths = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    Ok(paths)
}

/// トラック番号・ディスク番号を更新
///
/// 対象トラックが存在しない場合はエラーを返す。
pub fn update_track_numbers(
    conn: &Connection,
    track_id: &str,
    track_number: Option<i32>,
    disc_number: Option<i32>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let rows_affected = conn
        .execute(
            "UPDATE tracks SET track_number = ?1, disc_number = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![track_number, disc_number, now, track_id],
        )
        .map_err(|e| AppError::Database(format!("トラック番号の更新に失敗しました: {}", e)))?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "トラックが見つかりません: {}",
            track_id
        )));
    }

    Ok(())
}

/// トラックを新規挿入
pub fn insert_track(conn: &Connection, track: &Track) -> AppResult<()> {
    conn.execute(
        "INSERT INTO tracks (
            id, file_path, file_name, title, artist, album, genre, year,
            track_number, disc_number, duration, file_size, format, bitrate, sample_rate, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        rusqlite::params![
            track.id,
            track.file_path,
            track.file_name,
            track.title,
            track.artist,
            track.album,
            track.genre,
            track.year,
            track.track_number,
            track.disc_number,
            track.duration,
            track.file_size,
            track.format,
            track.bitrate,
            track.sample_rate,
            track.created_at,
            track.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(format!("トラックの保存に失敗しました: {}", e)))?;
    Ok(())
}

/// file_pathをキーに既存トラックを更新（重複時の置き換え用）
///
/// 対象が存在しない場合はエラーを返す（更新したつもりで実際は無変更、を防ぐ）。
pub fn update_track_by_file_path(conn: &Connection, track: &Track) -> AppResult<()> {
    let rows_affected = conn.execute(
        "UPDATE tracks SET
            file_name = ?2, title = ?3, artist = ?4, album = ?5, genre = ?6, year = ?7,
            track_number = ?8, disc_number = ?9, duration = ?10, file_size = ?11, format = ?12, bitrate = ?13, sample_rate = ?14,
            updated_at = ?15
        WHERE file_path = ?1",
        rusqlite::params![
            track.file_path,
            track.file_name,
            track.title,
            track.artist,
            track.album,
            track.genre,
            track.year,
            track.track_number,
            track.disc_number,
            track.duration,
            track.file_size,
            track.format,
            track.bitrate,
            track.sample_rate,
            track.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(format!("トラックの更新に失敗しました: {}", e)))?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "更新対象のトラックが見つかりません: {}",
            track.file_path
        )));
    }

    Ok(())
}

/// 登録済みの全ファイルパスを取得する
///
/// インポート時の重複判定に使用する。ファイルごとにクエリを発行する代わりに
/// 一度だけ取得することで、DBロックの保持時間とクエリ回数を削減する。
pub fn find_all_file_paths(conn: &Connection) -> AppResult<HashSet<String>> {
    let mut stmt = conn
        .prepare("SELECT file_path FROM tracks")
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let paths = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    Ok(paths)
}

/// トラックを1件削除（削除された行数を返す）
///
/// ON DELETE CASCADEにより、playlist_tracksとplay_historyの関連レコードも自動削除される。
pub fn delete_track(conn: &Connection, track_id: &str) -> AppResult<usize> {
    conn.execute("DELETE FROM tracks WHERE id = ?1", [track_id])
        .map_err(|e| AppError::Database(format!("トラックの削除に失敗しました: {}", e)))
}

/// お気に入り状態をトグルし、新しい状態を返す
pub fn toggle_track_favorite(conn: &Connection, track_id: &str) -> AppResult<bool> {
    let now = chrono::Utc::now().to_rfc3339();

    // 反転と読み出しを1文で行う
    // SELECTしてからUPDATEすると、その間に別の更新が入った場合に
    // 古い値を元にした反転結果で上書きしてしまう
    let new_value: i32 = conn
        .query_row(
            "UPDATE tracks
             SET is_favorite = 1 - COALESCE(is_favorite, 0), updated_at = ?1
             WHERE id = ?2
             RETURNING is_favorite",
            rusqlite::params![now, track_id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("トラックが見つかりません".to_string())
            }
            _ => AppError::Database(format!("お気に入りの更新に失敗しました: {}", e)),
        })?;

    Ok(new_value == 1)
}

/// レーティングを設定
pub fn set_track_rating(conn: &Connection, track_id: &str, rating: i32) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE tracks SET rating = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![rating, now, track_id],
    )
    .map_err(|e| AppError::Database(format!("レーティングの更新に失敗しました: {}", e)))?;

    Ok(())
}

/// 再生回数をインクリメントして再生履歴に追加し、新しい再生回数を返す
pub fn increment_track_play_count(conn: &Connection, track_id: &str) -> AppResult<i32> {
    let now = chrono::Utc::now().to_rfc3339();

    // 再生回数をインクリメントし、更新後の値をそのまま受け取る
    let new_count: i32 = conn
        .query_row(
            "UPDATE tracks SET
                play_count = COALESCE(play_count, 0) + 1,
                last_played_at = ?1,
                updated_at = ?1
             WHERE id = ?2
             RETURNING play_count",
            rusqlite::params![now, track_id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("トラックが見つかりません".to_string())
            }
            _ => AppError::Database(format!("再生回数の更新に失敗しました: {}", e)),
        })?;

    // 再生履歴に追加
    conn.execute(
        "INSERT INTO play_history (track_id, played_at) VALUES (?1, ?2)",
        rusqlite::params![track_id, now],
    )
    .map_err(|e| AppError::Database(format!("再生履歴の追加に失敗しました: {}", e)))?;

    Ok(new_count)
}

/// ユニークなアーティスト一覧を取得
pub fn find_unique_artists(conn: &Connection) -> AppResult<Vec<String>> {
    find_unique_values(conn, "artist")
}

/// ユニークなアルバム一覧を取得
pub fn find_unique_albums(conn: &Connection) -> AppResult<Vec<String>> {
    find_unique_values(conn, "album")
}

/// ユニークなジャンル一覧を取得
pub fn find_unique_genres(conn: &Connection) -> AppResult<Vec<String>> {
    find_unique_values(conn, "genre")
}

/// 指定カラムのユニーク値一覧を取得する共通ヘルパー
fn find_unique_values(conn: &Connection, column: &str) -> AppResult<Vec<String>> {
    let sql = format!(
        "SELECT DISTINCT {} FROM tracks WHERE {} IS NOT NULL ORDER BY {}",
        column, column, column
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let values = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// テスト用のインメモリDBを作成してスキーマを適用
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
        crate::db::run_migrations(&conn).expect("マイグレーション実行に失敗");
        conn
    }

    /// テスト用のトラックをDBに挿入
    fn insert_test_track(
        conn: &Connection,
        id: &str,
        title: &str,
        artist: &str,
        album: &str,
        genre: &str,
    ) {
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, title, artist, album, genre, year, format, file_size, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 2024, 'mp3', 1000, datetime('now'), datetime('now'))",
            rusqlite::params![
                id,
                format!("/test/{}.mp3", id),
                format!("{}.mp3", id),
                title,
                artist,
                album,
                genre
            ],
        )
        .expect("テストトラック挿入に失敗");
    }

    #[test]
    fn test_find_all_tracks_empty() {
        let conn = setup_test_db();
        let tracks = find_all_tracks(&conn).unwrap();
        assert!(tracks.is_empty());
    }

    #[test]
    fn test_find_all_tracks() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");

        let tracks = find_all_tracks(&conn).unwrap();
        assert_eq!(tracks.len(), 2);
    }

    #[test]
    fn test_find_track_by_id() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        let track = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(track.id, "t1");
        assert_eq!(track.title, Some("曲A".to_string()));
    }

    #[test]
    fn test_find_track_by_id_not_found() {
        let conn = setup_test_db();
        let result = find_track_by_id(&conn, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("見つかりません"));
    }

    #[test]
    fn test_search_tracks_like() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "夜に駆ける", "YOASOBI", "THE BOOK", "J-POP");
        insert_test_track(&conn, "t2", "群青", "YOASOBI", "THE BOOK 2", "J-POP");
        insert_test_track(&conn, "t3", "Lemon", "米津玄師", "BOOTLEG", "J-POP");

        // アーティスト名で検索
        let tracks = search_tracks_by_query(&conn, "YOASOBI").unwrap();
        assert_eq!(tracks.len(), 2);

        // タイトルで検索
        let tracks = search_tracks_by_query(&conn, "Lemon").unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].artist, Some("米津玄師".to_string()));
    }

    #[test]
    fn test_escape_like_pattern() {
        assert_eq!(escape_like_pattern("normal"), "normal");
        assert_eq!(escape_like_pattern("50%"), "50\\%");
        assert_eq!(escape_like_pattern("a_b"), "a\\_b");
        assert_eq!(escape_like_pattern("a\\b"), "a\\\\b");
    }

    /// LIKE検索でワイルドカードがリテラルとして扱われることを検証する
    ///
    /// `search_tracks_by_query`はFTS5を優先するため、フォールバック実装を直接呼ぶ。
    #[test]
    fn test_search_tracks_like_escapes_wildcards() {
        let conn = setup_test_db();
        insert_test_track(
            &conn,
            "t1",
            "50%OFF",
            "アーティストX",
            "アルバム1",
            "ロック",
        );
        insert_test_track(
            &conn,
            "t2",
            "50XOFF",
            "アーティストY",
            "アルバム2",
            "ロック",
        );
        insert_test_track(&conn, "t3", "a_b", "アーティストZ", "アルバム3", "ロック");
        insert_test_track(&conn, "t4", "axb", "アーティストW", "アルバム4", "ロック");

        // `%`はリテラルとして扱われ、"50XOFF"にはマッチしない
        let tracks = search_tracks_like(&conn, "50%O").unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "t1");

        // `_`もリテラルとして扱われ、"axb"にはマッチしない
        let tracks = search_tracks_like(&conn, "a_b").unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "t3");

        // 通常の検索語は従来どおり部分一致する
        let tracks = search_tracks_like(&conn, "OFF").unwrap();
        assert_eq!(tracks.len(), 2);
    }

    #[test]
    fn test_find_tracks_by_filter() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "アルバム2", "ポップ");
        insert_test_track(&conn, "t3", "曲C", "アーティストY", "アルバム1", "ロック");

        // アーティストでフィルタ
        let filters = FilterOptions {
            artist: Some("アーティストX".to_string()),
            album: None,
            genre: None,
        };
        let tracks = find_tracks_by_filter(&conn, &filters).unwrap();
        assert_eq!(tracks.len(), 2);

        // ジャンルでフィルタ
        let filters = FilterOptions {
            artist: None,
            album: None,
            genre: Some("ロック".to_string()),
        };
        let tracks = find_tracks_by_filter(&conn, &filters).unwrap();
        assert_eq!(tracks.len(), 2);

        // 複合フィルタ
        let filters = FilterOptions {
            artist: Some("アーティストX".to_string()),
            album: None,
            genre: Some("ロック".to_string()),
        };
        let tracks = find_tracks_by_filter(&conn, &filters).unwrap();
        assert_eq!(tracks.len(), 1);
    }

    #[test]
    fn test_find_file_path_by_track_id() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        let path = find_file_path_by_track_id(&conn, "t1").unwrap();
        assert_eq!(path, "/test/t1.mp3");
    }

    #[test]
    fn test_find_favorite_tracks() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");

        // t1をお気に入りに設定
        conn.execute("UPDATE tracks SET is_favorite = 1 WHERE id = 't1'", [])
            .unwrap();

        let tracks = find_favorite_tracks(&conn).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "t1");
        assert!(tracks[0].is_favorite);
    }

    #[test]
    fn test_toggle_track_favorite() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        // 未設定 → お気に入り
        assert!(toggle_track_favorite(&conn, "t1").unwrap());
        assert!(find_track_by_id(&conn, "t1").unwrap().is_favorite);

        // お気に入り → 解除
        assert!(!toggle_track_favorite(&conn, "t1").unwrap());
        assert!(!find_track_by_id(&conn, "t1").unwrap().is_favorite);

        // 存在しないトラックはNotFound
        let result = toggle_track_favorite(&conn, "nonexistent");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("トラックが見つかりません"));
    }

    #[test]
    fn test_increment_track_play_count() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        // 戻り値は更新後の再生回数
        assert_eq!(increment_track_play_count(&conn, "t1").unwrap(), 1);
        assert_eq!(increment_track_play_count(&conn, "t1").unwrap(), 2);

        let track = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(track.play_count, 2);
        assert!(track.last_played_at.is_some());

        // 再生履歴も追加される
        let history_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM play_history WHERE track_id = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history_count, 2);

        // 存在しないトラックはNotFound（履歴も追加しない）
        let result = increment_track_play_count(&conn, "nonexistent");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("トラックが見つかりません"));
    }

    #[test]
    fn test_find_most_played_tracks() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");

        conn.execute("UPDATE tracks SET play_count = 10 WHERE id = 't1'", [])
            .unwrap();
        conn.execute("UPDATE tracks SET play_count = 5 WHERE id = 't2'", [])
            .unwrap();

        let tracks = find_most_played_tracks(&conn, 10).unwrap();
        assert_eq!(tracks.len(), 2);
        // play_countの降順
        assert_eq!(tracks[0].id, "t1");
        assert_eq!(tracks[0].play_count, 10);
    }

    #[test]
    fn test_find_albums_grouped() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストY", "アルバム2", "ポップ");

        let albums = find_albums_grouped(&conn).unwrap();
        assert_eq!(albums.len(), 2);

        // アルバム名でソートされているはず
        let album1 = albums.iter().find(|a| a.name == "アルバム1").unwrap();
        assert_eq!(album1.track_count, 2);

        let album2 = albums.iter().find(|a| a.name == "アルバム2").unwrap();
        assert_eq!(album2.track_count, 1);
    }

    #[test]
    fn test_find_artists_grouped() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "アルバム2", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストY", "アルバム1", "ポップ");

        let artists = find_artists_grouped(&conn).unwrap();
        assert_eq!(artists.len(), 2);

        let artist_x = artists.iter().find(|a| a.name == "アーティストX").unwrap();
        assert_eq!(artist_x.track_count, 2);
        assert_eq!(artist_x.album_count, 2);
    }

    #[test]
    fn test_find_genres_grouped() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストY", "アルバム2", "ポップ");

        let genres = find_genres_grouped(&conn).unwrap();
        assert_eq!(genres.len(), 2);

        let rock = genres.iter().find(|g| g.name == "ロック").unwrap();
        assert_eq!(rock.track_count, 2);
    }

    #[test]
    fn test_find_unique_artists() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "アルバム2", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストY", "アルバム1", "ポップ");

        let artists = find_unique_artists(&conn).unwrap();
        assert_eq!(artists.len(), 2);
        assert!(artists.contains(&"アーティストX".to_string()));
        assert!(artists.contains(&"アーティストY".to_string()));
    }

    #[test]
    fn test_find_unique_albums() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストY", "アルバム2", "ポップ");

        let albums = find_unique_albums(&conn).unwrap();
        assert_eq!(albums.len(), 2);
    }

    #[test]
    fn test_find_unique_genres() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");

        let genres = find_unique_genres(&conn).unwrap();
        assert_eq!(genres.len(), 2);
    }

    #[test]
    fn test_map_track_row_is_favorite_conversion() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        // is_favorite = 0 → false
        let track = find_track_by_id(&conn, "t1").unwrap();
        assert!(!track.is_favorite);

        // is_favorite = 1 → true
        conn.execute("UPDATE tracks SET is_favorite = 1 WHERE id = 't1'", [])
            .unwrap();
        let track = find_track_by_id(&conn, "t1").unwrap();
        assert!(track.is_favorite);
    }

    /// 部分更新用の空メタデータを作成
    fn empty_metadata() -> Metadata {
        Metadata {
            title: None,
            artist: None,
            album: None,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            album_artist: None,
            composer: None,
        }
    }

    #[test]
    fn test_track_exists() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        assert!(track_exists(&conn, "t1").unwrap());
        assert!(!track_exists(&conn, "nonexistent").unwrap());
    }

    #[test]
    fn test_update_track_metadata_partial_updates_only_some_fields() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        // titleのみ更新（他フィールドは不変であること）
        let metadata = Metadata {
            title: Some("新タイトル".to_string()),
            ..empty_metadata()
        };
        update_track_metadata_partial(&conn, "t1", &metadata, "2026-01-01T00:00:00+00:00").unwrap();

        let track = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(track.title, Some("新タイトル".to_string()));
        assert_eq!(track.artist, Some("アーティストX".to_string()));
        assert_eq!(track.album, Some("アルバム1".to_string()));
        assert_eq!(track.genre, Some("ロック".to_string()));
        assert_eq!(track.updated_at, "2026-01-01T00:00:00+00:00");
    }

    #[test]
    fn test_update_track_metadata_partial_not_found() {
        let conn = setup_test_db();

        // 存在しないID + 更新フィールドあり → NotFound
        let metadata = Metadata {
            title: Some("新タイトル".to_string()),
            ..empty_metadata()
        };
        let result = update_track_metadata_partial(
            &conn,
            "nonexistent",
            &metadata,
            "2026-01-01T00:00:00+00:00",
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("トラックが見つかりません"));

        // 存在しないID + 更新フィールドなし → NotFound
        let result = update_track_metadata_partial(
            &conn,
            "nonexistent",
            &empty_metadata(),
            "2026-01-01T00:00:00+00:00",
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("トラックが見つかりません"));
    }

    /// 存在しないfile_pathへの更新は成功扱いにせずエラーにする
    #[test]
    fn test_update_track_by_file_path_not_found() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        let mut track = find_track_by_id(&conn, "t1").unwrap();
        track.title = Some("新タイトル".to_string());

        // 既存パスなら更新される
        update_track_by_file_path(&conn, &track).unwrap();
        assert_eq!(
            find_track_by_id(&conn, "t1").unwrap().title,
            Some("新タイトル".to_string())
        );

        // 存在しないパスならNotFound
        track.file_path = "/test/unknown.mp3".to_string();
        let result = update_track_by_file_path(&conn, &track);
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("更新対象のトラックが見つかりません"));
    }

    #[test]
    fn test_find_all_file_paths() {
        let conn = setup_test_db();
        assert!(find_all_file_paths(&conn).unwrap().is_empty());

        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");

        let paths = find_all_file_paths(&conn).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains("/test/t1.mp3"));
        assert!(paths.contains("/test/t2.mp3"));
        assert!(!paths.contains("/test/unknown.mp3"));
    }

    #[test]
    fn test_find_all_track_file_paths() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");

        let paths = find_all_track_file_paths(&conn).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&("t1".to_string(), "/test/t1.mp3".to_string())));
        assert!(paths.contains(&("t2".to_string(), "/test/t2.mp3".to_string())));
    }

    #[test]
    fn test_update_track_numbers() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        update_track_numbers(&conn, "t1", Some(3), Some(2)).unwrap();

        let track = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(track.track_number, Some(3));
        assert_eq!(track.disc_number, Some(2));

        // 存在しないID → NotFound
        let result = update_track_numbers(&conn, "nonexistent", Some(1), Some(1));
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("トラックが見つかりません"));
    }
}
