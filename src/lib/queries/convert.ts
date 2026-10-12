/**
 * 形式の変換の呼び出し
 *
 * 変換はRust側がバックグラウンドで進め、進捗をイベント（`ConvertProgress`）で知らせる。
 * 画面の状態は`#lib/stores/convert.svelte`が持つ。
 */
import { createQuery } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type { ConvertOptions, ConvertPlan, ConvertResult } from '#lib/types/models.js';
import { queryKeys } from './keys';

/**
 * 変換先にできる形式を取得するクエリ
 *
 * この環境で変換できなければ、空（形式の変換は、macOSだけで使える）。起動中は変わらない。
 */
export function useConvertFormatsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.convertFormats,
    queryFn: () => commands.getConvertFormats(),
    staleTime: Infinity
  }));
}

/**
 * 選んだ曲の、変換の計画（曲ごとの出力先と、変換するかどうか）を取得する
 *
 * 指定を変えるたびに取り直すため、キャッシュしない。書式の誤りは`VALIDATION`のエラーで返り、
 * 画面がその場に表示する（トーストは出さない）。
 */
export function fetchConversionPlan(
  trackIds: string[],
  options: ConvertOptions
): Promise<ConvertPlan> {
  return commands.planConversion(trackIds, options);
}

/**
 * 選んだ曲を変換する（終わるまで返らない）
 * @returns 結果（書き出すフォルダを選ばなかった場合はnull）
 */
export function convertTracks(
  trackIds: string[],
  options: ConvertOptions
): Promise<ConvertResult | null> {
  return commands.convertTracks(trackIds, options);
}

/** 実行中の変換を中断する */
export function cancelConversion(): Promise<null> {
  return commands.cancelConversion();
}
