/**
 * 形式の変換の状態
 *
 * 変換はRust側で進み、進捗がイベントで届く。変換の画面を閉じても続き、終わると通知が出る。
 */
import { events } from '#lib/bindings.js';
import type { ConvertOptions, ConvertProgress, ConvertResult, Track } from '#lib/types/models.js';
import { cancelConversion, convertTracks } from '#lib/queries/convert.js';
import { handleError, showSuccess, showWarning } from '#lib/stores/error.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { stopListening } from '#lib/utils/listen.js';

/**
 * 結果を、通知に出す文にする
 *
 * @internal テストのためにexportしている（アプリでは、このモジュールの中だけで使う）
 */
export function conversionSummary(result: ConvertResult): string {
  const message = result.cancelled ? m.convert.cancelledResult : m.convert.result;
  return message(result.convertedCount, result.skippedCount, result.failedCount);
}

class ConversionState {
  /** 変換を実行中か（書き出すフォルダを選んでいる間を含む） */
  isRunning = $state(false);
  /** 中断を頼んでから、止まるまでの間 */
  isCancelling = $state(false);
  /** 進み具合（1曲も変換が済んでいない間はnull） */
  progress = $state.raw<ConvertProgress | null>(null);

  /**
   * 変換の進捗の通知を受け取り始める（メインウィンドウを表示している間、呼んでおく）
   * @returns 受け取るのをやめる関数
   */
  watch(): () => void {
    const unlisten = events.convertProgress.listen((event) => {
      if (this.isRunning) this.progress = event.payload;
    });
    return () => {
      stopListening(unlisten);
    };
  }

  /**
   * 変換を始め、終わるまで待つ（結果は、通知にも出す）
   * @returns 結果（書き出すフォルダを選ばなかった・始められなかった場合はnull）
   */
  async start(trackIds: string[], options: ConvertOptions): Promise<ConvertResult | null> {
    if (this.isRunning) return null;
    this.isRunning = true;
    this.isCancelling = false;
    this.progress = null;
    try {
      const result = await convertTracks(trackIds, options);
      if (result === null) return null;
      const summary = conversionSummary(result);
      if (result.failedCount > 0 || result.cancelled) {
        showWarning(summary);
      } else {
        showSuccess(summary);
      }
      return result;
    } catch (e) {
      handleError(e, m.operations.convert);
      return null;
    } finally {
      this.isRunning = false;
      this.isCancelling = false;
      this.progress = null;
    }
  }

  /** 実行中の変換を中断する（変換の途中の曲は、出力を残さない） */
  async cancel(): Promise<void> {
    if (!this.isRunning || this.isCancelling) return;
    this.isCancelling = true;
    try {
      await cancelConversion();
    } catch (e) {
      this.isCancelling = false;
      handleError(e, m.operations.convert);
    }
  }
}

/** 形式の変換（曲のメニューの「形式を変換...」が使う） */
export const conversion = new ConversionState();

class ConvertDialogState {
  /** 変換する曲（一覧の順。閉じている間はnull） */
  tracks = $state.raw<Track[] | null>(null);

  /** 選んだ曲の、形式の変換の画面を開く（曲がなければ開かない） */
  open(tracks: Track[]): void {
    if (tracks.length > 0) this.tracks = tracks;
  }

  close(): void {
    this.tracks = null;
  }
}

/** 形式の変換の画面（曲のメニューから開き、`(app)/+layout.svelte`が表示する） */
export const convertDialog = new ConvertDialogState();
