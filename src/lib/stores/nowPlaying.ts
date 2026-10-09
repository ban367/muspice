/**
 * OSのNow Playing（macOSのコントロールセンターなど）への通知
 *
 * プレーヤーバーに表示している曲・再生中かどうか・再生位置を、Rust側（`media_controls`）へ伝える。
 * Rust側が、曲の情報とアルバムアートを添えてOSへ渡す。OSは、伝えられたアプリを「再生中のアプリ」
 * として扱い、メディアキーの操作をそこへ届ける。
 *
 * 再生位置は再生中ずっと変わり続けるが、OSは「伝えられた時点の位置」から今の位置を自分で計算して
 * 表示する。そのため、伝えるのは次の場合だけにする。
 *
 * - 曲・再生中かどうか・曲の長さが変わった
 * - 再生位置が、OSの計算とずれた（シークした・再生が途切れた）
 *
 * 呼び出しは1つずつ行う（前の結果を待つ）。続けて呼ぶと、届く順番が入れ替わって古い状態が
 * 残るおそれがあるため。待っている間に状態が変わったら、最後の状態だけを伝える。
 */
import type { NowPlayingUpdate } from '#lib/bindings.js';

/** OSが計算している位置と、実際の位置のずれを許す秒数（これ以上ずれたら、位置を伝え直す） */
const POSITION_TOLERANCE_SECONDS = 1;

/** プレーヤーバーの状態 */
export interface NowPlayingState {
  trackId: string;
  playing: boolean;
  /** 再生位置（秒） */
  position: number;
  /** 曲の長さ（秒。分からなければ0） */
  duration: number;
}

export interface NowPlayingReporter {
  /**
   * プレーヤーバーの状態を伝える（nullは、再生している曲がない）
   *
   * 状態が変わるたびに呼ぶ。OSへ伝え直す必要がなければ、何もしない。
   */
  update(state: NowPlayingState | null): void;
}

/**
 * @param send OSへ伝える関数（`commands.setNowPlaying`）
 * @param now 経過時間を測る時計（ms。テストで差し替える）
 */
export function createNowPlayingReporter(
  send: (update: NowPlayingUpdate | null) => Promise<unknown>,
  now: () => number = () => performance.now()
): NowPlayingReporter {
  /** 最後に伝えた（伝えることにした）状態。何も伝えていなければnull */
  let reported: { key: string; playing: boolean; position: number; at: number } | null = null;
  /** まだ送っていない状態（undefinedは、送るものがない） */
  let pending: NowPlayingUpdate | null | undefined;
  let sending = false;

  async function flush(): Promise<void> {
    if (sending) return;
    sending = true;
    try {
      while (pending !== undefined) {
        const update = pending;
        pending = undefined;
        try {
          await send(update);
        } catch (error) {
          // 表示だけの問題のため、再生は続ける
          console.warn('再生中の曲をOSへ伝えられませんでした:', error);
        }
      }
    } finally {
      sending = false;
    }
  }

  return {
    update(state) {
      if (state === null) {
        if (reported === null) return;
        reported = null;
        pending = null;
        void flush();
        return;
      }

      // 曲の長さは、秒未満の違い（ライブラリの値と、再生エンジンが読んだ値の違い）では伝え直さない
      const key = `${state.trackId}:${state.playing}:${Math.round(state.duration)}`;
      if (reported?.key === key) {
        // OSが計算している、今の位置
        const elapsed = reported.playing ? (now() - reported.at) / 1000 : 0;
        const expected = reported.position + elapsed;
        if (Math.abs(state.position - expected) < POSITION_TOLERANCE_SECONDS) return;
      }

      reported = { key, playing: state.playing, position: state.position, at: now() };
      pending = {
        trackId: state.trackId,
        playing: state.playing,
        position: state.position,
        duration: state.duration > 0 ? state.duration : null
      };
      void flush();
    }
  };
}
