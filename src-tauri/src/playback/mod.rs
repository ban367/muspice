//! 再生エンジン
//!
//! 音声ファイルを`symphonia`でデコードし、`cpal`で出力する。再生をWebViewのaudio要素に任せず
//! ここで行うのは、出力デバイスを選べるようにし、再生できる形式をOSのWebViewに依存させないため
//! （採用の経緯は`docs/design/decisions.md`のADR-024〜ADR-027）。
//!
//! - `engine`: エンジン本体（コマンドの処理、曲の切り替え、再生位置の通知、音量の正規化、クロスフェード）
//! - `effects`: 出力の直前にかける加工（イコライザ・リミッター）
//! - `normalization`: 音量の正規化（ReplayGain）の倍率の計算
//! - `decoder`: ファイルのデコード（`mp4_gapless`は、MP4のAACの頭と終わりの余分の読み取り）
//! - `convert`: 出力の形式（ステレオ・出力のサンプルレート）への変換
//! - `render`: 出力のコールバックでの音声の取り出し（音量・一時停止）
//! - `output`: 出力デバイスの一覧と、出力のストリーム

mod convert;
mod decoder;
mod effects;
mod engine;
mod mp4_gapless;
mod normalization;
mod output;
mod render;

pub use effects::EQ_BANDS;
pub use engine::{PlayRequest, PlaybackEngine, PlaybackOptions, PlaybackTrackInfo};
pub use output::{OutputDevice, list_output_devices};
