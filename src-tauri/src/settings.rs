//! アプリケーション設定の永続化
//!
//! アプリデータ配下の`settings.json`に保存する。ファイルがない・読めない・壊れている場合は
//! 既定値を使う（設定のせいでアプリが起動できなくならないようにする）。
//! 項目を追加しても古いファイルを読めるよう、ファイルにない項目は既定値で補う
//! （`Settings`自体に`#[serde(default)]`を付けるとTypeScript側で全項目が省略可能になるため、
//! 読み込み時に既定値とマージしてから解釈する）。

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 既定のアクセントカラー（`app.css`の`--color-primary`と同じ）
pub const DEFAULT_ACCENT_COLOR: &str = "#3b82f6";

/// クロスフェードの最大の秒数
pub const MAX_CROSSFADE_SECONDS: u8 = 12;

/// ライブラリフォルダを定期的に再スキャンする間隔として選べる値（分。0は再スキャンしない）
pub const LIBRARY_SCAN_INTERVALS: [u32; 5] = [0, 15, 30, 60, 360];

/// 起動時に開く画面
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum StartupPage {
    /// 前回開いていた画面
    #[default]
    LastOpened,
    /// 曲一覧
    Songs,
}

/// 画面の配色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// ダーク
    #[default]
    Dark,
    /// ライト
    Light,
    /// OSの設定に従う
    System,
}

/// 音量の正規化（ReplayGain）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum VolumeNormalization {
    /// 補正しない
    #[default]
    Off,
    /// トラック単位のゲインで補正する（ない場合はアルバム単位）
    Track,
    /// アルバム単位のゲインで補正する（ない場合はトラック単位）
    Album,
}

/// アプリケーション設定
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 起動時に開く画面
    pub startup_page: StartupPage,
    /// 画面の配色
    pub theme: Theme,
    /// アクセントカラー（`#rrggbb`）
    pub accent_color: String,
    /// 音量の正規化
    pub volume_normalization: VolumeNormalization,
    /// ギャップレス再生（次の曲を先読みし、曲間に無音を入れずに続ける）
    pub gapless_playback: bool,
    /// クロスフェードの秒数（0でクロスフェードしない）
    pub crossfade_seconds: u8,
    /// ライブラリフォルダを監視し、ファイルの変更を自動で反映する
    pub watch_library_folders: bool,
    /// ライブラリフォルダを定期的に再スキャンする間隔（分。0で再スキャンしない）
    pub library_scan_interval_minutes: u32,
}

impl Settings {
    /// ライブラリフォルダの変更を自動で反映するか（起動時の再スキャンもこれに従う）
    pub fn auto_sync_enabled(&self) -> bool {
        self.watch_library_folders || self.library_scan_interval_minutes > 0
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            startup_page: StartupPage::default(),
            theme: Theme::default(),
            accent_color: DEFAULT_ACCENT_COLOR.to_string(),
            volume_normalization: VolumeNormalization::default(),
            gapless_playback: true,
            crossfade_seconds: 0,
            watch_library_folders: false,
            library_scan_interval_minutes: 0,
        }
    }
}

/// 設定の値を検証する
pub fn validate_settings(settings: &Settings) -> AppResult<()> {
    let color = &settings.accent_color;
    let is_hex_color = color.len() == 7
        && color.starts_with('#')
        && color[1..].chars().all(|c| c.is_ascii_hexdigit());
    if !is_hex_color {
        return Err(AppError::Validation(
            "アクセントカラーは#rrggbb形式で指定してください".to_string(),
        ));
    }
    if settings.crossfade_seconds > MAX_CROSSFADE_SECONDS {
        return Err(AppError::Validation(format!(
            "クロスフェードは0〜{}秒で指定してください",
            MAX_CROSSFADE_SECONDS
        )));
    }
    if !LIBRARY_SCAN_INTERVALS.contains(&settings.library_scan_interval_minutes) {
        return Err(AppError::Validation(
            "再スキャンの間隔が選べる値ではありません".to_string(),
        ));
    }
    Ok(())
}

/// ファイルから設定を読み込む（ない・読めない・不正な場合は既定値）
pub fn load_settings(path: &Path) -> Settings {
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(e) if e.kind() == ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            log::warn!("設定ファイルを読み込めませんでした: {}", e);
            return Settings::default();
        }
    };

    match parse_with_defaults(&json) {
        Ok(settings) if validate_settings(&settings).is_ok() => settings,
        Ok(_) => {
            log::warn!("設定ファイルの値が不正なため既定値を使います");
            Settings::default()
        }
        Err(e) => {
            log::warn!("設定ファイルを解析できないため既定値を使います: {}", e);
            Settings::default()
        }
    }
}

/// JSONを既定値とマージしてから設定として解釈する（ファイルにない項目は既定値になる）
fn parse_with_defaults(json: &str) -> serde_json::Result<Settings> {
    let mut merged = serde_json::to_value(Settings::default())?;
    if let (Some(merged), Value::Object(stored)) =
        (merged.as_object_mut(), serde_json::from_str::<Value>(json)?)
    {
        merged.extend(stored);
    }
    serde_json::from_value(merged)
}

/// 設定をファイルに保存する
///
/// 書き込み途中で中断してもファイルが壊れないよう、一時ファイルに書いてから置き換える。
pub fn write_settings(path: &Path, settings: &Settings) -> AppResult<()> {
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| AppError::Io(format!("設定の変換に失敗しました: {}", e)))?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, json)
        .map_err(|e| AppError::Io(format!("設定の保存に失敗しました: {}", e)))?;
    fs::rename(&temp_path, path)
        .map_err(|e| AppError::Io(format!("設定の保存に失敗しました: {}", e)))
}

/// 現在の設定と保存先（Tauriの管理状態）
pub struct SettingsState {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsState {
    /// 保存先から設定を読み込んで作成する
    pub fn load(path: PathBuf) -> Self {
        let current = Mutex::new(load_settings(&path));
        Self { path, current }
    }

    /// 現在の設定を返す
    pub fn get(&self) -> AppResult<Settings> {
        self.current
            .lock()
            .map(|settings| settings.clone())
            .map_err(|e| AppError::Lock(format!("設定の読み込みに失敗しました: {}", e)))
    }

    /// 設定を検証して保存し、現在の設定を更新する
    pub fn save(&self, settings: Settings) -> AppResult<()> {
        validate_settings(&settings)?;
        let mut current = self
            .current
            .lock()
            .map_err(|e| AppError::Lock(format!("設定の保存に失敗しました: {}", e)))?;
        write_settings(&self.path, &settings)?;
        *current = settings;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テストごとに独立した一時ディレクトリの設定ファイルパス
    fn temp_settings_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muspice-settings-test-{}-{}",
            name,
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn test_missing_file_uses_defaults() {
        let path = temp_settings_path("missing");
        fs::remove_file(&path).ok();
        assert_eq!(load_settings(&path), Settings::default());
    }

    #[test]
    fn test_save_and_load_round_trip() {
        let path = temp_settings_path("round-trip");
        let state = SettingsState::load(path.clone());
        let settings = Settings {
            startup_page: StartupPage::Songs,
            theme: Theme::System,
            accent_color: "#ff8800".to_string(),
            volume_normalization: VolumeNormalization::Album,
            gapless_playback: false,
            crossfade_seconds: 5,
            watch_library_folders: true,
            library_scan_interval_minutes: 60,
        };

        state.save(settings.clone()).unwrap();

        assert_eq!(state.get().unwrap(), settings);
        assert_eq!(load_settings(&path), settings);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn test_missing_fields_are_filled_with_defaults() {
        let path = temp_settings_path("partial");
        fs::write(&path, r#"{ "startupPage": "songs" }"#).unwrap();

        let settings = load_settings(&path);
        assert_eq!(settings.startup_page, StartupPage::Songs);
        assert_eq!(settings.accent_color, DEFAULT_ACCENT_COLOR);
        // 項目を追加する前に保存したファイルでも、新しい項目は既定値になる
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.volume_normalization, VolumeNormalization::Off);
        assert!(settings.gapless_playback);
        assert_eq!(settings.crossfade_seconds, 0);
        assert!(!settings.auto_sync_enabled());
    }

    #[test]
    fn test_corrupted_or_invalid_file_uses_defaults() {
        let path = temp_settings_path("corrupted");
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(load_settings(&path), Settings::default());

        fs::write(&path, r##"{ "accentColor": "red" }"##).unwrap();
        assert_eq!(load_settings(&path), Settings::default());
    }

    #[test]
    fn test_rejects_invalid_accent_color() {
        let state = SettingsState::load(temp_settings_path("invalid"));
        for color in ["red", "#12345", "#1234567", "#gg0000", "3b82f6x"] {
            let settings = Settings {
                accent_color: color.to_string(),
                ..Settings::default()
            };
            assert!(state.save(settings).is_err(), "{} は不正な色", color);
        }
    }

    #[test]
    fn test_rejects_too_long_crossfade() {
        let state = SettingsState::load(temp_settings_path("crossfade"));
        let settings = |crossfade_seconds| Settings {
            crossfade_seconds,
            ..Settings::default()
        };
        assert!(state.save(settings(MAX_CROSSFADE_SECONDS)).is_ok());
        assert!(state.save(settings(MAX_CROSSFADE_SECONDS + 1)).is_err());
    }

    #[test]
    fn test_library_scan_interval_must_be_a_choice() {
        let state = SettingsState::load(temp_settings_path("scan-interval"));
        let settings = |library_scan_interval_minutes| Settings {
            library_scan_interval_minutes,
            ..Settings::default()
        };
        for minutes in LIBRARY_SCAN_INTERVALS {
            assert!(
                state.save(settings(minutes)).is_ok(),
                "{}分は選べる",
                minutes
            );
        }
        assert!(state.save(settings(1)).is_err());
    }

    #[test]
    fn test_auto_sync_enabled() {
        let watch = Settings {
            watch_library_folders: true,
            ..Settings::default()
        };
        let interval = Settings {
            library_scan_interval_minutes: 15,
            ..Settings::default()
        };
        assert!(watch.auto_sync_enabled());
        assert!(interval.auto_sync_enabled());
        assert!(!Settings::default().auto_sync_enabled());
    }
}
