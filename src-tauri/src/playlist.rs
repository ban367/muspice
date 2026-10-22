//! プレイリストのデータの読み書き
//!
//! どの関数も`AppResult`を返す。プレイリスト・曲・フォルダが見つからない場合は`NotFound`、
//! SQLの失敗は`Database`（操作の名前を付けたメッセージ）。コマンド層は、そのまま返す。

use crate::error::{AppError, AppResult};
use crate::models::{Playlist, PlaylistFolder, PlaylistTrack, Track};
use crate::repository::{TRACK_COLUMNS, query_tracks};
use crate::smart_playlist::{self, EvalContext, SmartRules};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// プレイリストが見つからない時のメッセージ
const PLAYLIST_NOT_FOUND: &str = "プレイリストが見つかりません";
/// 曲が見つからない時のメッセージ
const TRACK_NOT_FOUND: &str = "トラックが見つかりません";
/// プレイリストのフォルダが見つからない時のメッセージ
const FOLDER_NOT_FOUND: &str = "フォルダが見つかりません";

/// SQLの失敗を、操作の名前を付けた`Database`にする（`.map_err(database("プレイリストの作成"))`）
fn database(operation: &'static str) -> impl Fn(rusqlite::Error) -> AppError + Copy {
    move |e| AppError::Database(format!("{operation}に失敗しました: {e}"))
}

/// プレイリストを作成
pub fn create_playlist(conn: &Connection, name: &str) -> AppResult<Playlist> {
    insert_playlist(conn, name, None).map_err(database("プレイリストの作成"))
}

/// 自動プレイリスト（条件で曲を集めるプレイリスト）を作成
pub fn create_smart_playlist(
    conn: &Connection,
    name: &str,
    rules: &SmartRules,
) -> AppResult<Playlist> {
    insert_playlist(conn, name, Some(rules)).map_err(database("プレイリストの作成"))
}

fn insert_playlist(
    conn: &Connection,
    name: &str,
    rules: Option<&SmartRules>,
) -> rusqlite::Result<Playlist> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    // 手動の並び順では、いちばん後ろに置く
    let position = next_position(conn, "playlists")?;

    conn.execute(
        "INSERT INTO playlists (id, name, rules, position, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        rusqlite::params![
            id,
            name,
            rules.map(rules_to_json).transpose()?,
            position,
            now
        ],
    )?;

    Ok(Playlist {
        id,
        name: name.to_string(),
        description: None,
        tracks: Vec::new(),
        rules: rules.cloned(),
        folder_id: None,
        position,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// 手動の並び順で、いちばん後ろになる位置（表の名前は、固定の文字列だけを渡す）
fn next_position(conn: &Connection, table: &str) -> rusqlite::Result<i32> {
    conn.query_row(
        &format!("SELECT COALESCE(MAX(position), -1) + 1 FROM {table}"),
        [],
        |row| row.get(0),
    )
}

/// 行があることを確かめる（なければ`not_found`のメッセージの`NotFound`。表の名前は、固定の文字列だけを渡す）
fn ensure_exists(conn: &Connection, table: &str, id: &str, not_found: &str) -> AppResult<()> {
    let exists = conn
        .prepare_cached(&format!("SELECT 1 FROM {table} WHERE id = ?1"))
        .and_then(|mut stmt| stmt.exists([id]))
        .map_err(database("存在の確認"))?;
    if exists {
        Ok(())
    } else {
        Err(AppError::NotFound(not_found.to_string()))
    }
}

/// 曲がすべてあることを確かめる（1曲でもなければ`NotFound`）
fn ensure_tracks_exist(conn: &Connection, track_ids: &[String]) -> AppResult<()> {
    track_ids
        .iter()
        .try_for_each(|track_id| ensure_exists(conn, "tracks", track_id, TRACK_NOT_FOUND))
}

fn rules_to_json(rules: &SmartRules) -> rusqlite::Result<String> {
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
fn find_rules(conn: &Connection, playlist_id: &str) -> AppResult<Option<Option<SmartRules>>> {
    conn.query_row(
        "SELECT rules FROM playlists WHERE id = ?1",
        [playlist_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|found| found.map(parse_rules))
    .map_err(database("プレイリストの取得"))
}

/// 曲を自分で選ぶ通常のプレイリストであることを確かめる（曲の追加・削除・並べ替えの前に呼ぶ）
///
/// 自動プレイリストの曲は条件で決まるため、変更できない。プレイリストがない場合は、
/// このあとの操作が「見つからない」を返すため、ここでは通す。
pub fn ensure_manual_playlist(conn: &Connection, playlist_id: &str) -> AppResult<()> {
    if matches!(find_rules(conn, playlist_id)?, Some(Some(_))) {
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
    match find_rules(conn, playlist_id)? {
        None => return Err(AppError::NotFound(PLAYLIST_NOT_FOUND.to_string())),
        Some(None) => {
            return Err(AppError::Validation(
                "通常のプレイリストには、条件を設定できません".to_string(),
            ));
        }
        Some(Some(_)) => {}
    }

    let database = database("プレイリストの条件の変更");
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

/// プレイリストの名前を取得する
pub fn get_playlist_name(conn: &Connection, playlist_id: &str) -> AppResult<String> {
    conn.query_row(
        "SELECT name FROM playlists WHERE id = ?1",
        [playlist_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(database("プレイリストの取得"))?
    .ok_or_else(|| AppError::NotFound(PLAYLIST_NOT_FOUND.to_string()))
}

/// 曲を入れた状態で、プレイリストを作る（M3Uの読み込み用）
///
/// 同じ名前のプレイリストがある場合は、番号を付けた名前にする（`m3u::unique_playlist_name`）。
/// 1つのトランザクションで行い、見つからない曲がある場合（`NotFound`）・途中で失敗した場合は
/// 何も作らない。`track_ids`には、同じトラックを2回含めない。
pub fn create_playlist_with_tracks(
    conn: &mut Connection,
    name: &str,
    track_ids: &[String],
) -> AppResult<Playlist> {
    let database = database("プレイリストの作成");
    let tx = conn.transaction().map_err(database)?;
    let playlist = insert_playlist_with_tracks(&tx, name, track_ids)?;
    tx.commit().map_err(database)?;
    Ok(playlist)
}

/// `create_playlist_with_tracks`の本体（呼び出し側のトランザクションの中で行う）
///
/// ライブラリのXMLの取り込みは、曲の値と、すべてのプレイリストを1つのトランザクションで書く
/// （途中で失敗した場合に、作ったプレイリストだけが残らないようにする）。
pub fn insert_playlist_with_tracks(
    conn: &Connection,
    name: &str,
    track_ids: &[String],
) -> AppResult<Playlist> {
    let database = database("プレイリストの作成");
    ensure_tracks_exist(conn, track_ids)?;

    let existing: Vec<String> = conn
        .prepare("SELECT name FROM playlists")
        .and_then(|mut stmt| stmt.query_map([], |row| row.get(0))?.collect())
        .map_err(database)?;
    let name = crate::m3u::unique_playlist_name(name, &existing);

    let mut playlist = create_playlist(conn, &name)?;
    insert_playlist_tracks(conn, &mut playlist, track_ids).map_err(database)?;
    Ok(playlist)
}

/// 渡した曲を入れた状態で、プレイリストを作る（再生キューの保存用）
///
/// 名前はそのまま使う。同じ曲は、最初の1回だけを入れる（プレイリストには、同じ曲を2回入れない）。
/// 1つのトランザクションで行い、見つからない曲がある場合は何も作らない（`NotFound`）。
pub fn create_playlist_from_tracks(
    conn: &mut Connection,
    name: &str,
    track_ids: &[String],
) -> AppResult<Playlist> {
    let mut seen = HashSet::new();
    let unique: Vec<String> = track_ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect();

    let database = database("プレイリストの作成");
    ensure_tracks_exist(conn, &unique)?;
    let tx = conn.transaction().map_err(database)?;
    let mut playlist = create_playlist(&tx, name)?;
    insert_playlist_tracks(&tx, &mut playlist, &unique).map_err(database)?;

    tx.commit().map_err(database)?;
    Ok(playlist)
}

/// 作ったばかりのプレイリストに、曲を渡した順に入れる（`track_ids`には、同じトラックを2回含めない）
fn insert_playlist_tracks(
    conn: &Connection,
    playlist: &mut Playlist,
    track_ids: &[String],
) -> rusqlite::Result<()> {
    let mut insert = conn.prepare(
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
    Ok(())
}

/// すべてのプレイリストを取得
///
/// プレイリストごとにトラックを問い合わせるとN+1クエリになるため、
/// 全プレイリストのトラックを1クエリで取得してから割り当てる。
pub fn get_all_playlists(conn: &Connection) -> AppResult<Vec<Playlist>> {
    query_all_playlists(conn).map_err(database("プレイリストの取得"))
}

fn query_all_playlists(conn: &Connection) -> rusqlite::Result<Vec<Playlist>> {
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
        "SELECT id, name, description, created_at, updated_at, rules, folder_id, position
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
                folder_id: row.get(6)?,
                position: row.get(7)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

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
    if let Some(Some(rules)) = find_rules(conn, playlist_id)? {
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

/// プレイリストに複数のトラックを、渡した順に追加する
///
/// すでに入っているトラック（同じIDを2回渡した場合の2回目を含む）は飛ばす。
/// 1つのトランザクションで行い、見つからないプレイリスト・曲がある場合（`NotFound`）と、途中で
/// 失敗した場合は1曲も追加しない。追加したトラック数を返す。
///
/// プレイリストの確認・最後の位置の取得は1回だけ行い、曲ごとの文は使い回す（多くの曲を一度に
/// 追加しても、曲ごとに準備し直さない）。
pub fn add_tracks_to_playlist(
    conn: &Connection,
    playlist_id: &str,
    track_ids: &[String],
) -> AppResult<usize> {
    let database = database("トラックの追加");
    let tx = conn.unchecked_transaction().map_err(database)?;
    ensure_exists(&tx, "playlists", playlist_id, PLAYLIST_NOT_FOUND)?;

    let mut next_position: i32 = tx
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
            [playlist_id],
            |row| row.get(0),
        )
        .map_err(database)?;
    let now = Utc::now().to_rfc3339();

    let mut added = 0;
    {
        // すでに入っている曲は、主キー（プレイリストと曲）が同じため入らない
        let mut insert = tx
            .prepare(
                "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position, added_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(database)?;
        for track_id in track_ids {
            ensure_exists(&tx, "tracks", track_id, TRACK_NOT_FOUND)?;
            let inserted = insert
                .execute(rusqlite::params![playlist_id, track_id, next_position, now])
                .map_err(database)?;
            if inserted > 0 {
                next_position += 1;
                added += 1;
            }
        }
    }

    if added > 0 {
        tx.execute(
            "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, playlist_id],
        )
        .map_err(database)?;
    }
    tx.commit().map_err(database)?;
    Ok(added)
}

/// プレイリストから複数のトラックを外す
///
/// 入っていないトラックは飛ばす。1つのトランザクションで行い、外したトラック数を返す。
/// プレイリストがなければ`NotFound`。
pub fn remove_tracks_from_playlist(
    conn: &Connection,
    playlist_id: &str,
    track_ids: &[String],
) -> AppResult<usize> {
    let database = database("トラックの削除");
    let tx = conn.unchecked_transaction().map_err(database)?;
    ensure_exists(&tx, "playlists", playlist_id, PLAYLIST_NOT_FOUND)?;

    let mut removed = 0;
    {
        let mut delete = tx
            .prepare("DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2")
            .map_err(database)?;
        for track_id in track_ids {
            removed += delete
                .execute(rusqlite::params![playlist_id, track_id])
                .map_err(database)?;
        }
    }

    if removed > 0 {
        // position値を再計算して連番にする
        renumber_positions(&tx, playlist_id).map_err(database)?;
        tx.execute(
            "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![Utc::now().to_rfc3339(), playlist_id],
        )
        .map_err(database)?;
    }

    tx.commit().map_err(database)?;
    Ok(removed)
}

/// プレイリストの曲の位置を、今の順のまま0からの連番に振り直す（曲を外した後。1つの文で行う）
fn renumber_positions(conn: &Connection, playlist_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE playlist_tracks SET position = numbered.new_position
         FROM (
             SELECT track_id, ROW_NUMBER() OVER (ORDER BY position) - 1 AS new_position
             FROM playlist_tracks WHERE playlist_id = ?1
         ) AS numbered
         WHERE playlist_tracks.playlist_id = ?1 AND playlist_tracks.track_id = numbered.track_id",
        [playlist_id],
    )?;
    Ok(())
}

/// プレイリストの名前を変更
pub fn rename_playlist(conn: &Connection, playlist_id: &str, name: &str) -> AppResult<()> {
    ensure_exists(conn, "playlists", playlist_id, PLAYLIST_NOT_FOUND)?;
    conn.execute(
        "UPDATE playlists SET name = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![name, Utc::now().to_rfc3339(), playlist_id],
    )
    .map_err(database("プレイリスト名の変更"))?;
    Ok(())
}

/// プレイリストの説明を変更（値なし・空の文字列は、説明なし）
pub fn set_playlist_description(
    conn: &Connection,
    playlist_id: &str,
    description: Option<&str>,
) -> AppResult<()> {
    ensure_exists(conn, "playlists", playlist_id, PLAYLIST_NOT_FOUND)?;

    let description = description.map(str::trim).filter(|text| !text.is_empty());
    conn.execute(
        "UPDATE playlists SET description = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![description, Utc::now().to_rfc3339(), playlist_id],
    )
    .map_err(database("説明の変更"))?;
    Ok(())
}

/// プレイリストのフォルダを取得（手動の並び順。同じ位置なら、作った順）
pub fn get_playlist_folders(conn: &Connection) -> AppResult<Vec<PlaylistFolder>> {
    conn.prepare(
        "SELECT id, name, position, created_at FROM playlist_folders
         ORDER BY position, created_at, id",
    )
    .and_then(|mut stmt| {
        stmt.query_map([], |row| {
            Ok(PlaylistFolder {
                id: row.get(0)?,
                name: row.get(1)?,
                position: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect()
    })
    .map_err(database("フォルダの取得"))
}

/// プレイリストのフォルダを作成（手動の並び順では、いちばん後ろに置く）
pub fn create_playlist_folder(conn: &Connection, name: &str) -> AppResult<PlaylistFolder> {
    let database = database("フォルダの作成");
    let folder = PlaylistFolder {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        position: next_position(conn, "playlist_folders").map_err(database)?,
        created_at: Utc::now().to_rfc3339(),
    };
    conn.execute(
        "INSERT INTO playlist_folders (id, name, position, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![folder.id, folder.name, folder.position, folder.created_at],
    )
    .map_err(database)?;
    Ok(folder)
}

/// プレイリストのフォルダの名前を変更
pub fn rename_playlist_folder(conn: &Connection, folder_id: &str, name: &str) -> AppResult<()> {
    ensure_exists(conn, "playlist_folders", folder_id, FOLDER_NOT_FOUND)?;
    conn.execute(
        "UPDATE playlist_folders SET name = ?1 WHERE id = ?2",
        [name, folder_id],
    )
    .map_err(database("フォルダ名の変更"))?;
    Ok(())
}

/// プレイリストのフォルダを削除（中のプレイリストは消さず、フォルダの外へ出す）
pub fn delete_playlist_folder(conn: &Connection, folder_id: &str) -> AppResult<()> {
    let database = database("フォルダの削除");
    let tx = conn.unchecked_transaction().map_err(database)?;
    ensure_exists(&tx, "playlist_folders", folder_id, FOLDER_NOT_FOUND)?;

    tx.execute(
        "UPDATE playlists SET folder_id = NULL WHERE folder_id = ?1",
        [folder_id],
    )
    .map_err(database)?;
    tx.execute("DELETE FROM playlist_folders WHERE id = ?1", [folder_id])
        .map_err(database)?;

    tx.commit().map_err(database)
}

/// プレイリストを、フォルダへ移す（`folder_id`が値なしなら、フォルダの外へ出す）
///
/// 手動の並び順では、移した先のいちばん後ろに置く。
pub fn move_playlist(
    conn: &Connection,
    playlist_id: &str,
    folder_id: Option<&str>,
) -> AppResult<()> {
    let database = database("プレイリストの移動");
    let tx = conn.unchecked_transaction().map_err(database)?;
    ensure_exists(&tx, "playlists", playlist_id, PLAYLIST_NOT_FOUND)?;
    if let Some(folder_id) = folder_id {
        ensure_exists(&tx, "playlist_folders", folder_id, FOLDER_NOT_FOUND)?;
    }

    let position = next_position(&tx, "playlists").map_err(database)?;
    tx.execute(
        "UPDATE playlists SET folder_id = ?1, position = ?2 WHERE id = ?3",
        rusqlite::params![folder_id, position, playlist_id],
    )
    .map_err(database)?;

    tx.commit().map_err(database)
}

/// プレイリストの手動の並び順を変更（渡した順に、位置を振り直す）
///
/// 並び順は同じフォルダの中で比べるため、1つのフォルダ（またはフォルダの外）のプレイリストを渡す。
/// 見つからないプレイリストは無視する。
pub fn reorder_playlists(conn: &Connection, playlist_ids: &[String]) -> AppResult<()> {
    set_positions(conn, "playlists", playlist_ids).map_err(database("プレイリストの並び替え"))
}

/// プレイリストのフォルダの手動の並び順を変更（渡した順に、位置を振り直す）
pub fn reorder_playlist_folders(conn: &Connection, folder_ids: &[String]) -> AppResult<()> {
    set_positions(conn, "playlist_folders", folder_ids).map_err(database("フォルダの並び替え"))
}

/// 渡した順に、位置を振り直す（表の名前は、固定の文字列だけを渡す）
fn set_positions(conn: &Connection, table: &str, ids: &[String]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut update = tx.prepare(&format!("UPDATE {table} SET position = ?1 WHERE id = ?2"))?;
        for (position, id) in ids.iter().enumerate() {
            update.execute(rusqlite::params![position as i32, id])?;
        }
    }
    tx.commit()
}

/// プレイリストを削除
pub fn delete_playlist(conn: &Connection, playlist_id: &str) -> AppResult<()> {
    let database = database("プレイリストの削除");
    let tx = conn.unchecked_transaction().map_err(database)?;
    ensure_exists(&tx, "playlists", playlist_id, PLAYLIST_NOT_FOUND)?;

    // まずプレイリスト内のトラックを削除してから、プレイリスト自体を削除する
    tx.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
        [playlist_id],
    )
    .map_err(database)?;
    tx.execute("DELETE FROM playlists WHERE id = ?1", [playlist_id])
        .map_err(database)?;

    tx.commit().map_err(database)
}

/// プレイリスト内のトラックを並び替え（渡した順に、位置を振り直す）
pub fn reorder_playlist_tracks(
    conn: &Connection,
    playlist_id: &str,
    track_ids: &[String],
) -> AppResult<()> {
    let database = database("トラックの並び替え");
    let tx = conn.unchecked_transaction().map_err(database)?;
    ensure_exists(&tx, "playlists", playlist_id, PLAYLIST_NOT_FOUND)?;

    {
        let mut update = tx
            .prepare(
                "UPDATE playlist_tracks SET position = ?1
                 WHERE playlist_id = ?2 AND track_id = ?3",
            )
            .map_err(database)?;
        for (index, track_id) in track_ids.iter().enumerate() {
            update
                .execute(rusqlite::params![index as i32, playlist_id, track_id])
                .map_err(database)?;
        }
    }

    tx.execute(
        "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![Utc::now().to_rfc3339(), playlist_id],
    )
    .map_err(database)?;

    tx.commit().map_err(database)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        crate::db::open_in_memory()
    }

    fn insert_test_track(conn: &Connection, id: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tracks (id, file_path, file_name, title, created_at, updated_at, file_size, format)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'mp3')",
            rusqlite::params![id, format!("/test/{id}.mp3"), "test.mp3", "Test Track", now, now],
        )
        .unwrap();
    }

    /// プレイリストに1曲を追加する（追加したらtrue）
    fn add_track_to_playlist(
        conn: &Connection,
        playlist_id: &str,
        track_id: &str,
    ) -> AppResult<bool> {
        add_tracks_to_playlist(conn, playlist_id, &[track_id.to_string()]).map(|added| added > 0)
    }

    /// 名前の変更・取得。ないプレイリストは`NotFound`
    #[test]
    fn test_rename_and_get_playlist_name() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "通勤").unwrap();

        rename_playlist(&conn, &playlist.id, "帰宅").unwrap();
        assert_eq!(get_playlist_name(&conn, &playlist.id).unwrap(), "帰宅");

        assert!(matches!(
            rename_playlist(&conn, "missing", "名前"),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            get_playlist_name(&conn, "missing"),
            Err(AppError::NotFound(_))
        ));
    }

    /// 読み込み用のプレイリストの作成: 同じ名前には番号を付け、見つからない曲があれば作らない
    #[test]
    fn test_create_playlist_with_tracks() {
        let mut conn = setup_test_db();
        for id in ["track1", "track2"] {
            insert_test_track(&conn, id);
        }
        create_playlist(&conn, "読み込み").unwrap();

        let ids = ["track2", "track1"].map(String::from);
        let playlist = create_playlist_with_tracks(&mut conn, "読み込み", &ids).unwrap();
        assert_ne!(playlist.name, "読み込み");
        let order: Vec<&str> = playlist
            .tracks
            .iter()
            .map(|t| t.track_id.as_str())
            .collect();
        assert_eq!(order, ["track2", "track1"]);

        // 見つからない曲は、外部キーの違反（DATABASE）ではなく`NotFound`
        let with_missing = ["track1", "missing"].map(String::from);
        assert!(matches!(
            create_playlist_with_tracks(&mut conn, "だめ", &with_missing),
            Err(AppError::NotFound(_))
        ));
        assert_eq!(get_all_playlists(&conn).unwrap().len(), 2);
    }

    /// 曲を外した後の位置は、今の順のまま0からの連番になる（間の空いた位置からも）
    #[test]
    fn test_positions_are_renumbered_in_order() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        for (id, position) in [("track1", 10), ("track2", 4), ("track3", 7), ("track4", 20)] {
            insert_test_track(&conn, id);
            conn.execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
                rusqlite::params![playlist.id, id, position],
            )
            .unwrap();
        }

        remove_tracks_from_playlist(&conn, &playlist.id, &["track3".to_string()]).unwrap();

        let playlists = get_all_playlists(&conn).unwrap();
        let order: Vec<(&str, i32)> = playlists[0]
            .tracks
            .iter()
            .map(|t| (t.track_id.as_str(), t.position))
            .collect();
        assert_eq!(order, [("track2", 0), ("track1", 1), ("track4", 2)]);
    }

    /// 多くの曲を追加しても、曲の確認・追加の文は準備し直さない（使い回す）
    #[test]
    fn test_adding_many_tracks_appends_after_the_last_position() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        let ids: Vec<String> = (0..200).map(|index| format!("track{index}")).collect();
        for id in &ids {
            insert_test_track(&conn, id);
        }
        assert_eq!(
            add_tracks_to_playlist(&conn, &playlist.id, &ids[..100]).unwrap(),
            100
        );
        assert_eq!(
            add_tracks_to_playlist(&conn, &playlist.id, &ids).unwrap(),
            100
        );

        let playlists = get_all_playlists(&conn).unwrap();
        let positions: Vec<i32> = playlists[0].tracks.iter().map(|t| t.position).collect();
        assert_eq!(positions, (0..200).collect::<Vec<_>>());
        assert!(matches!(
            add_tracks_to_playlist(&conn, "missing", &ids[..1]),
            Err(AppError::NotFound(_))
        ));
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

    /// 今の位置を読めない場合は、先頭の位置に追加せず、エラーを返す
    #[test]
    fn test_add_track_to_playlist_fails_when_positions_cannot_be_read() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        insert_test_track(&conn, "track1");
        insert_test_track(&conn, "track2");
        // i32に収まらない位置
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, 'track1', 3000000000)",
            [&playlist.id],
        )
        .unwrap();

        assert!(add_track_to_playlist(&conn, &playlist.id, "track2").is_err());
        let added: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE track_id = 'track2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(added, 0);
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
        assert!(matches!(result, Err(AppError::NotFound(_))));

        let playlists = get_all_playlists(&conn).unwrap();
        assert!(playlists[0].tracks.is_empty());
    }

    /// 複数の曲をまとめて外し、残った曲の位置を詰める（入っていない曲は飛ばす）
    #[test]
    fn test_remove_tracks_from_playlist() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        for id in ["track1", "track2", "track3", "track4"] {
            insert_test_track(&conn, id);
            add_track_to_playlist(&conn, &playlist.id, id).unwrap();
        }
        insert_test_track(&conn, "other");

        let ids = ["track3", "track1", "other"].map(String::from);
        let removed = remove_tracks_from_playlist(&conn, &playlist.id, &ids).unwrap();
        assert_eq!(removed, 2);

        let playlists = get_all_playlists(&conn).unwrap();
        let rest: Vec<(&str, i32)> = playlists[0]
            .tracks
            .iter()
            .map(|t| (t.track_id.as_str(), t.position))
            .collect();
        assert_eq!(rest, [("track2", 0), ("track4", 1)]);

        // 1曲も外さなかった場合は0。プレイリストがなければ、見つからない
        assert_eq!(
            remove_tracks_from_playlist(&conn, &playlist.id, &ids[2..]).unwrap(),
            0
        );
        assert!(matches!(
            remove_tracks_from_playlist(&conn, "missing", &ids),
            Err(AppError::NotFound(_))
        ));
    }

    /// 渡した曲でプレイリストを作る（同じ曲は最初の1回だけ。見つからない曲があれば作らない）
    #[test]
    fn test_create_playlist_from_tracks() {
        let mut conn = setup_test_db();
        for id in ["track1", "track2", "track3"] {
            insert_test_track(&conn, id);
        }
        create_playlist(&conn, "キュー").unwrap();

        let ids = ["track2", "track1", "track2", "track3"].map(String::from);
        let playlist = create_playlist_from_tracks(&mut conn, "キュー", &ids).unwrap();

        // 同じ名前があっても、番号は付けない
        assert_eq!(playlist.name, "キュー");
        let saved = get_all_playlists(&conn).unwrap();
        let saved = saved.iter().find(|p| p.id == playlist.id).unwrap();
        let order: Vec<&str> = saved.tracks.iter().map(|t| t.track_id.as_str()).collect();
        assert_eq!(order, ["track2", "track1", "track3"]);

        let with_missing = ["track1", "missing"].map(String::from);
        assert!(matches!(
            create_playlist_from_tracks(&mut conn, "だめ", &with_missing),
            Err(AppError::NotFound(_))
        ));
        assert_eq!(get_all_playlists(&conn).unwrap().len(), 2);
    }

    #[test]
    fn test_set_playlist_description() {
        let conn = setup_test_db();
        let playlist = create_playlist(&conn, "Test Playlist").unwrap();
        let description =
            |conn: &Connection| get_all_playlists(conn).unwrap()[0].description.clone();

        set_playlist_description(&conn, &playlist.id, Some("  朝の通勤用  ")).unwrap();
        assert_eq!(description(&conn).as_deref(), Some("朝の通勤用"));

        // 空の文字列・値なしは、説明なし
        set_playlist_description(&conn, &playlist.id, Some("   ")).unwrap();
        assert_eq!(description(&conn), None);
        set_playlist_description(&conn, &playlist.id, Some("説明")).unwrap();
        set_playlist_description(&conn, &playlist.id, None).unwrap();
        assert_eq!(description(&conn), None);

        assert!(matches!(
            set_playlist_description(&conn, "missing", Some("説明")),
            Err(AppError::NotFound(_))
        ));
    }

    /// フォルダを作り、プレイリストを出し入れできる。フォルダを消しても、プレイリストは残る
    #[test]
    fn test_playlist_folders() {
        let conn = setup_test_db();
        let commute = create_playlist(&conn, "通勤").unwrap();
        let drive = create_playlist(&conn, "ドライブ").unwrap();
        let folder = create_playlist_folder(&conn, "外出").unwrap();
        let other = create_playlist_folder(&conn, "家").unwrap();
        assert_eq!((folder.position, other.position), (0, 1));
        let folder_of = |conn: &Connection, id: &str| {
            get_all_playlists(conn)
                .unwrap()
                .into_iter()
                .find(|p| p.id == id)
                .unwrap()
                .folder_id
        };

        // 作ったばかりのプレイリストは、フォルダの外
        assert_eq!(folder_of(&conn, &commute.id), None);
        move_playlist(&conn, &commute.id, Some(&folder.id)).unwrap();
        move_playlist(&conn, &drive.id, Some(&folder.id)).unwrap();
        assert_eq!(folder_of(&conn, &commute.id), Some(folder.id.clone()));

        rename_playlist_folder(&conn, &folder.id, "おでかけ").unwrap();
        move_playlist(&conn, &drive.id, None).unwrap();
        assert_eq!(folder_of(&conn, &drive.id), None);
        let folders = get_playlist_folders(&conn).unwrap();
        assert_eq!(
            folders.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["おでかけ", "家"]
        );

        // ないプレイリスト・ないフォルダへは移せない
        for (playlist_id, folder_id) in [
            ("missing", Some(folder.id.as_str())),
            (&commute.id, Some("missing")),
        ] {
            assert!(matches!(
                move_playlist(&conn, playlist_id, folder_id),
                Err(AppError::NotFound(_))
            ));
        }

        delete_playlist_folder(&conn, &folder.id).unwrap();
        assert_eq!(get_playlist_folders(&conn).unwrap().len(), 1);
        assert_eq!(get_all_playlists(&conn).unwrap().len(), 2);
        assert_eq!(folder_of(&conn, &commute.id), None);
        assert!(matches!(
            delete_playlist_folder(&conn, &folder.id),
            Err(AppError::NotFound(_))
        ));
    }

    /// 手動の並び順: 新しいものはいちばん後ろ。渡した順に並べ替えられる
    #[test]
    fn test_manual_order_of_playlists_and_folders() {
        let conn = setup_test_db();
        let ids: Vec<String> = ["A", "B", "C"]
            .iter()
            .map(|name| create_playlist(&conn, name).unwrap().id)
            .collect();
        let names_by_position = |conn: &Connection| {
            let mut playlists = get_all_playlists(conn).unwrap();
            playlists.sort_by_key(|p| p.position);
            playlists.into_iter().map(|p| p.name).collect::<Vec<_>>()
        };
        assert_eq!(names_by_position(&conn), ["A", "B", "C"]);

        reorder_playlists(
            &conn,
            &[
                ids[2].clone(),
                ids[0].clone(),
                "missing".to_string(),
                ids[1].clone(),
            ],
        )
        .unwrap();
        assert_eq!(names_by_position(&conn), ["C", "A", "B"]);

        // フォルダへ移すと、いちばん後ろになる。新しいプレイリストも、いちばん後ろ
        let folder = create_playlist_folder(&conn, "F").unwrap();
        move_playlist(&conn, &ids[2], Some(&folder.id)).unwrap();
        create_playlist(&conn, "D").unwrap();
        assert_eq!(names_by_position(&conn), ["A", "B", "C", "D"]);

        let second = create_playlist_folder(&conn, "G").unwrap();
        reorder_playlist_folders(&conn, &[second.id.clone(), folder.id.clone()]).unwrap();
        let folders = get_playlist_folders(&conn).unwrap();
        assert_eq!(
            folders.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["G", "F"]
        );
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
        let conn = crate::db::open_in_memory();
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
