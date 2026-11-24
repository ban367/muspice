//! ライブラリの書き出しコマンド（iTunes形式のライブラリXMLとCSV。ADR-121）
//!
//! 保存する場所は、Rust側で開く保存のダイアログで選ぶ（WebViewからパスを受け取らない。ADR-065）。

use super::{completed_save_path, run_blocking};
use crate::error::{AppError, AppResult};
use crate::library::to_count;
use crate::library_export::{ExportPlaylist, build_csv, build_itunes_xml, to_xml_bytes};
use crate::settings::Language;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// 書き出す形式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LibraryExportFormat {
    /// iTunes形式のライブラリXML（曲・評価・再生回数・プレイリスト）
    Xml,
    /// 曲の一覧のCSV
    Csv,
}

impl LibraryExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Xml => "xml",
            Self::Csv => "csv",
        }
    }
}

/// 書き出した結果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryExportResult {
    /// 書き出したファイル
    pub path: String,
    pub track_count: u32,
    /// 書き出したプレイリスト数（CSVは0）
    pub playlist_count: u32,
}

/// ダイアログの文言（アプリの設定の言語に合わせる）
fn labels(language: Language, format: LibraryExportFormat) -> (&'static str, &'static str) {
    match (language, format) {
        (Language::Ja, LibraryExportFormat::Xml) => {
            ("ライブラリをXMLで書き出す", "ライブラリのXML")
        }
        (Language::Ja, LibraryExportFormat::Csv) => ("ライブラリをCSVで書き出す", "CSV"),
        (Language::En, LibraryExportFormat::Xml) => ("Export Library as XML", "Library XML"),
        (Language::En, LibraryExportFormat::Csv) => ("Export Library as CSV", "CSV"),
    }
}

/// ライブラリを、保存のダイアログで選んだファイルへ書き出す（選ばなかった場合はnull。ADR-121）
///
/// XMLは、曲とプレイリスト（オートプレイリストは、書き出す時点で条件に合う曲。プレイリストのフォルダも）。
/// CSVは、曲の一覧（見出しはアプリの言語）。同じ名前のファイルを選んだ場合は置き換える（一時ファイルに
/// 書いてから置き換える）。
#[tauri::command]
#[specta::specta]
pub async fn export_library(
    format: LibraryExportFormat,
    window: WebviewWindow,
    app: AppHandle,
) -> AppResult<Option<LibraryExportResult>> {
    run_blocking(move || {
        let language = crate::menu::current_language(&app);
        let (title, filter) = labels(language, format);
        let Some(file) = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(title)
            .add_filter(filter, &[format.extension()])
            .set_file_name(format!("Muspice Library.{}", format.extension()))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|e| AppError::Io(format!("保存先を扱えません: {}", e)))?;
        let path = completed_save_path(&path, with_extension(path.clone(), format.extension()))?;
        export(&app.state::<AppState>(), format, language, &path).map(Some)
    })
    .await
}

/// 拡張子のない（違う）名前を選んだ場合は、拡張子を足す
fn with_extension(path: PathBuf, extension: &str) -> PathBuf {
    let has_extension = path
        .extension()
        .and_then(|current| current.to_str())
        .is_some_and(|current| current.eq_ignore_ascii_case(extension));
    if has_extension {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".");
    name.push(extension);
    PathBuf::from(name)
}

/// 書き出す途中の一時ファイル（保存先と同じフォルダの、ほかのファイルと重ならない名前）
///
/// `atomic_file::replace_file`は、一時ファイルの名前にあるものを消してから書くため、利用者のファイルと
/// 重ならないよう、`.`で始まる名前にUUIDを入れる。
fn temp_path(path: &Path) -> PathBuf {
    let name = format!(".muspice-export-{}.tmp", uuid::Uuid::new_v4());
    match path.parent() {
        Some(dir) => dir.join(name),
        None => PathBuf::from(name),
    }
}

/// ライブラリを読み、ファイルへ書き出す
fn export(
    state: &AppState,
    format: LibraryExportFormat,
    language: Language,
    path: &Path,
) -> AppResult<LibraryExportResult> {
    let context = state.smart_playlist_context();
    let (tracks, mut playlists, mut smart_playlists, mut folders, music_folder) =
        state.with_db(|db| {
            let tracks = crate::repository::find_all_tracks(db)?;
            let playlists: Vec<(i32, ExportPlaylist)> = crate::playlist::get_all_playlists(db)?
                .into_iter()
                .map(|playlist| {
                    (
                        playlist.position,
                        ExportPlaylist {
                            track_ids: crate::playlist::playlist_track_ids(&playlist),
                            id: playlist.id,
                            name: playlist.name,
                            folder_id: playlist.folder_id,
                        },
                    )
                })
                .collect();
            // オートプレイリストは、今の条件に合う曲の一覧として書き出す（条件は書き出さない。ADR-127）
            let smart_playlists = crate::smart_playlist::get_all_smart_playlists(db)?
                .into_iter()
                .map(|playlist| {
                    Ok((
                        playlist.position,
                        ExportPlaylist {
                            track_ids: crate::smart_playlist::smart_playlist_track_ids(
                                db, &playlist, context,
                            )?,
                            id: playlist.id,
                            name: playlist.name,
                            folder_id: None,
                        },
                    ))
                })
                .collect::<AppResult<Vec<_>>>()?;
            let folders = crate::playlist::get_playlist_folders(db)?;
            let music_folder =
                crate::library_folder::find_music_folder(db)?.map(|folder| folder.path);
            Ok((tracks, playlists, smart_playlists, folders, music_folder))
        })?;
    // プレイリストとフォルダは、サイドバーの手動の並び順にする（オートプレイリストは、その後ろ）
    let by_position = |(a, x): &(i32, ExportPlaylist), (b, y): &(i32, ExportPlaylist)| {
        a.cmp(b).then_with(|| x.name.cmp(&y.name))
    };
    playlists.sort_by(by_position);
    smart_playlists.sort_by(by_position);
    folders.sort_by(|a, b| {
        a.position
            .cmp(&b.position)
            .then_with(|| a.name.cmp(&b.name))
    });
    let playlists: Vec<ExportPlaylist> = playlists
        .into_iter()
        .chain(smart_playlists)
        .map(|(_, p)| p)
        .collect();

    let bytes = match format {
        LibraryExportFormat::Xml => to_xml_bytes(&build_itunes_xml(
            &tracks,
            &playlists,
            &folders,
            music_folder.as_deref(),
            env!("CARGO_PKG_VERSION"),
            jiff::Timestamp::now(),
        ))
        .map_err(|e| AppError::Io(format!("XMLを作れませんでした: {}", e)))?,
        LibraryExportFormat::Csv => build_csv(&tracks, language).into_bytes(),
    };
    crate::atomic_file::replace_file(path, &temp_path(path), &bytes)
        .map_err(|e| AppError::Io(format!("書き出せませんでした: {}", e)))?;
    log::info!(
        "Exported the library ({:?}): tracks={}, playlists={}",
        format,
        tracks.len(),
        playlists.len()
    );

    // XMLには、CUEシートの曲を書かない（`build_itunes_xml`。CSVは、ライブラリの一覧としてすべて書く）
    let track_count = match format {
        LibraryExportFormat::Xml => tracks.iter().filter(|track| track.cue.is_none()).count(),
        LibraryExportFormat::Csv => tracks.len(),
    };
    Ok(LibraryExportResult {
        path: path.to_string_lossy().into_owned(),
        track_count: to_count(track_count)?,
        playlist_count: match format {
            LibraryExportFormat::Xml => to_count(playlists.len())?,
            LibraryExportFormat::Csv => 0,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    #[test]
    fn test_with_extension() {
        assert_eq!(
            with_extension(PathBuf::from("/a/Library"), "xml"),
            PathBuf::from("/a/Library.xml")
        );
        assert_eq!(
            with_extension(PathBuf::from("/a/Library.XML"), "xml"),
            PathBuf::from("/a/Library.XML")
        );
        assert_eq!(
            with_extension(PathBuf::from("/a/Library.txt"), "csv"),
            PathBuf::from("/a/Library.txt.csv")
        );
    }

    #[test]
    fn test_export_writes_xml_and_csv() {
        let state = AppState::new(crate::db::open_in_memory());
        state
            .with_db(|db| {
                db.execute(
                    "INSERT INTO tracks (id, file_path, file_name, title, format, file_size,
                                         created_at, updated_at)
                     VALUES ('a1000000-0000-4000-8000-000000000001', '/Music/a.mp3', 'a.mp3',
                             'Song', 'MP3', 1, '2026-01-01T00:00:00+00:00',
                             '2026-01-01T00:00:00+00:00')",
                    [],
                )
                .unwrap();
                let playlist = crate::playlist::create_playlist(db, "通勤")?;
                crate::playlist::add_tracks_to_playlist(
                    db,
                    &playlist.id,
                    &["a1000000-0000-4000-8000-000000000001".to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        let dir = TempDir::new();

        let xml = dir.path().join("Library.xml");
        let result = export(&state, LibraryExportFormat::Xml, Language::Ja, &xml).unwrap();
        assert_eq!((result.track_count, result.playlist_count), (1, 1));
        let library = crate::library_xml::parse(&xml).unwrap();
        assert_eq!(library.tracks.len(), 1);
        assert_eq!(library.playlists[0].name, "通勤");

        // 保存先の隣にある、同じ名前に`.tmp`を付けたファイルは消さない
        std::fs::write(dir.path().join("Library.csv.tmp"), "user file").unwrap();
        let csv = dir.path().join("Library.csv");
        let result = export(&state, LibraryExportFormat::Csv, Language::En, &csv).unwrap();
        assert_eq!((result.track_count, result.playlist_count), (1, 0));
        let content = std::fs::read_to_string(&csv).unwrap();
        assert!(content.contains("\r\nSong,"));
        // 一時ファイルは残さない（XML・CSV・利用者のファイル）
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("Library.csv.tmp")).unwrap(),
            "user file"
        );
    }
}
