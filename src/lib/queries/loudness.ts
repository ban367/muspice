/**
 * 音量（ラウドネス）の解析の呼び出し
 *
 * 解析はRust側がバックグラウンドで進め、進捗と完了をイベント（`LoudnessScanProgress`・
 * `LoudnessScanFinished`）で知らせる。画面の状態は`#lib/stores/loudnessScan.svelte`が持つ
 * （設定ウィンドウとメインウィンドウのどちらから始めても、両方に進捗が出る）。
 */
import { commands } from '#lib/bindings.js';
import type { LoudnessScanResult, LoudnessScanStatus } from '#lib/types/models.js';

/** 解析の状態（実行中か・進み具合・音量の情報が欠けている曲の数）を取得する */
export function fetchLoudnessScanStatus(): Promise<LoudnessScanStatus> {
  return commands.getLoudnessScanStatus();
}

/**
 * 曲の音量を解析して、ゲインとピークをファイルのタグへ書き込む（終わるまで返らない）
 * @param trackIds - 解析し直す曲。nullなら、ライブラリの、音量の情報が欠けている曲
 */
export function startLoudnessScan(trackIds: string[] | null): Promise<LoudnessScanResult> {
  return commands.startLoudnessScan(trackIds);
}

/** 実行中の解析を中断する */
export function cancelLoudnessScan(): Promise<null> {
  return commands.cancelLoudnessScan();
}
