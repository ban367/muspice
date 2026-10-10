/**
 * 再生の補助（「この曲が終わったら停止」とスリープタイマー）の状態
 *
 * どちらも、その場かぎりの指示として扱う（保存せず、止めた後は自動で解除する）。
 * 状態を読んで再生を止めるのは、再生コントローラー（`./playback.svelte.ts`）が行う。
 */

/** スリープタイマーで止める時に、音量を下げていく秒数 */
export const SLEEP_FADE_SECONDS = 10;

/** スリープタイマーで選べる時間（分） */
export const SLEEP_TIMER_MINUTES = [15, 30, 45, 60, 90, 120] as const;

/** スリープタイマーの設定 */
export interface SleepTimer {
  /** 止める時刻（`Date.now()`と比べるミリ秒） */
  endsAt: number;
  /** 設定した時間（分。どれを選んでいるかの表示に使う） */
  minutes: number;
  /** 時間が来たら、再生中の曲の終わりまで再生してから止めるか（しない場合は、音量を下げて止める） */
  waitForTrackEnd: boolean;
}

class PlaybackAids {
  /**
   * 再生中の曲が終わったら、次の曲を再生せずに止めるか
   *
   * 止めた後は、再生コントローラーが解除する。有効な間は、次の曲の先読み
   * （ギャップレス再生・クロスフェード）を行わない。
   */
  stopAfterCurrent = $state(false);

  /** スリープタイマー（設定していなければnull） */
  sleepTimer = $state.raw<SleepTimer | null>(null);

  /** スリープタイマーの時間が来て、音量を下げている途中か（解除すると、音量を戻して再生を続ける） */
  isFadingOut = $state(false);

  /**
   * スリープタイマーを設定する（設定し直すと、時間を数え直す）
   * @param minutes - 止めるまでの時間（分）
   * @param waitForTrackEnd - 時間が来たら、再生中の曲の終わりまで再生してから止めるか
   * @param now - 現在の時刻（ミリ秒）
   */
  startSleepTimer(minutes: number, waitForTrackEnd: boolean, now: number = Date.now()): void {
    if (!Number.isFinite(minutes) || minutes <= 0) return;
    this.isFadingOut = false;
    this.sleepTimer = { endsAt: now + minutes * 60_000, minutes, waitForTrackEnd };
  }

  /** スリープタイマーの「曲の終わりまで再生してから止める」を切り替える（時間は数え直さない） */
  setSleepWaitForTrackEnd(waitForTrackEnd: boolean): void {
    if (this.sleepTimer) this.sleepTimer = { ...this.sleepTimer, waitForTrackEnd };
  }

  /** スリープタイマーを解除する（音量を下げている途中なら、音量を戻して再生を続ける） */
  cancelSleepTimer(): void {
    this.sleepTimer = null;
    this.isFadingOut = false;
  }

  /**
   * スリープタイマーの残りの秒数（設定していなければnull。時間が過ぎていれば0）
   * @param now - 現在の時刻（ミリ秒）
   */
  sleepRemainingSeconds(now: number = Date.now()): number | null {
    if (this.sleepTimer === null) return null;
    return Math.max(0, Math.ceil((this.sleepTimer.endsAt - now) / 1000));
  }

  /** すべての指示を解除する（再生を止めた時・テスト用） */
  reset(): void {
    this.stopAfterCurrent = false;
    this.cancelSleepTimer();
  }
}

export const playbackAids = new PlaybackAids();
