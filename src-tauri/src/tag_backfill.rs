//! 既存のトラックの、後から追加した項目（アルバムアーティスト・ソート用のタグ・複数の値）の読み込み
//!
//! これらの列（`tracks.album_artist`・`tracks.title_sort`など）を追加する前に登録したトラックは、
//! 値が空になっている。そのままでは、コンピレーションが曲のアーティストごとの別のアルバムに
//! 分かれて見え、読みの順に並ばないため、起動時にバックグラウンドでファイルのタグから読み込む。
//!
//! 複数の値（アーティスト・ジャンル。ADR-051）に対応する前に登録したトラックは、同じ項目が複数ある
//! タグの、最初の値だけを読んでいる。ファイルに複数の値があり、今の値がその最初の値のままなら、
//! すべての値に置き換える（アプリで編集した値は、変えない）。
//!
//! - 読み込むのは未読の項目だけで、ほかの項目は変えない（「メタデータを更新」と違い、
//!   以前のバージョンでDBだけに保存した編集内容を失わない）
//! - 読み込んだトラックは`album_artist_read`・`sort_tags_read`・`multi_values_read`を1にする。
//!   対象がなくなれば、次回以降の起動では何もしない
//! - ファイルを読めなかったトラック（外付けドライブが外れているなど）は未読のまま残し、次回の起動で読み直す
//! - 途中でアプリを終了した場合は、次回の起動で続きから読み込む

use crate::error::{AppError, AppResult};
use crate::events::LibraryChanged;
use crate::metadata::{BackfillTags, read_backfill_tags};
use crate::state::AppState;
use std::path::Path;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// 起動してから読み込みを始めるまでの待ち時間（起動直後の表示を妨げない）
const STARTUP_DELAY: Duration = Duration::from_secs(5);

/// 1回のトランザクションで記録するトラック数（ファイルの読み取りはDBロックの外で行う）
const BATCH_SIZE: u32 = 200;

/// 読み込みの結果
#[derive(Debug, Default, PartialEq)]
struct BackfillResult {
    /// ファイルを読んで記録したトラック数
    read_count: usize,
    /// そのうち、未読だった項目のタグがあったトラック数（一覧のまとめ方・並びが変わる）
    found_count: usize,
    /// ファイルを読めなかったトラック数（未読のまま残る）
    failed_count: usize,
}

/// アプリの起動時に呼ぶ: 未読のトラックがあれば、別スレッドで読み込む
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let spawned = thread::Builder::new()
        .name("tag-backfill".to_string())
        .spawn(move || {
            thread::sleep(STARTUP_DELAY);
            match backfill(&app.state::<AppState>(), read_backfill_tags) {
                Ok(result) if result.read_count == 0 && result.failed_count == 0 => {}
                Ok(result) => {
                    log::info!(
                        "既存のトラックのアルバムアーティスト・ソート用のタグを読み込みました（読み込み: {}曲、タグあり: {}曲、読めなかったファイル: {}曲）",
                        result.read_count,
                        result.found_count,
                        result.failed_count
                    );
                    // アルバム・アーティストの一覧のまとめ方と並びが変わるため、一覧を読み直させる
                    if result.found_count > 0 {
                        let _ = LibraryChanged.emit(&app);
                    }
                }
                Err(e) => log::error!("既存のトラックのタグの読み込みに失敗しました: {}", e),
            }
        });
    if let Err(e) = spawned {
        log::error!(
            "既存のトラックのタグの読み込みのスレッドを開始できません: {}",
            e
        );
    }
}

/// 未読のトラックのアルバムアーティスト・ソート用のタグを、`read`で読んで記録する
fn backfill(
    state: &AppState,
    read: impl Fn(&Path) -> AppResult<BackfillTags>,
) -> AppResult<BackfillResult> {
    let mut result = BackfillResult::default();
    let mut after_rowid = 0;

    loop {
        let tracks = state.with_db(|db| {
            crate::repository::find_tracks_with_unread_tags(db, after_rowid, BATCH_SIZE)
        })?;
        let Some(last) = tracks.last() else {
            break;
        };
        after_rowid = last.rowid;

        // ファイルの読み取りは、DBロックを持たずに行う
        let mut read_tracks = Vec::with_capacity(tracks.len());
        for track in &tracks {
            match read(Path::new(&track.file_path)) {
                Ok(tags) => read_tracks.push((track, tags)),
                Err(_) => result.failed_count += 1,
            }
        }

        state.with_db(|db| {
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;
            for (track, tags) in &read_tracks {
                // すでに読んだ項目は変えない（アプリで編集した値を、読み直しで戻さない）
                if track.needs_album_artist {
                    crate::repository::set_track_album_artist(
                        &tx,
                        &track.id,
                        tags.album_artist.as_deref(),
                    )?;
                }
                if track.needs_sort_tags {
                    crate::repository::set_track_sort_tags(&tx, &track.id, &tags.sort_tags)?;
                }
                if track.needs_multi_values {
                    crate::repository::set_track_multi_values(
                        &tx,
                        &track.id,
                        all_values(track.artist.as_deref(), tags.artist.as_deref()),
                        all_values(track.genre.as_deref(), tags.genre.as_deref()),
                    )?;
                }
            }
            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })
        })?;

        result.read_count += read_tracks.len();
        result.found_count += read_tracks
            .iter()
            .filter(|(track, tags)| {
                (track.needs_album_artist && tags.album_artist.is_some())
                    || (track.needs_sort_tags && !tags.sort_tags.is_empty())
                    || (track.needs_multi_values
                        && (all_values(track.artist.as_deref(), tags.artist.as_deref()).is_some()
                            || all_values(track.genre.as_deref(), tags.genre.as_deref()).is_some()))
            })
            .count();
    }

    Ok(result)
}

/// 最初の値だけを読んでいた項目なら、ファイルにあるすべての値（`; `でつないだ文字列）を返す
///
/// `current`は今の値、`from_file`はファイルから複数の値として読んだ値。ファイルに複数の値があり、
/// 今の値がその最初の値と同じ場合だけ、置き換える値を返す（アプリで編集した値・もともと1つの値は、
/// 変えない）。
fn all_values<'a>(current: Option<&str>, from_file: Option<&'a str>) -> Option<&'a str> {
    let from_file = from_file?;
    let file_values = crate::multi_value::split(from_file);
    let current_values = crate::multi_value::split(current?);
    (file_values.len() > 1 && current_values.as_slice() == &file_values[..1]).then_some(from_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SortTags;

    fn setup_state() -> AppState {
        let conn = crate::db::open_in_memory();
        AppState::new(conn)
    }

    /// 列を追加する前に登録したトラック（アルバムアーティストもソート用のタグも未読）を入れる
    fn insert_unread_track(state: &AppState, id: &str, title: &str) {
        state
            .with_db(|db| {
                db.execute(
                    "INSERT INTO tracks (id, file_path, file_name, title, artist, format, file_size, updated_at)
                     VALUES (?1, ?2, ?3, ?4, '曲のアーティスト', 'mp3', 1, '2026-01-01T00:00:00Z')",
                    rusqlite::params![id, format!("/music/{id}.mp3"), format!("{id}.mp3"), title],
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
    }

    fn album_artist_of(state: &AppState, id: &str) -> (Option<String>, bool, String, String) {
        state
            .with_db(|db| {
                Ok(db
                    .query_row(
                        "SELECT album_artist, album_artist_read, title, updated_at FROM tracks WHERE id = ?1",
                        [id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .unwrap())
            })
            .unwrap()
    }

    /// （ソート用のタグ, 読み込み済みか）
    fn sort_tags_of(state: &AppState, id: &str) -> (SortTags, bool) {
        state
            .with_db(|db| {
                Ok(db
                    .query_row(
                        "SELECT title_sort, artist_sort, album_sort, album_artist_sort, sort_tags_read
                         FROM tracks WHERE id = ?1",
                        [id],
                        |row| {
                            Ok((
                                SortTags {
                                    title: row.get(0)?,
                                    artist: row.get(1)?,
                                    album: row.get(2)?,
                                    album_artist: row.get(3)?,
                                },
                                row.get(4)?,
                            ))
                        },
                    )
                    .unwrap())
            })
            .unwrap()
    }

    fn album_artist(name: &str) -> BackfillTags {
        BackfillTags {
            album_artist: Some(name.to_string()),
            ..Default::default()
        }
    }

    fn artist_sort(reading: &str) -> SortTags {
        SortTags {
            artist: Some(reading.to_string()),
            ..Default::default()
        }
    }

    /// 最初の値だけを読んでいたアーティスト・ジャンルを、ファイルのすべての値に置き換える
    #[test]
    fn test_backfill_widens_values_that_were_read_as_the_first_one() {
        let state = setup_state();
        // t1: 最初の値だけを読んでいた曲。t2: アプリで編集した曲。t3: もともと1つの値の曲
        for (id, artist, genre) in [
            ("t1", "曲のアーティスト", "Rock"),
            ("t2", "編集したアーティスト", "Rock"),
            ("t3", "曲のアーティスト", "Rock"),
        ] {
            insert_unread_track(&state, id, "曲");
            state
                .with_db(|db| {
                    db.execute(
                        "UPDATE tracks SET artist = ?2, genre = ?3 WHERE id = ?1",
                        rusqlite::params![id, artist, genre],
                    )
                    .unwrap();
                    Ok(())
                })
                .unwrap();
        }

        let result = backfill(&state, |path| {
            let single = path.ends_with("t3.mp3");
            Ok(BackfillTags {
                artist: Some(if single {
                    "曲のアーティスト".to_string()
                } else {
                    "曲のアーティスト; ゲスト".to_string()
                }),
                genre: Some(if single { "Rock" } else { "Rock; Pop" }.to_string()),
                ..Default::default()
            })
        })
        .unwrap();

        assert_eq!(result.read_count, 3);
        // 値が変わったのは、t1（アーティスト・ジャンル）とt2（ジャンル）
        assert_eq!(result.found_count, 2);
        let values = |id: &str| {
            state
                .with_db(|db| {
                    Ok(db
                        .query_row(
                            "SELECT artist, genre, multi_values_read FROM tracks WHERE id = ?1",
                            [id],
                            |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, String>(1)?,
                                    row.get::<_, bool>(2)?,
                                ))
                            },
                        )
                        .unwrap())
                })
                .unwrap()
        };
        assert_eq!(
            values("t1"),
            (
                "曲のアーティスト; ゲスト".to_string(),
                "Rock; Pop".to_string(),
                true
            )
        );
        // 編集した値は変えない（最初の値のままの項目だけを置き換える）
        assert_eq!(
            values("t2"),
            (
                "編集したアーティスト".to_string(),
                "Rock; Pop".to_string(),
                true
            )
        );
        assert_eq!(
            values("t3"),
            ("曲のアーティスト".to_string(), "Rock".to_string(), true)
        );

        // 読み込み済みのトラックは、もう読まない
        let again = backfill(&state, |_| panic!("読み込み済みのトラックを読んだ")).unwrap();
        assert_eq!(again.read_count, 0);
    }

    #[test]
    fn test_all_values_replaces_only_first_value_reads() {
        assert_eq!(all_values(Some("A"), Some("A; B")), Some("A; B"));
        // もともと1つの値・すでに複数の値・編集した値・値のない項目は、変えない
        assert_eq!(all_values(Some("A"), Some("A")), None);
        assert_eq!(all_values(Some("A;B"), Some("A; B")), None);
        assert_eq!(all_values(Some("C"), Some("A; B")), None);
        assert_eq!(all_values(None, Some("A; B")), None);
        assert_eq!(all_values(Some("A"), None), None);
    }

    #[test]
    fn test_backfill_reads_album_artist_and_keeps_other_fields() {
        let state = setup_state();
        insert_unread_track(&state, "t1", "DBだけで編集したタイトル");
        insert_unread_track(&state, "t2", "曲2");

        let result = backfill(&state, |path| {
            Ok(if path.ends_with("t1.mp3") {
                album_artist("Various Artists")
            } else {
                BackfillTags::default()
            })
        })
        .unwrap();

        assert_eq!(
            result,
            BackfillResult {
                read_count: 2,
                found_count: 1,
                failed_count: 0
            }
        );
        // アルバムアーティストだけを変える（タイトル・更新日時は変えない）
        assert_eq!(
            album_artist_of(&state, "t1"),
            (
                Some("Various Artists".to_string()),
                true,
                "DBだけで編集したタイトル".to_string(),
                "2026-01-01T00:00:00Z".to_string()
            )
        );
        // タグのない曲も、読み込み済みにする
        assert_eq!(album_artist_of(&state, "t2").0, None);
        assert!(album_artist_of(&state, "t2").1);
        assert_eq!(sort_tags_of(&state, "t2"), (SortTags::default(), true));

        // 2回目は読むトラックがない
        let again = backfill(&state, |_| panic!("読み込み済みのトラックを読んだ")).unwrap();
        assert_eq!(again, BackfillResult::default());
    }

    #[test]
    fn test_backfill_reads_sort_tags() {
        let state = setup_state();
        insert_unread_track(&state, "t1", "曲1");
        insert_unread_track(&state, "t2", "曲2");

        let result = backfill(&state, |path| {
            Ok(BackfillTags {
                album_artist: None,
                sort_tags: if path.ends_with("t1.mp3") {
                    artist_sort("しいなりんご")
                } else {
                    SortTags::default()
                },
                ..Default::default()
            })
        })
        .unwrap();

        assert_eq!(
            result,
            BackfillResult {
                read_count: 2,
                found_count: 1,
                failed_count: 0
            }
        );
        assert_eq!(
            sort_tags_of(&state, "t1"),
            (artist_sort("しいなりんご"), true)
        );
        assert_eq!(sort_tags_of(&state, "t2"), (SortTags::default(), true));
        // タイトル・更新日時は変えない
        assert_eq!(album_artist_of(&state, "t1").2, "曲1");
        assert_eq!(album_artist_of(&state, "t1").3, "2026-01-01T00:00:00Z");
    }

    /// アルバムアーティストは読み込み済みで、ソート用のタグだけが未読のトラック
    /// （ソート用のタグの列を追加する前のバージョンで登録・編集したトラック）
    #[test]
    fn test_backfill_reads_only_unread_fields() {
        let state = setup_state();
        insert_unread_track(&state, "t1", "曲1");
        state
            .with_db(|db| {
                db.execute(
                    "UPDATE tracks SET album_artist = 'アプリで編集した値', album_artist_read = 1
                     WHERE id = 't1'",
                    [],
                )
                .unwrap();
                Ok(())
            })
            .unwrap();

        let result = backfill(&state, |_| {
            Ok(BackfillTags {
                album_artist: Some("ファイルの値".to_string()),
                sort_tags: artist_sort("よみ"),
                ..Default::default()
            })
        })
        .unwrap();

        assert_eq!(result.read_count, 1);
        assert_eq!(result.found_count, 1);
        // 読み込み済みのアルバムアーティストは変えない
        assert_eq!(
            album_artist_of(&state, "t1").0,
            Some("アプリで編集した値".to_string())
        );
        assert_eq!(sort_tags_of(&state, "t1"), (artist_sort("よみ"), true));

        // 未読だった項目にタグがなければ、「タグあり」に数えない
        insert_unread_track(&state, "t2", "曲2");
        state
            .with_db(|db| {
                db.execute(
                    "UPDATE tracks SET album_artist_read = 1 WHERE id = 't2'",
                    [],
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
        let result = backfill(&state, |_| Ok(album_artist("ファイルの値"))).unwrap();
        assert_eq!((result.read_count, result.found_count), (1, 0));
        assert_eq!(album_artist_of(&state, "t2").0, None);
    }

    #[test]
    fn test_backfill_leaves_unreadable_files_unread() {
        let state = setup_state();
        insert_unread_track(&state, "t1", "曲1");
        insert_unread_track(&state, "t2", "曲2");

        let result = backfill(&state, |path| {
            if path.ends_with("t1.mp3") {
                Err(AppError::Metadata("読めない".to_string()))
            } else {
                Ok(album_artist("アーティスト"))
            }
        })
        .unwrap();

        assert_eq!(
            result,
            BackfillResult {
                read_count: 1,
                found_count: 1,
                failed_count: 1
            }
        );
        assert!(!album_artist_of(&state, "t1").1);
        assert!(!sort_tags_of(&state, "t1").1);

        // 読めなかったトラックは、次回に読み直す
        let retried = backfill(&state, |_| Ok(album_artist("後で読めた"))).unwrap();
        assert_eq!(retried.read_count, 1);
        assert_eq!(
            album_artist_of(&state, "t1").0,
            Some("後で読めた".to_string())
        );
        assert_eq!(
            album_artist_of(&state, "t2").0,
            Some("アーティスト".to_string())
        );
    }

    #[test]
    fn test_backfill_processes_more_tracks_than_one_batch() {
        let state = setup_state();
        let count = BATCH_SIZE as usize * 2 + 10;
        for i in 0..count {
            insert_unread_track(&state, &format!("t{i}"), "曲");
        }

        let result = backfill(&state, |_| Ok(BackfillTags::default())).unwrap();

        assert_eq!(result.read_count, count);
        let unread: i64 = state
            .with_db(|db| {
                Ok(db
                    .query_row(
                        "SELECT COUNT(*) FROM tracks
                         WHERE album_artist_read = 0 OR sort_tags_read = 0",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap())
            })
            .unwrap();
        assert_eq!(unread, 0);
    }
}
