/**
 * 再生の補助（再生速度・「この曲が終わったら停止」・スリープタイマー）の状態
 *
 * どれも、その場かぎりの指示として扱う（保存しない。「この曲が終わったら停止」とスリープタイマーは、
 * 止めた後に自動で解除する。再生速度は、起動のたびに1倍に戻る）。
 * 状態を読んでエンジンへ伝える・再生を止めるのは、再生コントローラー（`./playback.svelte.ts`）が行う。
 */

/** 選べる再生速度の範囲（倍。Rust側の`MIN_SPEED`・`MAX_SPEED`と同じ） */
export const MIN_PLAYBACK_SPEED = 0.5;
export const MAX_PLAYBACK_SPEED = 2;

/** 再生速度の刻み（倍） */
export const PLAYBACK_SPEED_STEP = 0.05;

/** すぐに選べる再生速度（倍） */
export const PLAYBACK_SPEED_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2] as const;

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
   * 再生速度（倍。1が通常の速さ）
   *
   * 音程は変えない。再生位置・曲の長さは、速度を変えても元の曲の時間のまま。
   * 変える時は`setSpeed`を使う（範囲と刻みに合わせる）。
   */
  speed = $state(1);

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
   * 再生速度を変える（範囲の外の値は範囲の中に収め、刻みに合わせて丸める。数値でない値は無視する）
   * @param speed - 再生速度（倍）
   */
  setSpeed(speed: number): void {
    if (!Number.isFinite(speed)) return;
    const clamped = Math.min(MAX_PLAYBACK_SPEED, Math.max(MIN_PLAYBACK_SPEED, speed));
    // 刻みの倍数にする（0.05の倍数は2進数で表せないため、小数第2位で丸め直す）
    const stepped = Math.round(clamped / PLAYBACK_SPEED_STEP) * PLAYBACK_SPEED_STEP;
    this.speed = Math.round(stepped * 100) / 100;
  }

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

  /** すべての指示を解除し、再生速度を1倍に戻す（再生コントローラーを破棄する時・テスト用） */
  reset(): void {
    this.speed = 1;
    this.stopAfterCurrent = false;
    this.cancelSleepTimer();
  }
}

export const playbackAids = new PlaybackAids();
