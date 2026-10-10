//! プレイリストのM3U（M3U8）の読み込み・書き出しコマンド
//!
//! 読み書きするファイルは、Rust側で開くダイアログで選ぶ。WebViewからはパスを受け取らないため、
//! WebViewの権限（ファイルを保存するダイアログなど）を増やさずに済む（ADR-005・ADR-032）。

use super::{file_dialog, run_blocking};
use crate::error::{AppError, AppResult};
use crate::m3u;
use crate::models::{M3uExportResult, M3uImportResult};
use crate::settings::Language;
use crate::state::AppState;
use crate::validation::validate_playlist_id;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// ダイアログの文言（設定の言語に合わせる）
struct Labels {
    import_title: &'static str,
    export_title: &'static str,
    filter: &'static str,
}

impl Labels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Ja => Self {
                import_title: "プレイリスト（M3U）を読み込む",
                export_title: "プレイリストを書き出す",
                filter: "プレイリスト",
            },
            Language::En => Self {
                import_title: "Import Playlists (M3U)",
                export_title: "Export Playlist",
                filter: "Playlists",
            },
        }
    }
}

/// M3U（M3U8）のファイルを選んで読み込み、ファイルごとにプレイリストを作る
///
/// 曲の場所は、ライブラリの曲のパスと突き合わせる（別のOSで書き出したパスは、ファイル名と
/// 上のフォルダの一致で対応を付ける）。同じ名前のプレイリストがあれば、番号を付けた名前で作る。
/// ファイルを選ばなかった場合は、空の一覧を返す。
#[tauri::command]
#[specta::specta]
pub async fn import_m3u_playlists(app: AppHandle) -> AppResult<Vec<M3uImportResult>> {
    run_blocking(move || {
        let labels = Labels::for_language(crate::menu::current_language(&app));
        let Some(files) = file_dialog(&app)
            .set_title(labels.import_title)
            .add_filter(labels.filter, &["m3u", "m3u8"])
            .blocking_pick_files()
        else {
            return Ok(Vec::new());
        };
        let paths: Vec<PathBuf> = files
            .into_iter()
            .filter_map(|file| file.into_path().ok())
            .collect();
        import_files(&app.state::<AppState>(), &paths)
    })
    .await
}

/// M3Uのファイルを読み込み、ファイルごとにプレイリストを作る
fn import_files(state: &AppState, paths: &[PathBuf]) -> AppResult<Vec<M3uImportResult>> {
    // ライブラリの曲のパスは、まとめて1回だけ読む（ファイルの読み込みの間は、DBロックを持たない）
    let matcher =
        m3u::TrackMatcher::new(state.with_db(|db| crate::repository::find_track_paths(db))?);
    Ok(paths
        .iter()
        .map(|path| import_file(state, &matcher, path))
        .collect())
}

fn import_file(state: &AppState, matcher: &m3u::TrackMatcher, path: &Path) -> M3uImportResult {
    let mut result = M3uImportResult {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        playlist_id: None,
        playlist_name: None,
        added_count: 0,
        duplicate_count: 0,
        unmatched_count: 0,
        unmatched: Vec::new(),
        error: None,
    };

    let read = match m3u::read_playlist(path, matcher) {
        Ok(read) => read,
        Err(e) => {
            log::warn!("M3Uを読めません: {}: {}", path.display(), e);
            result.error = Some(e.to_string());
            return result;
        }
    };
    result.duplicate_count = read.duplicate_count;
    result.unmatched_count = read.unmatched_count;
    result.unmatched = read.unmatched;

    // 対応する曲が1つもなければ、空のプレイリストは作らない
    if read.track_ids.is_empty() {
        return result;
    }

    let created = state.with_db(|db| {
        crate::playlist::create_playlist_with_tracks(db, &read.name, &read.track_ids)
            .map_err(|e| AppError::Database(format!("プレイリストの作成に失敗しました: {}", e)))
    });
    match created {
        Ok(playlist) => {
            result.added_count = playlist.tracks.len() as u32;
            result.playlist_id = Some(playlist.id);
            result.playlist_name = Some(playlist.name);
        }
        Err(e) => {
            log::error!("M3Uからプレイリストを作れません: {}: {}", path.display(), e);
            result.error = Some(e.to_string());
        }
    }
    result
}

/// プレイリストを、保存先を選んでM3U8（UTF-8の拡張M3U）へ書き出す
///
/// `relative_paths`がtrueなら、曲の場所を書き出し先のフォルダからの相対パスで書く（falseは絶対パス）。
/// 保存先を選ばなかった場合はnullを返す。
#[tauri::command]
#[specta::specta]
pub async fn export_playlist_m3u(
    playlist_id: String,
    relative_paths: bool,
    app: AppHandle,
) -> AppResult<Option<M3uExportResult>> {
    validate_playlist_id(&playlist_id)?;

    run_blocking(move || {
        let state = app.state::<AppState>();
        let name = state.with_db(|db| {
            crate::playlist::get_playlist_name(db, &playlist_id).map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound("プレイリストが見つかりません".to_string())
                }
                e => AppError::Database(format!("プレイリストの取得に失敗しました: {}", e)),
            })
        })?;

        let labels = Labels::for_language(crate::menu::current_language(&app));
        let Some(file) = file_dialog(&app)
            .set_title(labels.export_title)
            .add_filter(labels.filter, &["m3u8"])
            .set_file_name(format!("{}.m3u8", name))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|e| AppError::Io(format!("保存先を扱えません: {}", e)))?;

        export_playlist(
            &state,
            &playlist_id,
            &with_m3u_extension(path),
            relative_paths,
        )
        .map(Some)
    })
    .await
}

/// 拡張子がM3U（`.m3u8`・`.m3u`）でなければ、`.m3u8`を付ける
fn with_m3u_extension(path: PathBuf) -> PathBuf {
    let is_m3u = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("m3u8") || extension.eq_ignore_ascii_case("m3u")
        });
    if is_m3u {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".m3u8");
    PathBuf::from(name)
}

/// プレイリストの曲を、M3U8のファイルへ書き出す
fn export_playlist(
    state: &AppState,
    playlist_id: &str,
    path: &Path,
    relative_paths: bool,
) -> AppResult<M3uExportResult> {
    let context = state.smart_playlist_context();
    let tracks =
        state.with_db(|db| crate::playlist::get_playlist_tracks(db, playlist_id, context))?;
    let base = if relative_paths { path.parent() } else { None };

    std::fs::write(path, m3u::build_m3u8(&tracks, base))
        .map_err(|e| AppError::Io(format!("プレイリストを書き出せません: {}", e)))?;

    Ok(M3uExportResult {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        track_count: tracks.len() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    struct Fixture {
        state: AppState,
        dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let conn = Connection::open_in_memory().unwrap();
            crate::db::run_migrations(&conn).unwrap();
            let dir =
                std::env::temp_dir().join(format!("muspice-m3u-cmd-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let fixture = Self {
                state: AppState::new(conn),
                dir,
            };
            for (id, path, title) in [
                (
                    "t1",
                    "/Users/me/Music/Aoi Sora/Blue Horizon/01 青い地平線.mp3",
                    "青い地平線",
                ),
                (
                    "t2",
                    "/Users/me/Music/Aoi Sora/Blue Horizon/02 Paper Planes.mp3",
                    "Paper Planes",
                ),
                (
                    "t3",
                    "/Users/me/Music/ネオン通り/夜明け/01 Intro.flac",
                    "Intro",
                ),
            ] {
                fixture.insert_track(id, path, title);
            }
            fixture
        }

        fn insert_track(&self, id: &str, path: &str, title: &str) {
            self.state
                .with_db(|db| {
                    db.execute(
                        "INSERT INTO tracks (id, file_path, file_name, title, artist, duration,
                                             format, file_size, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, 'Artist', 200, 'mp3', 1000,
                                 datetime('now'), datetime('now'))",
                        rusqlite::params![id, path, path.rsplit('/').next().unwrap(), title],
                    )
                    .unwrap();
                    Ok(())
                })
                .unwrap();
        }

        fn write(&self, name: &str, content: &str) -> PathBuf {
            let path = self.dir.join(name);
            std::fs::write(&path, content).unwrap();
            path
        }

        /// プレイリストの名前と、入っている曲（並び順）
        fn playlists(&self) -> Vec<(String, Vec<String>)> {
            self.state
                .with_db(|db| {
                    let mut playlists = crate::playlist::get_all_playlists(db).unwrap();
                    playlists.sort_by(|a, b| a.name.cmp(&b.name));
                    Ok(playlists
                        .into_iter()
                        .map(|playlist| {
                            let context = self.state.smart_playlist_context();
                            let tracks =
                                crate::playlist::get_playlist_tracks(db, &playlist.id, context)
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
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_import_creates_a_playlist_per_file_in_the_written_order() {
        let fixture = Fixture::new();
        let windows = fixture.write(
            "通勤.m3u8",
            "#EXTM3U\r\n\
             C:\\Music\\ネオン通り\\夜明け\\01 Intro.flac\r\n\
             C:\\Music\\Aoi Sora\\Blue Horizon\\01 青い地平線.mp3\r\n\
             C:\\Music\\Unknown\\99.mp3\r\n\
             C:\\Music\\ネオン通り\\夜明け\\01 Intro.flac\r\n",
        );
        let local = fixture.write(
            "作業用.m3u",
            "/Users/me/Music/Aoi Sora/Blue Horizon/02 Paper Planes.mp3\n",
        );

        let results = import_files(&fixture.state, &[windows, local]).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].file_name, "通勤.m3u8");
        assert_eq!(results[0].playlist_name.as_deref(), Some("通勤"));
        assert!(results[0].playlist_id.is_some());
        assert_eq!(results[0].added_count, 2);
        assert_eq!(results[0].duplicate_count, 1);
        assert_eq!(results[0].unmatched_count, 1);
        assert_eq!(results[0].unmatched, vec!["C:\\Music\\Unknown\\99.mp3"]);
        assert_eq!(results[0].error, None);
        assert_eq!(results[1].added_count, 1);
        assert_eq!(
            fixture.playlists(),
            vec![
                ("作業用".to_string(), vec!["t2".to_string()]),
                ("通勤".to_string(), vec!["t3".to_string(), "t1".to_string()]),
            ]
        );
    }

    #[test]
    fn test_import_uses_a_numbered_name_when_the_name_is_taken() {
        let fixture = Fixture::new();
        let path = fixture.write(
            "通勤.m3u8",
            "/Users/me/Music/Aoi Sora/Blue Horizon/01 青い地平線.mp3\n",
        );

        import_files(&fixture.state, std::slice::from_ref(&path)).unwrap();
        let second = import_files(&fixture.state, std::slice::from_ref(&path)).unwrap();
        let third = import_files(&fixture.state, &[path]).unwrap();

        assert_eq!(second[0].playlist_name.as_deref(), Some("通勤 (2)"));
        assert_eq!(third[0].playlist_name.as_deref(), Some("通勤 (3)"));
        // 既存のプレイリストは変えない
        assert_eq!(fixture.playlists().len(), 3);
    }

    #[test]
    fn test_import_creates_no_playlist_when_nothing_matches_or_the_file_is_unreadable() {
        let fixture = Fixture::new();
        let unmatched = fixture.write("知らない曲.m3u8", "C:\\Music\\Unknown\\99.mp3\n# comment\n");
        let missing = fixture.dir.join("ない.m3u8");

        let results = import_files(&fixture.state, &[unmatched, missing]).unwrap();

        assert_eq!(results[0].playlist_id, None);
        assert_eq!(results[0].added_count, 0);
        assert_eq!(results[0].unmatched_count, 1);
        assert_eq!(results[0].error, None);
        assert_eq!(results[1].playlist_id, None);
        assert!(
            results[1]
                .error
                .as_deref()
                .unwrap()
                .contains("ファイルを読めません")
        );
        assert!(fixture.playlists().is_empty());
    }

    #[test]
    fn test_export_writes_the_playlist_and_it_can_be_imported_again() {
        let fixture = Fixture::new();
        // 書き出し先と同じフォルダの下にある曲（相対パスで書ける）
        let local_path = fixture.dir.join("Music").join("04 Local.mp3");
        fixture.insert_track("t4", &local_path.to_string_lossy(), "Local");
        let source = fixture.write(
            "元.m3u8",
            &format!(
                "/Users/me/Music/ネオン通り/夜明け/01 Intro.flac\n{}\n",
                local_path.display()
            ),
        );
        let imported = import_files(&fixture.state, &[source]).unwrap();
        let playlist_id = imported[0].playlist_id.clone().unwrap();

        let absolute = fixture.dir.join("絶対.m3u8");
        let result = export_playlist(&fixture.state, &playlist_id, &absolute, false).unwrap();
        assert_eq!(result.file_name, "絶対.m3u8");
        assert_eq!(result.track_count, 2);
        assert_eq!(
            std::fs::read_to_string(&absolute).unwrap(),
            format!(
                "#EXTM3U\r\n\
                 #EXTINF:200,Artist - Intro\r\n/Users/me/Music/ネオン通り/夜明け/01 Intro.flac\r\n\
                 #EXTINF:200,Artist - Local\r\n{}\r\n",
                local_path.display()
            )
        );

        // 相対パス: 書き出し先のフォルダからのパスで書く
        let relative = fixture.dir.join("相対.m3u8");
        export_playlist(&fixture.state, &playlist_id, &relative, true).unwrap();
        let content = std::fs::read_to_string(&relative).unwrap();
        assert!(content.contains("\r\nMusic/04 Local.mp3\r\n"), "{content}");

        // 書き出したファイルを読み込むと、同じ曲・同じ順になる
        let again = import_files(&fixture.state, &[relative]).unwrap();
        assert_eq!(again[0].added_count, 2);
        assert_eq!(again[0].unmatched_count, 0);
        assert_eq!(
            fixture.playlists().last().unwrap(),
            &("相対".to_string(), vec!["t3".to_string(), "t4".to_string()])
        );
    }

    #[test]
    fn test_with_m3u_extension() {
        assert_eq!(
            with_m3u_extension(PathBuf::from("/tmp/a.m3u8")),
            PathBuf::from("/tmp/a.m3u8")
        );
        assert_eq!(
            with_m3u_extension(PathBuf::from("/tmp/a.M3U")),
            PathBuf::from("/tmp/a.M3U")
        );
        assert_eq!(
            with_m3u_extension(PathBuf::from("/tmp/a")),
            PathBuf::from("/tmp/a.m3u8")
        );
        assert_eq!(
            with_m3u_extension(PathBuf::from("/tmp/a.txt")),
            PathBuf::from("/tmp/a.txt.m3u8")
        );
    }
}
