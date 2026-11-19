/**
 * 曲の解析（テンポ・キー。macOS 27のMusicUnderstanding）の状態（ADR-104）
 *
 * 解析はRust側がバックグラウンドで1曲ずつ進め、状態が変わるたびにイベント（`MusicAnalysisProgress`）で
 * 知らせる。止めるまでに終わらなければ、次の起動で続きから再開する。解析の結果は、AIシャッフルの
 * 「似た曲」が使う。
 */
import { commands, events } from '#lib/bindings.js';
import type { MusicAnalysisStatus } from '#lib/types/models.js';
import { handleError } from '#lib/stores/error.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { stopListening } from '#lib/utils/listen.js';

class MusicAnalysisState {
  /** 解析の状態（まだ取得していなければnull） */
  status = $state.raw<MusicAnalysisStatus | null>(null);
  /** 止めるのを頼んでから、止まるまでの間（解析中の曲が終わるまで） */
  isStopping = $state(false);

  /** 解析していない曲の数（見つからない曲・解析できなかった曲を除く） */
  readonly pendingCount = $derived(
    this.status === null
      ? null
      : Math.max(0, this.status.total - this.status.analyzed - this.status.failed)
  );

  /**
   * 解析の状態の通知を受け取り始める（表示している間だけ呼ぶ）
   *
   * ライブラリが変わった時（`LibraryChanged`。音楽フォルダの変更でライブラリを置き換えた・曲を読み込んだ・
   * 外したなど）も、曲の数が変わるため取り直す（解析していない間は、解析の通知が来ない）。
   * @returns 受け取るのをやめる関数
   */
  watch(): () => void {
    const listeningProgress = events.musicAnalysisProgress.listen((event) =>
      this.#update(event.payload)
    );
    const listeningLibrary = events.libraryChanged.listen(() => void this.refresh());
    void this.refresh();
    return () => stopListening(listeningProgress, listeningLibrary);
  }

  /** 解析の状態を取り直す */
  async refresh(): Promise<void> {
    try {
      this.#update(await commands.getMusicAnalysisStatus());
    } catch (e) {
      handleError(e, m.operations.musicAnalysis);
    }
  }

  /** 解析を、バックグラウンドで始める */
  async start(): Promise<void> {
    if (this.status?.running) return;
    try {
      await commands.startMusicAnalysis();
    } catch (e) {
      handleError(e, m.operations.musicAnalysis);
    }
    await this.refresh();
  }

  /** 解析を止める（解析中の曲が終わってから止まる） */
  async stop(): Promise<void> {
    if (!this.status?.running || this.isStopping) return;
    this.isStopping = true;
    try {
      await commands.stopMusicAnalysis();
    } catch (e) {
      this.isStopping = false;
      handleError(e, m.operations.musicAnalysis);
    }
  }

  /** 解析の結果をすべて消す */
  async deleteAll(): Promise<void> {
    try {
      this.#update(await commands.deleteMusicAnalysis());
    } catch (e) {
      handleError(e, m.operations.musicAnalysis);
    }
  }

  #update(status: MusicAnalysisStatus): void {
    this.status = status;
    if (!status.running) this.isStopping = false;
  }
}

/** 曲の解析（設定ウィンドウの「AI機能」が使う） */
export const musicAnalysis = new MusicAnalysisState();
