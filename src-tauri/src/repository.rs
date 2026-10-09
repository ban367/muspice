//! データアクセス層
//!
//! commands.rsで重複していたTrackマッピングコードとSQLクエリを集約する。
//! 全てのデータベース読み取り操作はこのモジュールを経由する。

use crate::error::{AppError, AppResult};
use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, Row};

use crate::models::{
    AlbumGroup, AlbumSummary, ArtistSummary, GenreSummary, Metadata, PlayHistoryEntry, ReplayGain,
    Track,
};
use crate::track_relink::MissingTrack;

/// アルバムの中の曲の並び（ディスク番号 → トラック番号 → タイトル）
const ALBUM_TRACK_ORDER: &str = "COALESCE(disc_number, 1), track_number, title";

/// アーティストの詳細で、アルバムのない曲をまとめるアルバムの名前
const UNKNOWN_ALBUM: &str = "不明なアルバム";

/// アルバム・アーティストの一覧をまとめるアーティスト（アルバムアーティスト。なければ曲のアーティスト）
///
/// `db.rs`のインデックス（`idx_tracks_album_artist`）と同じ式にする。
const ALBUM_ARTIST: &str = "COALESCE(album_artist, artist)";

/// SELECTで使用するトラックカラム列挙（27列）
///
/// is_favorite, rating, play_countはCOALESCEでNULL安全にしている。
pub const TRACK_COLUMNS: &str = "id, file_path, file_name, title, artist, album, genre, year,
    track_number, disc_number, duration, file_size, format, bitrate, sample_rate,
    COALESCE(is_favorite, 0), COALESCE(rating, 0), COALESCE(play_count, 0), last_played_at,
    created_at, updated_at,
    replay_gain_track_gain, replay_gain_track_peak, replay_gain_album_gain, replay_gain_album_peak,
    album_artist, missing_since IS NOT NULL, COALESCE(skip_count, 0)";

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
        album_artist: row.get(25)?,
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
        skip_count: row.get(27)?,
        last_played_at: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
        replay_gain: ReplayGain {
            track_gain: row.get(21)?,
            track_peak: row.get(22)?,
            album_gain: row.get(23)?,
            album_peak: row.get(24)?,
        },
        is_missing: row.get(26)?,
    })
}

/// 全トラックを取得（作成日時の降順）
///
/// 件数の上限はない（一覧は、フロントが見えている行だけを描画する）。
pub fn find_all_tracks(conn: &Connection) -> AppResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks ORDER BY created_at DESC",
        TRACK_COLUMNS
    );
    query_tracks(conn, &sql, &[])
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

/// trigramの索引で探せる語の、最小の文字数
const TRIGRAM_MIN_CHARS: usize = 3;

/// テキスト検索（タイトル・アーティスト・アルバム・ジャンル・アルバムアーティストの部分一致）
///
/// 検索語を空白で区切り、すべての語をどこかに含む曲を返す（語の途中の一致を含む）。
/// 大文字と小文字・全角と半角・ひらがなとカタカナの違いは同じとみなす（`search_text`）。
///
/// - すべての語が3文字以上: 全文検索の索引（FTS5のtrigram）で探す
/// - 3文字未満の語がある: 索引では探せないため、検索用の文字列を`LIKE`で走査する
pub fn search_tracks_by_query(conn: &Connection, query: &str) -> AppResult<Vec<Track>> {
    let terms = crate::search_text::query_terms(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let uses_index = terms
        .iter()
        .all(|term| term.chars().count() >= TRIGRAM_MIN_CHARS);
    let (condition, params): (String, Vec<String>) = if uses_index {
        // 語をフレーズ（二重引用符で囲む。中の二重引用符は重ねる）にして、ANDでつなぐ
        let fts_query = terms
            .iter()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        ("tracks_fts MATCH ?1".to_string(), vec![fts_query])
    } else {
        let condition = (1..=terms.len())
            .map(|index| format!("text LIKE ?{index} ESCAPE '\\'"))
            .collect::<Vec<_>>()
            .join(" AND ");
        let patterns = terms
            .iter()
            .map(|term| format!("%{}%", escape_like_pattern(term)))
            .collect();
        (condition, patterns)
    };

    let sql = format!(
        "SELECT {} FROM tracks
         WHERE rowid IN (SELECT rowid FROM tracks_fts WHERE {})
         ORDER BY created_at DESC",
        TRACK_COLUMNS, condition
    );
    let params: Vec<&dyn rusqlite::ToSql> = params
        .iter()
        .map(|param| param as &dyn rusqlite::ToSql)
        .collect();
    query_tracks(conn, &sql, &params)
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

    sql.push_str(" ORDER BY created_at DESC");

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

/// 1回のクエリで指定するトラックIDの数（SQLiteの変数の数の上限より十分に小さくする）
const TRACKS_BY_ID_CHUNK: usize = 500;

/// トラックIDの一覧から、トラックをまとめて取得する（見つからないIDは、結果に含めない）
///
/// 再生キューの復元など、多くのIDを1曲ずつ引くと遅い場合に使う。
pub fn find_tracks_by_ids(
    conn: &Connection,
    track_ids: &[String],
) -> AppResult<HashMap<String, Track>> {
    let mut tracks = HashMap::with_capacity(track_ids.len());
    for chunk in track_ids.chunks(TRACKS_BY_ID_CHUNK) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!(
            "SELECT {} FROM tracks WHERE id IN ({})",
            TRACK_COLUMNS, placeholders
        );
        let params: Vec<&dyn rusqlite::ToSql> =
            chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        for track in query_tracks(conn, &sql, &params)? {
            tracks.insert(track.id.clone(), track);
        }
    }
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
        "SELECT {} FROM tracks WHERE COALESCE(is_favorite, 0) != 0
         ORDER BY favorited_at DESC, album, {ALBUM_TRACK_ORDER}",
        TRACK_COLUMNS
    );
    query_tracks(conn, &sql, &[])
}

/// 最も再生されたトラックを取得（再生回数の多い順。同じ回数なら、最近再生した曲を先にする）
pub fn find_most_played_tracks(conn: &Connection, limit: i32) -> AppResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE play_count > 0
         ORDER BY play_count DESC, last_played_at DESC LIMIT ?1",
        TRACK_COLUMNS
    );
    query_tracks(conn, &sql, &[&limit])
}

/// 再生履歴を取得（新しい順。同じ曲が何度も出る）
///
/// 件数の上限はない（履歴は消さずに残し、一覧は、フロントが見えている行だけを描画する）。
/// 曲の情報は含めない（1件あたりを小さくし、曲はフロントが全曲の一覧から引く）。
pub fn find_play_history(conn: &Connection) -> AppResult<Vec<PlayHistoryEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, track_id, played_at FROM play_history
             ORDER BY played_at DESC, id DESC",
        )
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let entries = stmt
        .query_map([], |row| {
            Ok(PlayHistoryEntry {
                id: row.get(0)?,
                track_id: row.get(1)?,
                played_at: row.get(2)?,
            })
        })
        .map_err(|e| AppError::Database(format!("再生履歴の取得に失敗しました: {}", e)))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| AppError::Database(format!("再生履歴の読み取りに失敗しました: {}", e)))?;

    Ok(entries)
}

/// すべてのトラックの、IDとファイルのパスを取得する（M3Uの読み込みでの突き合わせ用）
pub fn find_track_paths(conn: &Connection) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare("SELECT id, file_path FROM tracks")
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| AppError::Database(format!("トラックの取得に失敗しました: {}", e)))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| AppError::Database(format!("トラックの読み取りに失敗しました: {}", e)))
}

/// 共通のトラッククエリ実行ヘルパー
pub(crate) fn query_tracks(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> AppResult<Vec<Track>> {
    query_rows(conn, sql, params, map_track_row)
}

/// 一覧の集計に使う行を読む（列は呼び出し側のSQLで決める）
fn query_rows<T>(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    map_row: impl FnMut(&Row) -> rusqlite::Result<T>,
) -> AppResult<Vec<T>> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let rows = stmt
        .query_map(params, map_row)
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    Ok(rows)
}

/// アルバムの一覧を取得（アルバム名の順。曲は含めない）
///
/// 「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめる。同じ名前でも
/// アーティストが違うアルバムは別のアルバムにし、アルバムアーティストが同じ曲は、
/// 曲ごとのアーティストが違っても1つのアルバムにする。
/// 代表の曲は、アルバムの最初の曲（`ALBUM_TRACK_ORDER`の順）。
pub fn find_album_summaries(conn: &Connection) -> AppResult<Vec<AlbumSummary>> {
    let sql = format!(
        "SELECT album, {ALBUM_ARTIST} AS group_artist, id, duration FROM tracks
         WHERE album IS NOT NULL
         ORDER BY album, group_artist, {ALBUM_TRACK_ORDER}"
    );
    let rows = query_rows(conn, &sql, &[], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i32>>(3)?,
        ))
    })?;

    // 同じアルバムの曲は続けて並ぶため、前の行と比べてまとめる
    let mut albums: Vec<AlbumSummary> = Vec::new();
    for (album, artist, id, duration) in rows {
        match albums.last_mut() {
            Some(last) if last.name == album && last.artist == artist => {
                last.track_count += 1;
                last.total_duration += duration.unwrap_or(0);
            }
            _ => albums.push(AlbumSummary {
                name: album,
                artist,
                track_count: 1,
                total_duration: duration.unwrap_or(0),
                representative_track_id: id,
            }),
        }
    }

    // 同じ名前のアルバムは、アーティスト名の順（上のSQLの順）のまま並べる
    albums.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(albums)
}

/// アルバムの曲を取得（`ALBUM_TRACK_ORDER`の順）
///
/// `artist`は、アルバムをまとめたアーティスト（`AlbumSummary::artist`。アルバムアーティストも
/// 曲のアーティストもないアルバムはNone）。
pub fn find_album_tracks(
    conn: &Connection,
    album: &str,
    artist: Option<&str>,
) -> AppResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE album = ?1 AND {ALBUM_ARTIST} IS ?2
         ORDER BY {ALBUM_TRACK_ORDER}",
        TRACK_COLUMNS
    );
    query_tracks(conn, &sql, &[&album, &artist])
}

/// アーティストの一覧を取得（アーティスト名の順。アルバムと曲は含めない）
///
/// アルバムアーティスト（なければ曲のアーティスト）でまとめる。コンピレーションの曲は、
/// 曲ごとのアーティストではなく、アルバムアーティスト（「Various Artists」など）に入る。
/// 代表の曲は、詳細で最初に表示するアルバム（名前の順で最初）の最初の曲。
pub fn find_artist_summaries(conn: &Connection) -> AppResult<Vec<ArtistSummary>> {
    let sql = format!(
        "SELECT {ALBUM_ARTIST} AS group_artist, COALESCE(album, ?1) AS album_name, id, duration
         FROM tracks
         WHERE {ALBUM_ARTIST} IS NOT NULL
         ORDER BY group_artist, album_name, {ALBUM_TRACK_ORDER}"
    );
    let rows = query_rows(conn, &sql, &[&UNKNOWN_ALBUM], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i32>>(3)?,
        ))
    })?;

    // 同じアーティスト・同じアルバムの曲は続けて並ぶため、前の行と比べてまとめる
    let mut artists: Vec<ArtistSummary> = Vec::new();
    let mut current_album: Option<String> = None;
    // 代表の曲を選んだアルバムの名前（小文字。詳細のアルバムの並びと同じ比較にする）
    let mut representative_album: Option<String> = None;
    for (artist, album, id, duration) in rows {
        if artists.last().is_none_or(|last| last.name != artist) {
            artists.push(ArtistSummary {
                name: artist,
                album_count: 0,
                track_count: 0,
                total_duration: 0,
                representative_track_id: String::new(),
            });
            current_album = None;
            representative_album = None;
        }
        let Some(summary) = artists.last_mut() else {
            continue;
        };

        summary.track_count += 1;
        summary.total_duration += duration.unwrap_or(0);
        if current_album.as_deref() != Some(album.as_str()) {
            summary.album_count += 1;
            let album_key = album.to_lowercase();
            if representative_album
                .as_ref()
                .is_none_or(|best| album_key < *best)
            {
                summary.representative_track_id = id;
                representative_album = Some(album_key);
            }
            current_album = Some(album);
        }
    }

    artists.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(artists)
}

/// アーティストのアルバムと曲を取得（アルバム名の順。曲は`ALBUM_TRACK_ORDER`の順）
///
/// `artist`は、アルバムアーティスト（なければ曲のアーティスト）。曲ごとのアーティストが違う曲も、
/// アルバムアーティストが一致すれば含める。
/// アルバムのない曲は、`UNKNOWN_ALBUM`という名前のアルバムにまとめる。
pub fn find_artist_albums(conn: &Connection, artist: &str) -> AppResult<Vec<AlbumGroup>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE {ALBUM_ARTIST} = ?1
         ORDER BY COALESCE(album, ?2), {ALBUM_TRACK_ORDER}",
        TRACK_COLUMNS
    );
    let tracks = query_tracks(conn, &sql, &[&artist, &UNKNOWN_ALBUM])?;

    // 同じアルバムの曲は続けて並ぶため、前の行と比べてまとめる
    let mut albums: Vec<AlbumGroup> = Vec::new();
    for track in tracks {
        let album_name = track.album.as_deref().unwrap_or(UNKNOWN_ALBUM);
        match albums.last_mut() {
            Some(last) if last.name == album_name => {
                last.track_count += 1;
                last.total_duration += track.duration.unwrap_or(0);
                last.tracks.push(track);
            }
            _ => albums.push(AlbumGroup {
                name: album_name.to_string(),
                artist: Some(artist.to_string()),
                track_count: 1,
                total_duration: track.duration.unwrap_or(0),
                representative_track_id: track.id.clone(),
                tracks: vec![track],
            }),
        }
    }

    albums.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(albums)
}

/// ジャンルの曲の並び（アルバムをまとめるアーティスト → アルバム → アルバムの中の並び）
fn genre_track_order() -> String {
    format!("{ALBUM_ARTIST}, album, {ALBUM_TRACK_ORDER}")
}

/// ジャンルの一覧を取得（ジャンル名の順。曲は含めない）
///
/// 代表の曲は、ジャンルの最初の曲（`find_genre_tracks`の順）。
pub fn find_genre_summaries(conn: &Connection) -> AppResult<Vec<GenreSummary>> {
    let sql = format!(
        "SELECT genre, id, duration FROM tracks WHERE genre IS NOT NULL
         ORDER BY genre, {}",
        genre_track_order()
    );
    let rows = query_rows(conn, &sql, &[], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i32>>(2)?,
        ))
    })?;

    // 同じジャンルの曲は続けて並ぶため、前の行と比べてまとめる
    let mut genres: Vec<GenreSummary> = Vec::new();
    for (genre, id, duration) in rows {
        match genres.last_mut() {
            Some(last) if last.name == genre => {
                last.track_count += 1;
                last.total_duration += duration.unwrap_or(0);
            }
            _ => genres.push(GenreSummary {
                name: genre,
                track_count: 1,
                total_duration: duration.unwrap_or(0),
                representative_track_id: id,
            }),
        }
    }

    genres.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(genres)
}

/// ジャンルの曲を取得（アーティスト → アルバム → アルバムの中の並び）
pub fn find_genre_tracks(conn: &Connection, genre: &str) -> AppResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE genre = ?1 ORDER BY {}",
        TRACK_COLUMNS,
        genre_track_order()
    );
    query_tracks(conn, &sql, &[&genre])
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
            // アルバムアーティストは、指定された場合だけ変える（編集画面に項目がなく、
            // ファイルのタグも指定された場合だけ書き込むため）
            "UPDATE tracks SET
                title = ?1,
                artist = ?2,
                album = ?3,
                genre = ?4,
                year = ?5,
                album_artist = COALESCE(?6, album_artist),
                updated_at = ?7
             WHERE id = ?8",
            rusqlite::params![
                metadata.title,
                metadata.artist,
                metadata.album,
                metadata.genre,
                metadata.year,
                metadata.album_artist,
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

    if metadata.album_artist.is_some() {
        update_parts.push("album_artist = ?");
        params.push(Box::new(metadata.album_artist.clone()));
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

/// ファイルのタグと比べるための、トラックのDB上の値（編集画面の項目と評価）
#[derive(Debug, Clone, PartialEq)]
pub struct TrackTagValues {
    pub id: String,
    pub file_path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub rating: i32,
}

/// 全トラックの、ファイルのタグと比べる値を取得する（件数の上限なし）
pub fn find_all_track_tag_values(conn: &Connection) -> AppResult<Vec<TrackTagValues>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, title, artist, album, genre, year, COALESCE(rating, 0)
             FROM tracks",
        )
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    stmt.query_map([], |row| {
        Ok(TrackTagValues {
            id: row.get(0)?,
            file_path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            genre: row.get(5)?,
            year: row.get(6)?,
            rating: row.get(7)?,
        })
    })
    .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))
}

/// トラックを新規挿入
///
/// `file_modified_at`はファイルの更新日時（UNIX時間の秒）。再スキャンで変更の検出に使う。
pub fn insert_track(
    conn: &Connection,
    track: &Track,
    file_modified_at: Option<i64>,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO tracks (
            id, file_path, file_name, title, artist, album, genre, year,
            track_number, disc_number, duration, file_size, format, bitrate, sample_rate, created_at, updated_at,
            file_modified_at,
            replay_gain_track_gain, replay_gain_track_peak, replay_gain_album_gain, replay_gain_album_peak,
            rating, album_artist, album_artist_read
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, 1)",
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
            file_modified_at,
            track.replay_gain.track_gain,
            track.replay_gain.track_peak,
            track.replay_gain.album_gain,
            track.replay_gain.album_peak,
            track.rating,
            track.album_artist,
        ],
    )
    .map_err(|e| AppError::Database(format!("トラックの保存に失敗しました: {}", e)))?;
    Ok(())
}

/// file_pathをキーに既存トラックを、ファイルから読んだ内容で更新する
/// （重複時の置き換え・再スキャンで変更を読み直した時・「メタデータを更新」）
///
/// 対象が存在しない場合はエラーを返す（更新したつもりで実際は無変更、を防ぐ）。
/// ファイルを読めたため、見つからない曲にしていた場合は元に戻す。
/// 評価はファイルのタグの値にする。タグで持てないお気に入り・再生回数などは変えない。
pub fn update_track_by_file_path(
    conn: &Connection,
    track: &Track,
    file_modified_at: Option<i64>,
) -> AppResult<()> {
    let rows_affected = conn.execute(
        "UPDATE tracks SET
            file_name = ?2, title = ?3, artist = ?4, album = ?5, genre = ?6, year = ?7,
            track_number = ?8, disc_number = ?9, duration = ?10, file_size = ?11, format = ?12, bitrate = ?13, sample_rate = ?14,
            updated_at = ?15, file_modified_at = ?16,
            replay_gain_track_gain = ?17, replay_gain_track_peak = ?18,
            replay_gain_album_gain = ?19, replay_gain_album_peak = ?20,
            rating = ?21, album_artist = ?22, album_artist_read = 1,
            missing_since = NULL
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
            file_modified_at,
            track.replay_gain.track_gain,
            track.replay_gain.track_peak,
            track.replay_gain.album_gain,
            track.replay_gain.album_peak,
            track.rating,
            track.album_artist,
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

/// アルバムアーティストをまだファイルから読んでいないトラック
#[derive(Debug, PartialEq)]
pub struct UnreadAlbumArtistTrack {
    /// `tracks`のrowid（続きから取得するために使う）
    pub rowid: i64,
    pub id: String,
    pub file_path: String,
}

/// アルバムアーティストをまだファイルから読んでいないトラックを、rowidの順に取得する
///
/// `after_rowid`より後のトラックを、`limit`件まで返す（読めなかったトラックは未読のまま残るため、
/// 件数ではなくrowidで続きを指定する）。
pub fn find_tracks_with_unread_album_artist(
    conn: &Connection,
    after_rowid: i64,
    limit: u32,
) -> AppResult<Vec<UnreadAlbumArtistTrack>> {
    query_rows(
        conn,
        "SELECT rowid, id, file_path FROM tracks
         WHERE album_artist_read = 0 AND rowid > ?1
         ORDER BY rowid LIMIT ?2",
        &[&after_rowid, &limit],
        |row| {
            Ok(UnreadAlbumArtistTrack {
                rowid: row.get(0)?,
                id: row.get(1)?,
                file_path: row.get(2)?,
            })
        },
    )
}

/// ファイルから読んだアルバムアーティストを記録し、読み込み済みにする
///
/// ほかの項目（更新日時を含む）は変えない。
pub fn set_track_album_artist(
    conn: &Connection,
    track_id: &str,
    album_artist: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE tracks SET album_artist = ?2, album_artist_read = 1 WHERE id = ?1",
        rusqlite::params![track_id, album_artist],
    )
    .map_err(|e| AppError::Database(format!("アルバムアーティストの記録に失敗しました: {}", e)))?;
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

/// デバイスへの転送に使うトラックの情報（ファイルのパスと、デバイス上の配置に使うタグ）
#[derive(Debug, Clone, PartialEq)]
pub struct TransferTrack {
    pub id: String,
    pub file_path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub duration: Option<i32>,
}

/// デバイスへの転送用に、全トラックを取得する（件数の上限なし）
///
/// 一覧表示用の`find_all_tracks`は1000件で打ち切るため、転送には使えない。
/// アルバムの曲順に並べる（この順にコピーする。フォルダに書き込んだ順に再生する機器があるため）。
pub fn find_transfer_tracks(conn: &Connection) -> AppResult<Vec<TransferTrack>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, title, artist, album, track_number, disc_number, duration
             FROM tracks
             ORDER BY artist, album, disc_number, track_number, title, id",
        )
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    stmt.query_map([], |row| {
        Ok(TransferTrack {
            id: row.get(0)?,
            file_path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            track_number: row.get(5)?,
            disc_number: row.get(6)?,
            duration: row.get(7)?,
        })
    })
    .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))
}

/// 再スキャンで変更を検出するための、トラックのファイルの状態
#[derive(Debug, Clone, PartialEq)]
pub struct TrackFileState {
    pub id: String,
    pub file_path: String,
    pub file_size: i64,
    /// ファイルの更新日時（UNIX時間の秒）。記録前に登録したトラックはNone
    pub file_modified_at: Option<i64>,
    /// 前の再スキャンでファイルが見つからず、見つからない曲にしているか
    pub is_missing: bool,
}

/// 接頭辞で始まる文字列の範囲の上限（接頭辞の最後の文字を、次に大きい文字にしたもの）
///
/// `file_path`は二分比較（BINARY）のため、接頭辞で始まるパスはちょうど`[接頭辞, 上限)`の
/// 範囲になる。`LIKE`（ASCIIの大文字・小文字を区別しない）や`substr`（インデックスを使えない）の
/// 代わりに、この範囲で`file_path`のインデックス（UNIQUE）を使って検索する。
fn prefix_upper_bound(prefix: &str) -> String {
    let mut chars: Vec<char> = prefix.chars().collect();
    if let Some(last) = chars.pop() {
        let next = (u32::from(last) + 1..=u32::from(char::MAX))
            .find_map(char::from_u32)
            .unwrap_or(char::MAX);
        chars.push(next);
    }
    chars.into_iter().collect()
}

/// 指定した接頭辞（フォルダのパス＋区切り文字）で始まるパスのトラックを取得する
pub fn find_track_file_states_under(
    conn: &Connection,
    path_prefix: &str,
) -> AppResult<Vec<TrackFileState>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, file_size, file_modified_at, missing_since IS NOT NULL
             FROM tracks
             WHERE file_path >= ?1 AND file_path < ?2",
        )
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    stmt.query_map([path_prefix, &prefix_upper_bound(path_prefix)], |row| {
        Ok(TrackFileState {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_size: row.get(2)?,
            file_modified_at: row.get(3)?,
            is_missing: row.get(4)?,
        })
    })
    .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))
}

/// ファイルが見つからなくなったトラックを、見つからない曲にする（すでに見つからない曲なら何もしない）
///
/// トラックは外さない（お気に入り・再生回数・プレイリストへの登録を残す）。
pub fn mark_track_missing(conn: &Connection, track_id: &str, now: &str) -> AppResult<usize> {
    conn.execute(
        "UPDATE tracks SET missing_since = ?2 WHERE id = ?1 AND missing_since IS NULL",
        rusqlite::params![track_id, now],
    )
    .map_err(|e| AppError::Database(format!("トラックの状態の記録に失敗しました: {}", e)))
}

/// ファイルが同じ場所に戻ったトラックを、見つかる曲に戻す
pub fn restore_missing_track(conn: &Connection, track_id: &str) -> AppResult<usize> {
    conn.execute(
        "UPDATE tracks SET missing_since = NULL WHERE id = ?1 AND missing_since IS NOT NULL",
        [track_id],
    )
    .map_err(|e| AppError::Database(format!("トラックの状態の記録に失敗しました: {}", e)))
}

/// ファイルが見つかったパスのトラックを、見つかる曲に戻す（インポートで登録済みのファイルを見つけた時）
pub fn restore_missing_track_by_file_path(conn: &Connection, file_path: &str) -> AppResult<usize> {
    conn.execute(
        "UPDATE tracks SET missing_since = NULL
         WHERE file_path = ?1 AND missing_since IS NOT NULL",
        [file_path],
    )
    .map_err(|e| AppError::Database(format!("トラックの状態の記録に失敗しました: {}", e)))
}

/// 見つからない曲をすべて取得する（移動・改名されたファイルの対応付けの候補）
pub fn find_missing_tracks(conn: &Connection) -> AppResult<Vec<MissingTrack>> {
    query_rows(
        conn,
        "SELECT id, file_name, file_size, file_modified_at, title, artist, album,
                track_number, disc_number, duration
         FROM tracks WHERE missing_since IS NOT NULL",
        &[],
        |row| {
            Ok(MissingTrack {
                id: row.get(0)?,
                file_name: row.get(1)?,
                file_size: row.get(2)?,
                file_modified_at: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                track_number: row.get(7)?,
                disc_number: row.get(8)?,
                duration: row.get(9)?,
            })
        },
    )
}

/// 見つからない曲を、移動・改名された先のファイルに結び付ける
///
/// パスとファイルの更新日時だけを変え、見つかる曲に戻す。トラックのID・お気に入り・再生回数・
/// プレイリストへの登録などは変えない（同じファイルのため、タグの内容も読み直さない）。
pub fn relink_track(
    conn: &Connection,
    track_id: &str,
    file_path: &str,
    file_name: &str,
    file_modified_at: Option<i64>,
) -> AppResult<()> {
    let rows_affected = conn
        .execute(
            "UPDATE tracks SET file_path = ?2, file_name = ?3, file_modified_at = ?4,
                 missing_since = NULL
             WHERE id = ?1",
            rusqlite::params![track_id, file_path, file_name, file_modified_at],
        )
        .map_err(|e| AppError::Database(format!("トラックのパスの更新に失敗しました: {}", e)))?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "結び付けるトラックが見つかりません: {}",
            track_id
        )));
    }
    Ok(())
}

/// 見つからない曲の数
pub fn count_missing_tracks(conn: &Connection) -> AppResult<u32> {
    conn.query_row(
        "SELECT COUNT(*) FROM tracks WHERE missing_since IS NOT NULL",
        [],
        |row| row.get(0),
    )
    .map_err(|e| AppError::Database(format!("トラック数の取得に失敗しました: {}", e)))
}

/// 見つからない曲をすべてライブラリから外す（外した件数を返す）
///
/// プレイリスト・再生履歴の関連レコードは外部キーのCASCADEで消える。
pub fn delete_missing_tracks(conn: &Connection) -> AppResult<usize> {
    conn.execute("DELETE FROM tracks WHERE missing_since IS NOT NULL", [])
        .map_err(|e| AppError::Database(format!("トラックの削除に失敗しました: {}", e)))
}

/// 指定した接頭辞（フォルダのパス＋区切り文字）で始まるパスのトラック数
pub fn count_tracks_under(conn: &Connection, path_prefix: &str) -> AppResult<u32> {
    conn.query_row(
        "SELECT COUNT(*) FROM tracks WHERE file_path >= ?1 AND file_path < ?2",
        [path_prefix, &prefix_upper_bound(path_prefix)],
        |row| row.get(0),
    )
    .map_err(|e| AppError::Database(format!("トラック数の取得に失敗しました: {}", e)))
}

/// 全トラック数
pub fn count_all_tracks(conn: &Connection) -> AppResult<u32> {
    conn.query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
        .map_err(|e| AppError::Database(format!("トラック数の取得に失敗しました: {}", e)))
}

/// 指定した接頭辞（フォルダのパス＋区切り文字）で始まるパスのトラックを削除する
///
/// 削除した件数を返す。プレイリスト・再生履歴の関連レコードは外部キーのCASCADEで消える。
pub fn delete_tracks_under(conn: &Connection, path_prefix: &str) -> AppResult<usize> {
    conn.execute(
        "DELETE FROM tracks WHERE file_path >= ?1 AND file_path < ?2",
        [path_prefix, &prefix_upper_bound(path_prefix)],
    )
    .map_err(|e| AppError::Database(format!("トラックの削除に失敗しました: {}", e)))
}

/// ファイルのサイズと更新日時を記録する（アプリがファイルにタグを書き込んだ後）
pub fn set_track_file_state(
    conn: &Connection,
    track_id: &str,
    file_size: i64,
    file_modified_at: Option<i64>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE tracks SET file_size = ?2, file_modified_at = ?3 WHERE id = ?1",
        rusqlite::params![track_id, file_size, file_modified_at],
    )
    .map_err(|e| AppError::Database(format!("ファイルの状態の記録に失敗しました: {}", e)))?;
    Ok(())
}

/// ファイルの更新日時を記録する（記録前に登録したトラックの補完用）
pub fn set_track_file_modified_at(
    conn: &Connection,
    track_id: &str,
    file_modified_at: i64,
) -> AppResult<()> {
    conn.execute(
        "UPDATE tracks SET file_modified_at = ?2 WHERE id = ?1",
        rusqlite::params![track_id, file_modified_at],
    )
    .map_err(|e| AppError::Database(format!("ファイルの更新日時の記録に失敗しました: {}", e)))?;
    Ok(())
}

/// トラックを1件削除（削除された行数を返す）
///
/// ON DELETE CASCADEにより、playlist_tracksとplay_historyの関連レコードも自動削除される。
pub fn delete_track(conn: &Connection, track_id: &str) -> AppResult<usize> {
    conn.execute("DELETE FROM tracks WHERE id = ?1", [track_id])
        .map_err(|e| AppError::Database(format!("トラックの削除に失敗しました: {}", e)))
}

/// トラックをお気に入りにする・お気に入りから外す（複数のトラックをまとめて）
///
/// お気に入りにした日時も記録する（すでにお気に入りの曲は、元の日時のまま）。再生統計と同じく
/// タグには書かず、`updated_at`も変えない。
/// 見つからないトラックがある場合は`NotFound`で、1曲も変えない。
pub fn set_tracks_favorite(
    conn: &mut Connection,
    track_ids: &[String],
    favorite: bool,
) -> AppResult<()> {
    // 同じIDが2回渡されても、1曲として数える
    let mut unique_ids: Vec<&str> = track_ids.iter().map(String::as_str).collect();
    unique_ids.sort_unstable();
    unique_ids.dedup();

    let now = chrono::Utc::now().to_rfc3339();
    let favorite_value = i32::from(favorite);

    let tx = conn
        .transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;

    let mut updated = 0;
    for chunk in unique_ids.chunks(TRACKS_BY_ID_CHUNK) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        // is_favoriteに値域制約はないため、0以外はすべて「お気に入り」とみなす
        let sql = format!(
            "UPDATE tracks SET
                favorited_at = CASE
                    WHEN ?1 = 0 THEN NULL
                    WHEN COALESCE(is_favorite, 0) != 0 AND favorited_at IS NOT NULL THEN favorited_at
                    ELSE ?2
                END,
                is_favorite = ?1
             WHERE id IN ({placeholders})"
        );
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&favorite_value, &now];
        params.extend(chunk.iter().map(|id| id as &dyn rusqlite::ToSql));
        updated += tx
            .execute(&sql, params.as_slice())
            .map_err(|e| AppError::Database(format!("お気に入りの更新に失敗しました: {}", e)))?;
    }

    if updated != unique_ids.len() {
        // コミットせずに返すため、ここまでの変更は取り消される
        return Err(AppError::NotFound("トラックが見つかりません".to_string()));
    }

    tx.commit()
        .map_err(|e| AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e)))
}

/// レーティングを設定（ファイルのタグへ書き込んだ後に、同じ値を記録する）
///
/// 対象トラックが存在しない場合はエラーを返す。
pub fn set_track_rating(conn: &Connection, track_id: &str, rating: i32) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let rows_affected = conn
        .execute(
            "UPDATE tracks SET rating = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![rating, now, track_id],
        )
        .map_err(|e| AppError::Database(format!("レーティングの更新に失敗しました: {}", e)))?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(
            "指定されたトラックが見つかりません".to_string(),
        ));
    }

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

/// スキップ回数をインクリメントし、新しいスキップ回数を返す
///
/// 再生統計だけの変更のため、`updated_at`は変えない。
pub fn increment_track_skip_count(conn: &Connection, track_id: &str) -> AppResult<i32> {
    conn.query_row(
        "UPDATE tracks SET skip_count = COALESCE(skip_count, 0) + 1
         WHERE id = ?1
         RETURNING skip_count",
        [track_id],
        |row| row.get(0),
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound("トラックが見つかりません".to_string())
        }
        _ => AppError::Database(format!("スキップ回数の更新に失敗しました: {}", e)),
    })
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
    fn test_find_transfer_tracks_orders_by_album_and_track_number() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲2", "B", "アルバム", "ロック");
        insert_test_track(&conn, "t2", "曲1", "B", "アルバム", "ロック");
        insert_test_track(&conn, "t3", "曲", "A", "アルバム", "ロック");
        conn.execute("UPDATE tracks SET track_number = 2 WHERE id = 't1'", [])
            .unwrap();
        conn.execute(
            "UPDATE tracks SET track_number = 1, duration = 200 WHERE id = 't2'",
            [],
        )
        .unwrap();

        let tracks = find_transfer_tracks(&conn).unwrap();

        let ids: Vec<&str> = tracks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["t3", "t2", "t1"]);
        assert_eq!(tracks[1].file_path, "/test/t2.mp3");
        assert_eq!(tracks[1].track_number, Some(1));
        assert_eq!(tracks[1].duration, Some(200));
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
    fn test_find_tracks_by_ids_returns_existing_tracks() {
        let conn = setup_test_db();
        // 1回のクエリの上限（500件）を超える数でも、まとめて取得できる
        let ids: Vec<String> = (0..1_200).map(|i| format!("track-{i}")).collect();
        for id in &ids {
            insert_test_track(&conn, id, "曲", "アーティスト", "アルバム", "ジャンル");
        }
        let mut requested = ids.clone();
        requested.push("missing".to_string());

        let tracks = find_tracks_by_ids(&conn, &requested).unwrap();

        assert_eq!(tracks.len(), 1_200);
        assert_eq!(tracks["track-1199"].file_name, "track-1199.mp3");
        assert!(!tracks.contains_key("missing"));
        assert!(find_tracks_by_ids(&conn, &[]).unwrap().is_empty());
    }

    #[test]
    fn test_find_track_by_id_not_found() {
        let conn = setup_test_db();
        let result = find_track_by_id(&conn, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("見つかりません"));
    }

    #[test]
    fn test_search_matches_each_field() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "夜に駆ける", "YOASOBI", "THE BOOK", "J-POP");
        insert_test_track(&conn, "t2", "群青", "YOASOBI", "THE BOOK 2", "J-POP");
        insert_test_track(&conn, "t3", "Lemon", "米津玄師", "BOOTLEG", "Rock");

        // アーティスト名で検索
        let tracks = search_tracks_by_query(&conn, "YOASOBI").unwrap();
        assert_eq!(tracks.len(), 2);

        // タイトルで検索
        let tracks = search_tracks_by_query(&conn, "Lemon").unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].artist, Some("米津玄師".to_string()));

        // アルバム・ジャンルで検索
        assert_eq!(search_tracks_by_query(&conn, "BOOTLEG").unwrap().len(), 1);
        assert_eq!(search_tracks_by_query(&conn, "J-POP").unwrap().len(), 2);

        // 空の検索語は何も返さない
        assert!(search_tracks_by_query(&conn, "  ").unwrap().is_empty());
    }

    #[test]
    fn test_search_matches_in_the_middle_of_words() {
        let conn = setup_test_db();
        insert_test_track(
            &conn,
            "t1",
            "First Love",
            "宇多田ヒカル",
            "First Love",
            "J-POP",
        );
        insert_test_track(&conn, "t2", "Hey Jude", "The Beatles", "1", "Rock");
        insert_test_track(&conn, "t3", "夜に駆ける", "YOASOBI", "THE BOOK", "J-POP");

        // 区切りのない日本語の途中（3文字以上: 索引で探す）
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "ヒカル").unwrap()),
            ["t1"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "駆ける").unwrap()),
            ["t3"]
        );
        // 単語の先頭・途中
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "beat").unwrap()),
            ["t2"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "atle").unwrap()),
            ["t2"]
        );
        // 3文字未満の検索語（索引では探せないため、走査する）
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "宇多").unwrap()),
            ["t1"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "夜").unwrap()),
            ["t3"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "ey").unwrap()),
            ["t2"]
        );
        // 項目をまたいだ一致は起こさない（タイトルの終わりとアーティストの始まり）
        assert!(search_tracks_by_query(&conn, "judethe").unwrap().is_empty());
        assert!(search_tracks_by_query(&conn, "ey").unwrap().len() == 1);
        assert!(search_tracks_by_query(&conn, "eth").unwrap().is_empty());
    }

    #[test]
    fn test_search_ignores_case_width_and_kana() {
        let conn = setup_test_db();
        insert_test_track(
            &conn,
            "t1",
            "First Love",
            "宇多田ヒカル",
            "First Love",
            "J-POP",
        );
        insert_test_track(&conn, "t2", "ｶﾌﾞﾄﾑｼ", "ａｉｋｏ", "桜の木の下", "J-POP");
        insert_test_track(
            &conn,
            "t3",
            "さくら",
            "ケツメイシ",
            "ケツノポリス4",
            "J-POP",
        );

        // 大文字と小文字
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "FIRST love").unwrap()),
            ["t1"]
        );
        // ひらがなとカタカナ（どちらで入力しても、どちらで書かれた曲も見つかる）
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "ひかる").unwrap()),
            ["t1"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "サクラ").unwrap()),
            ["t3"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "けつめいし").unwrap()),
            ["t3"]
        );
        // 全角と半角（タグが全角の英字・半角のカタカナ）
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "aiko").unwrap()),
            ["t2"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "カブトムシ").unwrap()),
            ["t2"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "かぶと").unwrap()),
            ["t2"]
        );
        // 検索語が全角の英字・半角のカタカナ
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "ｆｉｒｓｔ").unwrap()),
            ["t1"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "ﾋｶﾙ").unwrap()),
            ["t1"]
        );
        // 2文字（走査）でも同じ
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "ＡＩ").unwrap()),
            ["t2"]
        );
    }

    #[test]
    fn test_search_requires_all_terms() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "Help!", "The Beatles", "Help!", "Rock");
        insert_test_track(&conn, "t2", "Hey Jude", "The Beatles", "1", "Rock");
        insert_test_track(&conn, "t3", "Help Me", "Other", "Album", "Pop");

        // 空白で区切った語を、すべて含む曲（項目が違ってもよい）
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "beatles help").unwrap()),
            ["t1"]
        );
        // 全角の空白でも区切る。3文字未満の語が混ざる場合は、すべての語を走査で探す
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "beatles　he").unwrap()).len(),
            2
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "he me").unwrap()),
            ["t3"]
        );
        assert!(
            search_tracks_by_query(&conn, "beatles other")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_escape_like_pattern() {
        assert_eq!(escape_like_pattern("normal"), "normal");
        assert_eq!(escape_like_pattern("50%"), "50\\%");
        assert_eq!(escape_like_pattern("a_b"), "a\\_b");
        assert_eq!(escape_like_pattern("a\\b"), "a\\\\b");
    }

    /// 検索語の記号が、ワイルドカードや全文検索の構文として解釈されないことを検証する
    #[test]
    fn test_search_treats_symbols_literally() {
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
        insert_test_track(
            &conn,
            "t5",
            "Rock \"n\" Roll",
            "AC-DC",
            "NOT OR AND",
            "ロック",
        );

        // `%`はリテラルとして扱われ、"50XOFF"にはマッチしない（索引・走査のどちらでも）
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "50%O").unwrap()),
            ["t1"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "0%").unwrap()),
            ["t1"]
        );
        // `_`もリテラルとして扱われ、"axb"にはマッチしない
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "a_b").unwrap()),
            ["t3"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "_b").unwrap()),
            ["t3"]
        );
        // 通常の検索語は部分一致する
        assert_eq!(search_tracks_by_query(&conn, "OFF").unwrap().len(), 2);

        // 全文検索の演算子・記号・二重引用符も、文字として探す
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "AC-DC").unwrap()),
            ["t5"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "NOT OR AND").unwrap()),
            ["t5"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "NOT AND").unwrap()),
            ["t5"]
        );
        assert!(
            search_tracks_by_query(&conn, "NOT roll*")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "\"n\" roll").unwrap()),
            ["t5"]
        );
        assert!(search_tracks_by_query(&conn, "roll*").unwrap().is_empty());
        assert!(
            search_tracks_by_query(&conn, "text:rock")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_search_follows_updates_and_deletes() {
        let conn = setup_test_db();
        insert_test_track(
            &conn,
            "t1",
            "古いタイトル",
            "アーティスト",
            "アルバム",
            "ロック",
        );

        conn.execute(
            "UPDATE tracks SET title = '新しいタイトル' WHERE id = 't1'",
            [],
        )
        .unwrap();
        assert!(
            search_tracks_by_query(&conn, "古いタイトル")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            search_tracks_by_query(&conn, "新しいタイトル")
                .unwrap()
                .len(),
            1
        );

        // 検索の対象ではない項目の更新では、検索の結果は変わらない
        conn.execute("UPDATE tracks SET play_count = 5 WHERE id = 't1'", [])
            .unwrap();
        assert_eq!(
            search_tracks_by_query(&conn, "新しいタイトル")
                .unwrap()
                .len(),
            1
        );

        delete_track(&conn, "t1").unwrap();
        assert!(
            search_tracks_by_query(&conn, "新しいタイトル")
                .unwrap()
                .is_empty()
        );
        assert!(search_tracks_by_query(&conn, "新").unwrap().is_empty());
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
    fn test_find_favorite_tracks_newest_first() {
        let mut conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");
        insert_test_track(&conn, "t3", "曲C", "アーティストZ", "アルバム3", "ジャズ");
        assert!(find_favorite_tracks(&conn).unwrap().is_empty());

        set_tracks_favorite(&mut conn, &["t1".to_string()], true).unwrap();
        // 後からお気に入りにした曲を先にする
        conn.execute(
            "UPDATE tracks SET favorited_at = '2026-01-01T00:00:00+00:00' WHERE id = 't1'",
            [],
        )
        .unwrap();
        set_tracks_favorite(&mut conn, &["t3".to_string()], true).unwrap();

        let tracks = find_favorite_tracks(&conn).unwrap();
        assert_eq!(track_ids(&tracks), vec!["t3", "t1"]);
        assert!(tracks.iter().all(|track| track.is_favorite));
    }

    #[test]
    fn test_find_favorite_tracks_keeps_album_order_for_tracks_favorited_together() {
        let mut conn = setup_test_db();
        for (id, title) in [("t1", "曲C"), ("t2", "曲A"), ("t3", "曲B")] {
            insert_test_track(&conn, id, title, "アーティストX", "アルバム1", "ロック");
        }
        set_track_position(&conn, "t1", Some(1), Some(3), None);
        set_track_position(&conn, "t2", Some(1), Some(1), None);
        set_track_position(&conn, "t3", Some(1), Some(2), None);

        // アルバムをまとめてお気に入りにした場合は、アルバムの中の順に並べる
        let ids: Vec<String> = ["t1", "t2", "t3"].iter().map(|id| id.to_string()).collect();
        set_tracks_favorite(&mut conn, &ids, true).unwrap();

        let tracks = find_favorite_tracks(&conn).unwrap();
        assert_eq!(track_ids(&tracks), vec!["t2", "t3", "t1"]);
    }

    #[test]
    fn test_set_tracks_favorite() {
        let mut conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");
        let before = find_track_by_id(&conn, "t1").unwrap();
        let favorited_at = |conn: &Connection, id: &str| -> Option<String> {
            conn.query_row(
                "SELECT favorited_at FROM tracks WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap()
        };

        // 同じIDを2回渡しても、1曲として扱う
        let ids = vec!["t1".to_string(), "t2".to_string(), "t1".to_string()];
        set_tracks_favorite(&mut conn, &ids, true).unwrap();
        let track = find_track_by_id(&conn, "t1").unwrap();
        assert!(track.is_favorite);
        assert!(find_track_by_id(&conn, "t2").unwrap().is_favorite);
        // タグに書かない項目のため、更新日時は変えない
        assert_eq!(track.updated_at, before.updated_at);

        // すでにお気に入りの曲は、お気に入りにした日時を変えない
        conn.execute(
            "UPDATE tracks SET favorited_at = '2026-01-01T00:00:00+00:00' WHERE id = 't1'",
            [],
        )
        .unwrap();
        set_tracks_favorite(&mut conn, &ids, true).unwrap();
        assert_eq!(
            favorited_at(&conn, "t1").as_deref(),
            Some("2026-01-01T00:00:00+00:00")
        );

        // 外すと、お気に入りにした日時も消す
        set_tracks_favorite(&mut conn, &["t1".to_string()], false).unwrap();
        assert!(!find_track_by_id(&conn, "t1").unwrap().is_favorite);
        assert_eq!(favorited_at(&conn, "t1"), None);
        assert!(find_track_by_id(&conn, "t2").unwrap().is_favorite);

        // 何も渡さなければ、何もしない
        set_tracks_favorite(&mut conn, &[], true).unwrap();
    }

    #[test]
    fn test_set_tracks_favorite_changes_nothing_when_a_track_is_missing() {
        let mut conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        let ids = vec!["t1".to_string(), "nonexistent".to_string()];
        let result = set_tracks_favorite(&mut conn, &ids, true);

        assert!(matches!(result, Err(AppError::NotFound(_))));
        assert!(!find_track_by_id(&conn, "t1").unwrap().is_favorite);
    }

    /// is_favoriteに0/1以外が入っていても、お気に入りとして扱う
    ///
    /// カラムに値域制約がないため、外部要因で異常値が入り得る前提で検証する。
    #[test]
    fn test_favorite_with_unexpected_value() {
        let mut conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        conn.execute("UPDATE tracks SET is_favorite = 2 WHERE id = 't1'", [])
            .unwrap();

        assert!(find_track_by_id(&conn, "t1").unwrap().is_favorite);
        assert_eq!(find_favorite_tracks(&conn).unwrap().len(), 1);

        set_tracks_favorite(&mut conn, &["t1".to_string()], false).unwrap();
        let stored: i32 = conn
            .query_row(
                "SELECT is_favorite FROM tracks WHERE id = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, 0);
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
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("トラックが見つかりません")
        );
    }

    #[test]
    fn test_increment_track_skip_count() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        let before = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(before.skip_count, 0);

        // 戻り値は更新後のスキップ回数
        assert_eq!(increment_track_skip_count(&conn, "t1").unwrap(), 1);
        assert_eq!(increment_track_skip_count(&conn, "t1").unwrap(), 2);

        // 再生回数・最後に再生した日時・再生履歴・更新日時は変えない
        let track = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(track.skip_count, 2);
        assert_eq!(track.play_count, 0);
        assert!(track.last_played_at.is_none());
        assert_eq!(track.updated_at, before.updated_at);
        assert!(find_play_history(&conn).unwrap().is_empty());

        let result = increment_track_skip_count(&conn, "nonexistent");
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    #[test]
    fn test_find_play_history_returns_every_play_newest_first() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "アルバム2", "ポップ");
        assert!(find_play_history(&conn).unwrap().is_empty());

        for (track_id, played_at) in [
            ("t1", "2026-10-01T10:00:00+00:00"),
            ("t2", "2026-10-02T09:00:00+00:00"),
            ("t1", "2026-10-02T09:05:00+00:00"),
            // 同じ日時なら、後から記録したほうを先にする
            ("t2", "2026-10-02T09:05:00+00:00"),
        ] {
            conn.execute(
                "INSERT INTO play_history (track_id, played_at) VALUES (?1, ?2)",
                [track_id, played_at],
            )
            .unwrap();
        }

        let history = find_play_history(&conn).unwrap();
        let plays: Vec<(&str, &str)> = history
            .iter()
            .map(|entry| (entry.track_id.as_str(), &entry.played_at[..16]))
            .collect();
        assert_eq!(
            plays,
            vec![
                ("t2", "2026-10-02T09:05"),
                ("t1", "2026-10-02T09:05"),
                ("t2", "2026-10-02T09:00"),
                ("t1", "2026-10-01T10:00"),
            ]
        );
        // 行を見分けるIDは、すべて違う
        let mut ids: Vec<i64> = history.iter().map(|entry| entry.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 4);

        // 再生回数に数えると、履歴の先頭に加わる
        increment_track_play_count(&conn, "t1").unwrap();
        let history = find_play_history(&conn).unwrap();
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].track_id, "t1");

        // ライブラリから外した曲の履歴は消える
        conn.execute("DELETE FROM tracks WHERE id = 't1'", [])
            .unwrap();
        let history = find_play_history(&conn).unwrap();
        assert!(history.iter().all(|entry| entry.track_id == "t2"));
        assert_eq!(history.len(), 2);
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

        // 同じ回数なら、最近再生した曲を先にする
        insert_test_track(&conn, "t3", "曲C", "アーティストZ", "アルバム3", "ジャズ");
        conn.execute(
            "UPDATE tracks SET play_count = 5, last_played_at = '2026-10-02T00:00:00+00:00' WHERE id = 't3'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE tracks SET last_played_at = '2026-10-01T00:00:00+00:00' WHERE id = 't2'",
            [],
        )
        .unwrap();
        let tracks = find_most_played_tracks(&conn, 10).unwrap();
        assert_eq!(track_ids(&tracks), vec!["t1", "t3", "t2"]);
    }

    /// 一覧の集計のテスト用に、ディスク番号・トラック番号・長さを設定する
    fn set_track_position(
        conn: &Connection,
        id: &str,
        disc_number: Option<i32>,
        track_number: Option<i32>,
        duration: Option<i32>,
    ) {
        conn.execute(
            "UPDATE tracks SET disc_number = ?2, track_number = ?3, duration = ?4 WHERE id = ?1",
            rusqlite::params![id, disc_number, track_number, duration],
        )
        .unwrap();
    }

    fn track_ids(tracks: &[Track]) -> Vec<&str> {
        tracks.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn test_find_all_tracks_has_no_limit() {
        let conn = setup_test_db();
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..1500 {
            insert_test_track(
                &conn,
                &format!("t{i}"),
                "曲",
                "アーティスト",
                "アルバム",
                "ロック",
            );
        }
        conn.execute_batch("COMMIT").unwrap();

        assert_eq!(find_all_tracks(&conn).unwrap().len(), 1500);
        // 走査（3文字未満）と索引（3文字以上）のどちらの検索でも、すべて返す
        assert_eq!(search_tracks_by_query(&conn, "曲").unwrap().len(), 1500);
        assert_eq!(
            search_tracks_by_query(&conn, "アーティスト").unwrap().len(),
            1500
        );
        let filters = FilterOptions {
            artist: None,
            album: None,
            genre: Some("ロック".to_string()),
        };
        assert_eq!(find_tracks_by_filter(&conn, &filters).unwrap().len(), 1500);

        conn.execute("UPDATE tracks SET is_favorite = 1", [])
            .unwrap();
        assert_eq!(find_favorite_tracks(&conn).unwrap().len(), 1500);
    }

    #[test]
    fn test_find_album_summaries() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストY", "beta", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストY", "beta", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストY", "Alpha", "ポップ");
        set_track_position(&conn, "t1", Some(1), Some(2), Some(100));
        set_track_position(&conn, "t2", Some(1), Some(1), Some(50));
        set_track_position(&conn, "t3", None, None, None);
        // アルバムのない曲は一覧に含めない
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, format, file_size) VALUES ('t4', '/test/t4.mp3', 't4.mp3', 'mp3', 1)",
            [],
        )
        .unwrap();

        let albums = find_album_summaries(&conn).unwrap();

        // 大文字・小文字を区別しない名前の順
        assert_eq!(
            albums,
            [
                AlbumSummary {
                    name: "Alpha".to_string(),
                    artist: Some("アーティストY".to_string()),
                    track_count: 1,
                    total_duration: 0,
                    representative_track_id: "t3".to_string(),
                },
                // 代表の曲は、アルバムの最初の曲（トラック番号の順）
                AlbumSummary {
                    name: "beta".to_string(),
                    artist: Some("アーティストY".to_string()),
                    track_count: 2,
                    total_duration: 150,
                    representative_track_id: "t2".to_string(),
                },
            ]
        );
    }

    #[test]
    fn test_find_album_tracks_orders_by_disc_and_track_number() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t4", "曲D", "アーティストX", "アルバム2", "ロック");
        set_track_position(&conn, "t1", Some(2), Some(1), None);
        set_track_position(&conn, "t2", Some(1), Some(2), None);
        // ディスク番号のない曲は、ディスク1として並べる
        set_track_position(&conn, "t3", None, Some(1), None);

        let tracks = find_album_tracks(&conn, "アルバム1", Some("アーティストX")).unwrap();

        assert_eq!(track_ids(&tracks), ["t3", "t2", "t1"]);
        assert!(
            find_album_tracks(&conn, "ないアルバム", Some("アーティストX"))
                .unwrap()
                .is_empty()
        );
        assert!(
            find_album_tracks(&conn, "アルバム1", Some("別のアーティスト"))
                .unwrap()
                .is_empty()
        );
    }

    fn set_album_artist(conn: &Connection, id: &str, album_artist: &str) {
        conn.execute(
            "UPDATE tracks SET album_artist = ?2 WHERE id = ?1",
            rusqlite::params![id, album_artist],
        )
        .unwrap();
    }

    #[test]
    fn test_albums_are_grouped_by_album_artist_and_name() {
        let conn = setup_test_db();
        // 別のアーティストの、同じ名前のアルバム
        insert_test_track(
            &conn,
            "a1",
            "曲",
            "アーティストA",
            "Greatest Hits",
            "ロック",
        );
        insert_test_track(
            &conn,
            "b1",
            "曲",
            "アーティストB",
            "Greatest Hits",
            "ロック",
        );
        // コンピレーション（曲ごとのアーティストが違い、アルバムアーティストが同じ）
        insert_test_track(&conn, "c1", "曲1", "アーティストA", "Compilation", "ロック");
        insert_test_track(&conn, "c2", "曲2", "アーティストB", "Compilation", "ロック");
        set_album_artist(&conn, "c1", "Various Artists");
        set_album_artist(&conn, "c2", "Various Artists");
        // フィーチャリング（アルバムアーティストは主のアーティスト）
        insert_test_track(&conn, "f1", "曲1", "アーティストA", "Album", "ロック");
        insert_test_track(
            &conn,
            "f2",
            "曲2",
            "アーティストA feat. B",
            "Album",
            "ロック",
        );
        set_album_artist(&conn, "f1", "アーティストA");
        set_album_artist(&conn, "f2", "アーティストA");
        // アルバムアーティストも曲のアーティストもないアルバム
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, album, format, file_size) VALUES ('n1', '/test/n1.mp3', 'n1.mp3', 'No Artist', 'mp3', 1)",
            [],
        )
        .unwrap();

        let albums = find_album_summaries(&conn).unwrap();

        let summary: Vec<(&str, Option<&str>, i32)> = albums
            .iter()
            .map(|a| (a.name.as_str(), a.artist.as_deref(), a.track_count))
            .collect();
        assert_eq!(
            summary,
            [
                ("Album", Some("アーティストA"), 2),
                ("Compilation", Some("Various Artists"), 2),
                ("Greatest Hits", Some("アーティストA"), 1),
                ("Greatest Hits", Some("アーティストB"), 1),
                ("No Artist", None, 1),
            ]
        );

        // アルバムの曲は、アルバムをまとめたアーティストで取得する
        let compilation = find_album_tracks(&conn, "Compilation", Some("Various Artists")).unwrap();
        assert_eq!(track_ids(&compilation), ["c1", "c2"]);
        let hits_b = find_album_tracks(&conn, "Greatest Hits", Some("アーティストB")).unwrap();
        assert_eq!(track_ids(&hits_b), ["b1"]);
        let no_artist = find_album_tracks(&conn, "No Artist", None).unwrap();
        assert_eq!(track_ids(&no_artist), ["n1"]);
        // 曲のアーティストでは取得しない（アルバムアーティストがある曲）
        assert!(
            find_album_tracks(&conn, "Compilation", Some("アーティストA"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_artists_are_grouped_by_album_artist() {
        let conn = setup_test_db();
        insert_test_track(&conn, "a1", "曲", "アーティストA", "Solo", "ロック");
        insert_test_track(&conn, "c1", "曲1", "アーティストA", "Compilation", "ロック");
        insert_test_track(&conn, "c2", "曲2", "アーティストB", "Compilation", "ロック");
        set_album_artist(&conn, "c1", "Various Artists");
        set_album_artist(&conn, "c2", "Various Artists");
        insert_test_track(&conn, "f1", "曲", "アーティストA feat. B", "Solo", "ロック");
        set_album_artist(&conn, "f1", "アーティストA");

        let artists = find_artist_summaries(&conn).unwrap();

        // コンピレーションの参加アーティスト（B）・フィーチャリングの表記は、一覧に並ばない
        let summary: Vec<(&str, i32, i32)> = artists
            .iter()
            .map(|a| (a.name.as_str(), a.album_count, a.track_count))
            .collect();
        assert_eq!(
            summary,
            [("Various Artists", 1, 2), ("アーティストA", 1, 2)]
        );

        // 曲ごとのアーティストが違う曲も、アルバムアーティストのアルバムに入る
        let albums = find_artist_albums(&conn, "アーティストA").unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(track_ids(&albums[0].tracks), ["a1", "f1"]);
        assert_eq!(
            albums[0].tracks[1].artist.as_deref(),
            Some("アーティストA feat. B")
        );
        assert_eq!(
            albums[0].tracks[1].album_artist.as_deref(),
            Some("アーティストA")
        );
        let various = find_artist_albums(&conn, "Various Artists").unwrap();
        assert_eq!(track_ids(&various[0].tracks), ["c1", "c2"]);
        assert!(
            find_artist_albums(&conn, "アーティストB")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_search_matches_album_artist() {
        let conn = setup_test_db();
        insert_test_track(&conn, "c1", "曲1", "アーティストA", "Compilation", "ロック");
        insert_test_track(&conn, "x1", "曲2", "アーティストB", "Other", "ロック");
        set_album_artist(&conn, "c1", "Various Artists");

        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "Various").unwrap()),
            ["c1"]
        );
        assert_eq!(
            track_ids(&search_tracks_by_query(&conn, "ar").unwrap()),
            ["c1"]
        );
    }

    #[test]
    fn test_insert_and_update_record_album_artist() {
        let conn = setup_test_db();
        let mut track = Track {
            id: "t1".to_string(),
            file_path: "/test/t1.mp3".to_string(),
            file_name: "t1.mp3".to_string(),
            title: Some("曲".to_string()),
            artist: Some("アーティストA".to_string()),
            album: Some("アルバム".to_string()),
            album_artist: Some("Various Artists".to_string()),
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration: None,
            file_size: 1,
            format: "mp3".to_string(),
            bitrate: None,
            sample_rate: None,
            is_favorite: false,
            rating: 0,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            replay_gain: ReplayGain::default(),
            is_missing: false,
        };
        insert_track(&conn, &track, None).unwrap();

        let saved = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(saved.album_artist.as_deref(), Some("Various Artists"));
        // ファイルから読んで登録したトラックは、読み込み済みにする
        assert!(
            find_tracks_with_unread_album_artist(&conn, 0, 10)
                .unwrap()
                .is_empty()
        );

        // ファイルを読み直した時は、タグの内容にする（タグがなくなった場合は消す）
        track.album_artist = None;
        update_track_by_file_path(&conn, &track, None).unwrap();
        assert_eq!(find_track_by_id(&conn, "t1").unwrap().album_artist, None);
    }

    #[test]
    fn test_update_track_metadata_keeps_album_artist_unless_given() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲", "アーティストA", "アルバム", "ロック");
        set_album_artist(&conn, "t1", "Various Artists");
        let mut metadata = Metadata {
            title: Some("新しいタイトル".to_string()),
            artist: Some("アーティストA".to_string()),
            album: Some("アルバム".to_string()),
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            album_artist: None,
            composer: None,
        };

        // 編集画面はアルバムアーティストを送らないため、今の値を保つ
        update_track_metadata(&conn, "t1", &metadata).unwrap();
        let track = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(track.title.as_deref(), Some("新しいタイトル"));
        assert_eq!(track.album_artist.as_deref(), Some("Various Artists"));

        metadata.album_artist = Some("アーティストA".to_string());
        update_track_metadata(&conn, "t1", &metadata).unwrap();
        assert_eq!(
            find_track_by_id(&conn, "t1")
                .unwrap()
                .album_artist
                .as_deref(),
            Some("アーティストA")
        );

        let partial = Metadata {
            title: None,
            artist: None,
            album: None,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            album_artist: Some("Various Artists".to_string()),
            composer: None,
        };
        update_track_metadata_partial(&conn, "t1", &partial, "2026-01-02T00:00:00Z").unwrap();
        assert_eq!(
            find_track_by_id(&conn, "t1")
                .unwrap()
                .album_artist
                .as_deref(),
            Some("Various Artists")
        );
    }

    #[test]
    fn test_missing_tracks_are_kept_and_can_be_restored() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲1", "アーティスト", "アルバム", "ロック");
        insert_test_track(&conn, "t2", "曲2", "アーティスト", "アルバム", "ロック");
        conn.execute("UPDATE tracks SET play_count = 7 WHERE id = 't1'", [])
            .unwrap();

        assert_eq!(
            mark_track_missing(&conn, "t1", "2026-01-02T00:00:00Z").unwrap(),
            1
        );
        // すでに見つからない曲なら、日時を上書きしない
        assert_eq!(
            mark_track_missing(&conn, "t1", "2026-01-03T00:00:00Z").unwrap(),
            0
        );

        // 見つからない曲は外さず、一覧にも残す
        let tracks = find_all_tracks(&conn).unwrap();
        assert_eq!(tracks.len(), 2);
        let t1 = tracks.iter().find(|t| t.id == "t1").unwrap();
        assert!(t1.is_missing);
        assert_eq!(t1.play_count, 7);
        assert!(!tracks.iter().find(|t| t.id == "t2").unwrap().is_missing);
        assert_eq!(count_missing_tracks(&conn).unwrap(), 1);
        let states = find_track_file_states_under(&conn, "/test/").unwrap();
        assert!(states.iter().find(|t| t.id == "t1").unwrap().is_missing);

        let missing = find_missing_tracks(&conn).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].id, "t1");
        assert_eq!(missing[0].file_name, "t1.mp3");
        assert_eq!(missing[0].title.as_deref(), Some("曲1"));

        // ファイルが同じ場所に戻った
        assert_eq!(restore_missing_track(&conn, "t1").unwrap(), 1);
        assert_eq!(restore_missing_track(&conn, "t1").unwrap(), 0);
        assert!(!find_track_by_id(&conn, "t1").unwrap().is_missing);

        // インポートで登録済みのファイルを見つけた
        mark_track_missing(&conn, "t2", "2026-01-02T00:00:00Z").unwrap();
        assert_eq!(
            restore_missing_track_by_file_path(&conn, "/test/t2.mp3").unwrap(),
            1
        );
        assert_eq!(count_missing_tracks(&conn).unwrap(), 0);
    }

    #[test]
    fn test_relink_track_keeps_id_stats_and_playlists() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲1", "アーティスト", "アルバム", "ロック");
        conn.execute(
            "UPDATE tracks SET play_count = 7, is_favorite = 1, rating = 4 WHERE id = 't1'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlists (id, name) VALUES ('p1', 'プレイリスト')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES ('p1', 't1', 0)",
            [],
        )
        .unwrap();
        mark_track_missing(&conn, "t1", "2026-01-02T00:00:00Z").unwrap();

        relink_track(
            &conn,
            "t1",
            "/test/moved/renamed.mp3",
            "renamed.mp3",
            Some(99),
        )
        .unwrap();

        let track = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(track.file_path, "/test/moved/renamed.mp3");
        assert_eq!(track.file_name, "renamed.mp3");
        assert!(!track.is_missing);
        // お気に入り・再生回数・評価・タグの内容は変えない
        assert_eq!(track.play_count, 7);
        assert!(track.is_favorite);
        assert_eq!(track.rating, 4);
        assert_eq!(track.title.as_deref(), Some("曲1"));
        // プレイリストへの登録も残る
        let in_playlist: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE track_id = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(in_playlist, 1);
        let states = find_track_file_states_under(&conn, "/test/").unwrap();
        assert_eq!(states[0].file_modified_at, Some(99));

        assert!(matches!(
            relink_track(&conn, "missing", "/test/x.mp3", "x.mp3", None),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn test_delete_missing_tracks_removes_only_missing_tracks() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲1", "アーティスト", "アルバム", "ロック");
        insert_test_track(&conn, "t2", "曲2", "アーティスト", "アルバム", "ロック");
        insert_test_track(&conn, "t3", "曲3", "アーティスト", "アルバム", "ロック");
        mark_track_missing(&conn, "t1", "2026-01-02T00:00:00Z").unwrap();
        mark_track_missing(&conn, "t3", "2026-01-02T00:00:00Z").unwrap();

        assert_eq!(delete_missing_tracks(&conn).unwrap(), 2);

        assert_eq!(track_ids(&find_all_tracks(&conn).unwrap()), ["t2"]);
        assert_eq!(delete_missing_tracks(&conn).unwrap(), 0);
    }

    #[test]
    fn test_update_track_by_file_path_restores_missing_track() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲1", "アーティスト", "アルバム", "ロック");
        mark_track_missing(&conn, "t1", "2026-01-02T00:00:00Z").unwrap();
        let mut track = find_track_by_id(&conn, "t1").unwrap();
        track.title = Some("読み直したタイトル".to_string());

        // ファイルを読み直せたトラックは、見つかる曲に戻す
        update_track_by_file_path(&conn, &track, Some(1)).unwrap();

        let updated = find_track_by_id(&conn, "t1").unwrap();
        assert!(!updated.is_missing);
        assert_eq!(updated.title.as_deref(), Some("読み直したタイトル"));
    }

    #[test]
    fn test_find_tracks_with_unread_album_artist_pages_by_rowid() {
        let conn = setup_test_db();
        // 列を追加する前に登録したトラック（`insert_test_track`は読み込み済みにしない）
        for id in ["t1", "t2", "t3"] {
            insert_test_track(&conn, id, "曲", "アーティスト", "アルバム", "ロック");
        }

        let first = find_tracks_with_unread_album_artist(&conn, 0, 2).unwrap();
        assert_eq!(
            first.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["t1", "t2"]
        );
        assert_eq!(first[0].file_path, "/test/t1.mp3");

        set_track_album_artist(&conn, "t1", Some("Various Artists")).unwrap();
        let rest = find_tracks_with_unread_album_artist(&conn, first[1].rowid, 2).unwrap();
        assert_eq!(
            rest.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["t3"]
        );
        // 読み込み済みのトラックは、最初から取得し直しても含まれない
        let again = find_tracks_with_unread_album_artist(&conn, 0, 10).unwrap();
        assert_eq!(
            again.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["t2", "t3"]
        );
        assert_eq!(
            find_track_by_id(&conn, "t1")
                .unwrap()
                .album_artist
                .as_deref(),
            Some("Various Artists")
        );
    }

    #[test]
    fn test_find_artist_summaries() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "beta", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "Alpha", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストX", "Alpha", "ロック");
        insert_test_track(&conn, "t4", "曲D", "アーティストY", "beta", "ポップ");
        set_track_position(&conn, "t1", None, Some(1), Some(10));
        set_track_position(&conn, "t2", None, Some(2), Some(20));
        set_track_position(&conn, "t3", None, Some(1), Some(30));
        // アルバムのない曲は、1つのアルバムとして数える
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, artist, format, file_size) VALUES ('t5', '/test/t5.mp3', 't5.mp3', 'アーティストX', 'mp3', 1)",
            [],
        )
        .unwrap();

        let artists = find_artist_summaries(&conn).unwrap();

        assert_eq!(
            artists,
            [
                // 代表の曲は、名前の順で最初のアルバム（Alpha）の最初の曲
                ArtistSummary {
                    name: "アーティストX".to_string(),
                    album_count: 3,
                    track_count: 4,
                    total_duration: 60,
                    representative_track_id: "t3".to_string(),
                },
                ArtistSummary {
                    name: "アーティストY".to_string(),
                    album_count: 1,
                    track_count: 1,
                    total_duration: 0,
                    representative_track_id: "t4".to_string(),
                },
            ]
        );
    }

    #[test]
    fn test_find_artist_albums() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "beta", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "Alpha", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストX", "Alpha", "ロック");
        insert_test_track(&conn, "t4", "曲D", "アーティストY", "Alpha", "ポップ");
        set_track_position(&conn, "t2", None, Some(2), Some(20));
        set_track_position(&conn, "t3", None, Some(1), Some(30));
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, artist, format, file_size) VALUES ('t5', '/test/t5.mp3', 't5.mp3', 'アーティストX', 'mp3', 1)",
            [],
        )
        .unwrap();

        let albums = find_artist_albums(&conn, "アーティストX").unwrap();

        let names: Vec<&str> = albums.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", UNKNOWN_ALBUM]);
        assert_eq!(track_ids(&albums[0].tracks), ["t3", "t2"]);
        assert_eq!(albums[0].track_count, 2);
        assert_eq!(albums[0].total_duration, 50);
        assert_eq!(albums[0].representative_track_id, "t3");
        assert_eq!(albums[0].artist.as_deref(), Some("アーティストX"));
        assert_eq!(track_ids(&albums[2].tracks), ["t5"]);

        // 一覧の件数・代表の曲と一致する
        let summary = &find_artist_summaries(&conn).unwrap()[0];
        assert_eq!(summary.album_count as usize, albums.len());
        assert_eq!(
            summary.representative_track_id,
            albums[0].representative_track_id
        );
        assert!(find_artist_albums(&conn, "いない").unwrap().is_empty());
    }

    #[test]
    fn test_find_genre_summaries_and_tracks() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストY", "アルバム1", "ロック");
        insert_test_track(&conn, "t2", "曲B", "アーティストX", "アルバム2", "ロック");
        insert_test_track(&conn, "t3", "曲C", "アーティストX", "アルバム1", "ロック");
        insert_test_track(&conn, "t4", "曲D", "アーティストY", "アルバム2", "ポップ");
        set_track_position(&conn, "t1", None, None, Some(10));
        set_track_position(&conn, "t2", None, None, Some(20));
        set_track_position(&conn, "t3", None, None, Some(30));

        let genres = find_genre_summaries(&conn).unwrap();

        assert_eq!(
            genres,
            [
                GenreSummary {
                    name: "ポップ".to_string(),
                    track_count: 1,
                    total_duration: 0,
                    representative_track_id: "t4".to_string(),
                },
                // 代表の曲は、アーティスト → アルバムの順で最初の曲
                GenreSummary {
                    name: "ロック".to_string(),
                    track_count: 3,
                    total_duration: 60,
                    representative_track_id: "t3".to_string(),
                },
            ]
        );

        let tracks = find_genre_tracks(&conn, "ロック").unwrap();
        assert_eq!(track_ids(&tracks), ["t3", "t2", "t1"]);
        assert!(find_genre_tracks(&conn, "ないジャンル").unwrap().is_empty());
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
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("トラックが見つかりません")
        );

        // 存在しないID + 更新フィールドなし → NotFound
        let result = update_track_metadata_partial(
            &conn,
            "nonexistent",
            &empty_metadata(),
            "2026-01-01T00:00:00+00:00",
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("トラックが見つかりません")
        );
    }

    /// 存在しないfile_pathへの更新は成功扱いにせずエラーにする
    #[test]
    fn test_update_track_by_file_path_not_found() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        let mut track = find_track_by_id(&conn, "t1").unwrap();
        track.title = Some("新タイトル".to_string());

        // 既存パスなら更新される
        update_track_by_file_path(&conn, &track, None).unwrap();
        assert_eq!(
            find_track_by_id(&conn, "t1").unwrap().title,
            Some("新タイトル".to_string())
        );

        // 存在しないパスならNotFound
        track.file_path = "/test/unknown.mp3".to_string();
        let result = update_track_by_file_path(&conn, &track, None);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("更新対象のトラックが見つかりません")
        );
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

    /// ファイルから読み直した内容での更新は、評価をファイルの値にし、
    /// タグで持てないお気に入り・再生回数は変えない
    #[test]
    fn test_update_track_by_file_path_takes_rating_from_file() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        conn.execute(
            "UPDATE tracks SET rating = 2, is_favorite = 1, play_count = 7 WHERE id = 't1'",
            [],
        )
        .unwrap();

        // ファイルから作ったトラック（評価はタグの値、お気に入り・再生回数は初期値）
        let mut from_file = find_track_by_id(&conn, "t1").unwrap();
        from_file.rating = 5;
        from_file.is_favorite = false;
        from_file.play_count = 0;
        from_file.track_number = Some(3);
        update_track_by_file_path(&conn, &from_file, None).unwrap();

        let track = find_track_by_id(&conn, "t1").unwrap();
        assert_eq!(track.rating, 5);
        assert_eq!(track.track_number, Some(3));
        assert!(track.is_favorite);
        assert_eq!(track.play_count, 7);

        // 新規の登録でも、ファイルの評価を記録する
        from_file.id = "t2".to_string();
        from_file.file_path = "/test/t2.mp3".to_string();
        from_file.rating = 3;
        insert_track(&conn, &from_file, None).unwrap();
        assert_eq!(find_track_by_id(&conn, "t2").unwrap().rating, 3);
    }

    #[test]
    fn test_set_track_rating() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");

        set_track_rating(&conn, "t1", 4).unwrap();
        assert_eq!(find_track_by_id(&conn, "t1").unwrap().rating, 4);

        // 存在しないID → NotFound
        assert!(matches!(
            set_track_rating(&conn, "nonexistent", 3),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn test_find_all_track_tag_values() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        set_track_rating(&conn, "t1", 3).unwrap();

        let values = find_all_track_tag_values(&conn).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].id, "t1");
        assert_eq!(values[0].title.as_deref(), Some("曲A"));
        assert_eq!(values[0].artist.as_deref(), Some("アーティストX"));
        assert_eq!(values[0].rating, 3);
    }

    /// 任意のパスでトラックを挿入する（フォルダ単位の操作のテスト用）
    fn insert_track_at(conn: &Connection, id: &str, path: &str, modified_at: Option<i64>) {
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, format, file_size, file_modified_at, created_at, updated_at)
             VALUES (?1, ?2, 'f.mp3', 'mp3', 1000, ?3, datetime('now'), datetime('now'))",
            rusqlite::params![id, path, modified_at],
        )
        .expect("テストトラック挿入に失敗");
    }

    #[test]
    fn test_tracks_under_prefix_are_case_sensitive_and_exclude_siblings() {
        let conn = setup_test_db();
        insert_track_at(&conn, "a", "/music/a.mp3", Some(10));
        insert_track_at(&conn, "b", "/music/sub/b.mp3", None);
        insert_track_at(&conn, "c", "/music2/c.mp3", None);
        insert_track_at(&conn, "d", "/Music/d.mp3", None);
        insert_track_at(&conn, "e", "/music/日本語/e.mp3", None);
        // 範囲の境界（区切り文字の前後の文字）
        insert_track_at(&conn, "f", "/music0/f.mp3", None);
        insert_track_at(&conn, "g", "/music.mp3", None);

        let mut ids: Vec<String> = find_track_file_states_under(&conn, "/music/")
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        ids.sort();
        assert_eq!(ids, ["a", "b", "e"]);
        assert_eq!(count_tracks_under(&conn, "/music/").unwrap(), 3);
        assert_eq!(count_all_tracks(&conn).unwrap(), 7);

        let state = find_track_file_states_under(&conn, "/music/")
            .unwrap()
            .into_iter()
            .find(|t| t.id == "a")
            .unwrap();
        assert_eq!(state.file_size, 1000);
        assert_eq!(state.file_modified_at, Some(10));
    }

    #[test]
    fn test_prefix_upper_bound() {
        assert_eq!(prefix_upper_bound("/music/"), "/music0");
        assert_eq!(prefix_upper_bound("C:\\Music\\"), "C:\\Music]");
        // ASCII以外の文字も、次のコードポイントの文字にする（U+697D → U+697E）
        assert_eq!(prefix_upper_bound("/音楽"), "/音\u{697E}");
    }

    #[test]
    fn test_tracks_under_prefix_use_file_path_index() {
        let conn = setup_test_db();
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM tracks WHERE file_path >= ?1 AND file_path < ?2",
                ["/music/", "/music0"],
                |row| row.get(3),
            )
            .unwrap();
        assert!(plan.contains("INDEX"), "インデックスを使わない: {}", plan);
    }

    #[test]
    fn test_delete_tracks_under_cascades_to_playlists() {
        let conn = setup_test_db();
        insert_track_at(&conn, "a", "/music/a.mp3", None);
        insert_track_at(&conn, "c", "/other/c.mp3", None);
        conn.execute(
            "INSERT INTO playlists (id, name) VALUES ('p', 'プレイリスト')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES ('p', 'a', 0), ('p', 'c', 1)",
            [],
        )
        .unwrap();

        assert_eq!(delete_tracks_under(&conn, "/music/").unwrap(), 1);
        assert_eq!(count_all_tracks(&conn).unwrap(), 1);
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM playlist_tracks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn test_insert_and_update_record_file_modified_at() {
        let conn = setup_test_db();
        insert_test_track(&conn, "t1", "曲A", "アーティストX", "アルバム1", "ロック");
        let mut track = find_track_by_id(&conn, "t1").unwrap();

        update_track_by_file_path(&conn, &track, Some(123)).unwrap();
        let modified_at = |conn: &Connection| -> Option<i64> {
            conn.query_row(
                "SELECT file_modified_at FROM tracks WHERE id = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(modified_at(&conn), Some(123));

        set_track_file_modified_at(&conn, "t1", 456).unwrap();
        assert_eq!(modified_at(&conn), Some(456));

        set_track_file_state(&conn, "t1", 2048, Some(999)).unwrap();
        assert_eq!(modified_at(&conn), Some(999));
        assert_eq!(find_track_by_id(&conn, "t1").unwrap().file_size, 2048);

        track.id = "t2".to_string();
        track.file_path = "/test/t2.mp3".to_string();
        track.replay_gain = ReplayGain {
            track_gain: Some(1.5),
            track_peak: Some(0.5),
            album_gain: None,
            album_peak: Some(0.75),
        };
        insert_track(&conn, &track, Some(789)).unwrap();
        assert_eq!(
            find_track_by_id(&conn, "t2").unwrap().replay_gain,
            track.replay_gain
        );
        let inserted: Option<i64> = conn
            .query_row(
                "SELECT file_modified_at FROM tracks WHERE id = 't2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(inserted, Some(789));
    }
}
