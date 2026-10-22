/**
 * 音量（ラウドネス）の解析の状態
 *
 * 解析はRust側で進み、進捗と完了がイベントで届く。設定ウィンドウとメインウィンドウの
 * どちらから始めても、`watch()`しているウィンドウのすべてで、進み具合が分かる。
 * 結果の通知（トースト）は、解析を始めたウィンドウが出す。
 */
import { events } from '#lib/bindings.js';
import type { LoudnessScanProgress, LoudnessScanResult, Track } from '#lib/types/models.js';
import {
  cancelLoudnessScan,
  fetchLoudnessScanStatus,
  startLoudnessScan
} from '#lib/queries/loudness.js';
import { handleError, showSuccess, showWarning } from '#lib/stores/error.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { stopListening } from '#lib/utils/listen.js';

/**
 * 結果を、通知に出す文にする
 *
 * @internal テストのためにexportしている（アプリでは、このモジュールの中だけで使う）
 */
export function loudnessScanSummary(result: LoudnessScanResult): string {
  const message = result.cancelled ? m.loudnessScan.cancelledResult : m.loudnessScan.result;
  return message(result.writtenCount, result.unchangedCount, result.errorCount);
}

class LoudnessScanState {
  /** 解析を実行中か */
  isRunning = $state(false);
  /** 中断を頼んでから、止まるまでの間 */
  isCancelling = $state(false);
  /** 進み具合（実行中で、1曲も読み終わっていない間はnull） */
  progress = $state.raw<LoudnessScanProgress | null>(null);
  /** 直前の解析の結果 */
  lastResult = $state.raw<LoudnessScanResult | null>(null);
  /** 音量の情報が欠けている曲の数（まだ取得していなければnull） */
  pendingCount = $state<number | null>(null);

  /**
   * 解析の進捗と完了の通知を受け取り始める（ウィンドウごとに、表示している間だけ呼ぶ）
   * @returns 受け取るのをやめる関数
   */
  watch(): () => void {
    const unlistenProgress = events.loudnessScanProgress.listen((event) => {
      this.isRunning = true;
      this.progress = event.payload;
    });
    const unlistenFinished = events.loudnessScanFinished.listen((event) => {
      this.#finish(event.payload);
      void this.refresh();
    });
    void this.refresh();
    return () => {
      stopListening(unlistenProgress, unlistenFinished);
    };
  }

  /** 解析の状態を取り直す（画面を開いた時・解析が終わった時） */
  async refresh(): Promise<void> {
    try {
      const status = await fetchLoudnessScanStatus();
      this.pendingCount = status.pendingCount;
      this.isRunning = status.running;
      if (!status.running) {
        this.progress = null;
        this.isCancelling = false;
      } else if (this.progress === null && status.total > 0) {
        this.progress = { current: status.current, total: status.total, currentFile: '' };
      }
    } catch (e) {
      handleError(e, m.operations.loudnessScan);
    }
  }

  /**
   * 解析を始め、終わるまで待つ（結果は、通知にも出す）
   * @param trackIds - 解析し直す曲。nullなら、ライブラリの、音量の情報が欠けている曲
   * @returns 結果（始められなかった・失敗した場合はnull）
   */
  async start(trackIds: string[] | null): Promise<LoudnessScanResult | null> {
    if (this.isRunning) return null;
    this.isRunning = true;
    this.isCancelling = false;
    this.progress = null;
    this.lastResult = null;
    try {
      const result = await startLoudnessScan(trackIds);
      this.#finish(result);
      const summary = loudnessScanSummary(result);
      if (result.errorCount > 0 || result.cancelled) {
        showWarning(summary);
      } else {
        showSuccess(summary);
      }
      return result;
    } catch (e) {
      handleError(e, m.operations.loudnessScan);
      // ほかのウィンドウで実行中の場合などに、今の状態に合わせる
      await this.refresh();
      return null;
    }
  }

  /** 選んだ曲を解析し直す（ファイルが見つからない曲は除く） */
  startForTracks(tracks: readonly Track[]): Promise<LoudnessScanResult | null> {
    const trackIds = tracks.filter((track) => !track.isMissing).map((track) => track.id);
    return trackIds.length > 0 ? this.start(trackIds) : Promise.resolve(null);
  }

  /** 実行中の解析を中断する（解析の途中のアルバムには、何も書かれない） */
  async cancel(): Promise<void> {
    if (!this.isRunning || this.isCancelling) return;
    this.isCancelling = true;
    try {
      await cancelLoudnessScan();
    } catch (e) {
      this.isCancelling = false;
      handleError(e, m.operations.loudnessScan);
    }
  }

  #finish(result: LoudnessScanResult): void {
    this.isRunning = false;
    this.isCancelling = false;
    this.progress = null;
    this.lastResult = result;
  }
}

/** 音量の解析（設定ウィンドウの「ライブラリ」と、曲のメニューの「音量を解析...」が使う） */
export const loudnessScan = new LoudnessScanState();

class LoudnessScanDialogState {
  /** 解析する曲（閉じている間はnull） */
  tracks = $state.raw<Track[] | null>(null);

  /** 選んだ曲の、音量の解析の画面を開く（曲がなければ開かない） */
  open(tracks: Track[]): void {
    if (tracks.length > 0) this.tracks = tracks;
  }

  close(): void {
    this.tracks = null;
  }
}

/** 選んだ曲の音量の解析の画面（曲のメニューから開き、`(app)/+layout.svelte`が表示する） */
export const loudnessScanDialog = new LoudnessScanDialogState();
