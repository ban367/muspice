//! ネイティブの再生エンジン
//!
//! 音声ファイルを`symphonia`でデコードし、`cpal`で出力する。WebViewのaudio要素では
//! できないこと（出力デバイスの選択、OSのWebViewに依存しない形式への対応）のために使う
//! （採用の経緯は`docs/design/decisions.md`のADR-024・ADR-025）。
//!
//! - `engine`: エンジン本体（コマンドの処理、曲の切り替え、再生位置の通知）
//! - `decoder`: ファイルのデコード（`mp4_gapless`は、MP4のAACの頭と終わりの余分の読み取り）
//! - `convert`: 出力の形式（ステレオ・出力のサンプルレート）への変換
//! - `render`: 出力のコールバックでの音声の取り出し（音量・一時停止）
//! - `output`: 出力デバイスの一覧と、出力のストリーム

mod convert;
mod decoder;
mod engine;
mod mp4_gapless;
mod output;
mod render;

pub use engine::{PlayRequest, PlaybackEngine, PlaybackTrackInfo};
pub use output::{OutputDevice, list_output_devices};
