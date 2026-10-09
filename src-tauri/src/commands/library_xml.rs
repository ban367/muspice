//! ライブラリのXML（iTunes形式）の取り込みコマンド
//!
//! ほかのプレーヤー（MusicBee・iTunes / ミュージック）が書き出したライブラリのXMLから、
//! 再生回数・最終再生日・追加日・スキップ回数・お気に入りと、プレイリストを取り込む（ADR-033）。
//! 読み込むファイルは、Rust側で開くダイアログで選ぶ（WebViewからパスを受け取らない）。

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::events::LibraryChanged;
use crate::library_xml;
use crate::m3u::{self, TrackMatcher};
use crate::models::LibraryXmlImportResult;
use crate::settings::Language;
use crate::state::AppState;
use std::path::Path;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

/// ライブラリのXML（iTunes形式）を選んで取り込む
///
/// 曲は、XMLの場所とライブラリの曲のパスで突き合わせる（M3Uの読み込みと同じ）。対応が付いた曲の
/// 再生回数・スキップ回数は多い方、最後に再生した日時は新しい方、追加した日時は古い方にする。
/// XMLでお気に入りの曲は、お気に入りにする。`include_playlists`がtrueなら、プレイリストも作る
/// （同じ名前があれば、番号を付けた名前にする）。ファイルを選ばなかった場合はnullを返す。
#[tauri::command]
#[specta::specta]
pub async fn import_library_xml(
    include_playlists: bool,
    window: WebviewWindow,
    app: AppHandle,
) -> AppResult<Option<LibraryXmlImportResult>> {
    run_blocking(move || {
        let (title, filter) = match crate::menu::current_language(&app) {
            Language::Ja => ("ライブラリのXMLを選ぶ", "ライブラリのXML"),
            Language::En => ("Choose a Library XML File", "Library XML"),
        };
        let Some(file) = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(title)
            .add_filter(filter, &["xml"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|e| AppError::Io(format!("ファイルを扱えません: {}", e)))?;

        let result = import_file(&app.state::<AppState>(), &path, include_playlists)?;
        // ほかのウィンドウ（メインウィンドウ）の曲の一覧・プレイリストを取り直させる
        if let Err(e) = LibraryChanged.emit(&app) {
            log::warn!("ライブラリの変更を通知できません: {}", e);
        }
        Ok(Some(result))
    })
    .await
}

/// ライブラリのXMLを読み、曲の値とプレイリストを取り込む
fn import_file(
    state: &AppState,
    path: &Path,
    include_playlists: bool,
) -> AppResult<LibraryXmlImportResult> {
    // XMLの読み込みと突き合わせは、DBロックの外で行う（大きなライブラリでは時間がかかる）
    let library = library_xml::parse(path)?;
    if library.tracks.is_empty() {
        return Err(AppError::Validation(
            "このファイルには、取り込める曲がありません（iTunes形式のライブラリのXMLを選んでください）"
                .to_string(),
        ));
    }
    let matcher = TrackMatcher::new(state.with_db(|db| crate::repository::find_track_paths(db))?);
    let matched = library_xml::match_tracks(&library, &matcher);

    let updated_count = state.with_db(|db| {
        let current = crate::repository::find_track_stats(db)?;
        let tx = db.transaction().map_err(|e| {
            AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
        })?;
        let mut updated = 0;
        for (track_id, imported) in &matched.stats {
            // 突き合わせた後に外された曲は飛ばす
            let Some(stats) = current.get(track_id) else {
                continue;
            };
            if let Some(merged) = library_xml::merge_stats(stats, imported) {
                crate::repository::update_track_stats(&tx, track_id, &merged)?;
                updated += 1;
            }
        }
        tx.commit().map_err(|e| {
            AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
        })?;
        Ok(updated)
    })?;

    let mut playlist_count = 0;
    let mut skipped_playlist_count = 0;
    if include_playlists {
        for playlist in &library.playlists {
            let track_ids = library_xml::playlist_track_ids(playlist, &matched.track_ids);
            // 曲が1つも入らないプレイリストは作らない
            if track_ids.is_empty() {
                skipped_playlist_count += 1;
                continue;
            }
            let name = m3u::sanitize_playlist_name(&playlist.name);
            state.with_db(|db| {
                crate::playlist::create_playlist_with_tracks(db, &name, &track_ids).map_err(|e| {
                    AppError::Database(format!("プレイリストの作成に失敗しました: {}", e))
                })
            })?;
            playlist_count += 1;
        }
    }

    let unmatched_count = matched.unmatched.len() as u32;
    let mut unmatched = matched.unmatched;
    unmatched.truncate(m3u::MAX_REPORTED_UNMATCHED);

    Ok(LibraryXmlImportResult {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        track_count: library.tracks.len() as u32,
        matched_count: matched.track_ids.len() as u32,
        updated_count,
        unmatched_count,
        unmatched,
        playlist_count,
        skipped_playlist_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library_xml::tests::{SAMPLE_XML, write_sample};
    use rusqlite::Connection;

    fn test_state() -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        let state = AppState::new(conn);
        state
            .with_db(|db| {
                for (id, path, play_count) in [
                    (
                        "t1",
                        "/Users/me/Music/Aoi Sora/Blue Horizon/01 青い地平線.mp3",
                        3,
                    ),
                    (
                        "t2",
                        "/Users/me/Music/Aoi Sora/Blue Horizon/02 Paper Planes.mp3",
                        7,
                    ),
                    ("t3", "/Users/me/Music/Other/03 Untouched.mp3", 1),
                ] {
                    db.execute(
                        "INSERT INTO tracks (id, file_path, file_name, title, format, file_size,
                                             play_count, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?1, 'mp3', 1000, ?4,
                                 '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
                        rusqlite::params![id, path, path.rsplit('/').next().unwrap(), play_count],
                    )
                    .unwrap();
                }
                Ok(())
            })
            .unwrap();
        state
    }

    /// 曲の（再生回数, スキップ回数, 最後に再生した日時, 追加した日時, お気に入り, 更新日時）
    type Row = (i64, i64, Option<String>, String, bool, String);

    fn row(state: &AppState, id: &str) -> Row {
        state
            .with_db(|db| {
                Ok(db
                    .query_row(
                        "SELECT play_count, skip_count, last_played_at, created_at, is_favorite,
                                updated_at
                         FROM tracks WHERE id = ?1",
                        [id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get::<_, i32>(4)? != 0,
                                row.get(5)?,
                            ))
                        },
                    )
                    .unwrap())
            })
            .unwrap()
    }

    fn playlists(state: &AppState) -> Vec<(String, Vec<String>)> {
        state
            .with_db(|db| {
                let playlists = crate::playlist::get_all_playlists(db).unwrap();
                Ok(playlists
                    .into_iter()
                    .map(|playlist| {
                        let tracks = crate::playlist::get_playlist_tracks(db, &playlist.id)
                            .unwrap()
                            .into_iter()
                            .map(|track| track.id)
                            .collect();
                        (playlist.name, tracks)
                    })
                    .collect())
            })
            .unwrap()
    }

    #[test]
    fn test_import_merges_statistics_and_creates_playlists() {
        let state = test_state();
        let path = write_sample(SAMPLE_XML);

        let result = import_file(&state, &path, true).unwrap();

        assert_eq!(
            result,
            LibraryXmlImportResult {
                file_name: "iTunes Music Library.xml".to_string(),
                track_count: 3,
                matched_count: 2,
                updated_count: 2,
                unmatched_count: 1,
                unmatched: vec!["D:/Music/Unknown/99 Nothing.mp3".to_string()],
                playlist_count: 1,
                skipped_playlist_count: 1,
            }
        );
        // 再生回数は多い方、最後に再生した日時・追加した日時はXMLの値。更新日時は変えない
        assert_eq!(
            row(&state, "t1"),
            (
                21,
                0,
                Some("2024-09-25T18:31:11+00:00".to_string()),
                "2019-03-10T22:30:36+00:00".to_string(),
                false,
                "2026-01-01T00:00:00+00:00".to_string()
            )
        );
        // このアプリの再生回数の方が多ければそのまま。XMLでお気に入りの曲は、お気に入りにする
        assert_eq!(
            row(&state, "t2"),
            (
                7,
                4,
                None,
                "2020-01-02T03:04:05+00:00".to_string(),
                true,
                "2026-01-01T00:00:00+00:00".to_string()
            )
        );
        // XMLにない曲は変えない
        assert_eq!(row(&state, "t3").0, 1);
        assert_eq!(row(&state, "t3").3, "2026-01-01T00:00:00+00:00");
        // プレイリスト: 対応が付いた曲だけを、XMLの順で入れる
        assert_eq!(
            playlists(&state),
            vec![("通勤".to_string(), vec!["t2".to_string(), "t1".to_string()])]
        );

        // もう一度取り込んでも、値は変わらない（プレイリストは、番号を付けた名前でもう1つできる）
        let again = import_file(&state, &path, true).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        assert_eq!(again.updated_count, 0);
        assert_eq!(row(&state, "t1").0, 21);
        let mut names: Vec<String> = playlists(&state)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        names.sort();
        assert_eq!(names, vec!["通勤".to_string(), "通勤 (2)".to_string()]);
    }

    #[test]
    fn test_import_without_playlists_only_merges_statistics() {
        let state = test_state();
        let path = write_sample(SAMPLE_XML);

        let result = import_file(&state, &path, false).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();

        assert_eq!(result.updated_count, 2);
        assert_eq!(result.playlist_count, 0);
        assert_eq!(result.skipped_playlist_count, 0);
        assert!(playlists(&state).is_empty());
    }

    #[test]
    fn test_import_rejects_files_without_tracks() {
        let state = test_state();
        let empty = write_sample(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Tracks</key><dict></dict></dict></plist>"#,
        );
        let not_a_plist = write_sample("<html><body>hello</body></html>");

        let results = [
            import_file(&state, &empty, true),
            import_file(&state, &not_a_plist, true),
        ];
        std::fs::remove_dir_all(empty.parent().unwrap()).unwrap();
        std::fs::remove_dir_all(not_a_plist.parent().unwrap()).unwrap();

        for result in results {
            assert!(matches!(result, Err(AppError::Validation(_))), "{result:?}");
        }
        assert_eq!(row(&state, "t1").0, 3);
    }
}
