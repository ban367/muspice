/**
 * 再生回数・スキップ回数に数えるかどうかの判定
 *
 * 再生コントローラー（`./playback.svelte.ts`）が、曲の再生の始まり・再生位置の通知・曲の終わりを
 * ここへ伝える。ここでは「実際に鳴らした時間」を数え、次のように決める（ADR-030）。
 *
 * - 再生回数: 鳴らした時間の合計が、曲の長さの半分か4分（短いほう）に届いた時に1回
 *   - シークで飛ばした分は含めない（再生位置が続けて進んだ分だけを足す）
 *   - 曲の長さが分からない場合は、4分か、曲の終わりまで再生した時
 *   - 同じ曲を頭から再生し直した場合（1曲リピートの繰り返し・「前へ」での頭出し）は、
 *     聴いた時間を数え直す（届くたびに1回）
 * - スキップ回数: 再生回数に数える前に、別の曲を再生し始めた時に1回
 *   - 鳴らした時間が2秒に満たない曲は数えない（続けて次へ送っている途中の曲）
 *   - 停止・キューの終わり・再生の失敗・アプリの終了では数えない
 */

/** 再生回数に数えるまでに聴く時間の上限（秒）。曲の半分がこれより長ければ、この時間で数える */
export const PLAYED_SECONDS_LIMIT = 240;
/** スキップに数える、聴いた時間の下限（秒） */
export const SKIP_MIN_SECONDS = 2;
/**
 * 再生位置の通知の間で「続けて再生した」とみなす、進んだ時間の上限（秒）
 *
 * 再生位置は0.25秒ごとに届く。これより大きく進んだ・戻った場合は、シークとして足さない。
 */
const MAX_PROGRESS_STEP_SECONDS = 1.5;

export interface PlayTrackerCallbacks {
  /** 再生回数に数える */
  onPlayed: (trackId: string) => void;
  /** スキップ回数に数える */
  onSkipped: (trackId: string) => void;
}

export interface PlayTracker {
  /**
   * 曲を頭から再生し始めた
   *
   * 前の曲を再生回数に数える前に別の曲へ移った場合は、前の曲をスキップに数える。
   * @param duration 曲の長さ（秒。分からなければ0）
   */
  begin(trackId: string, duration: number): void;
  /** 再生中の曲の長さ（秒）が分かった（再生エンジンがファイルから読んだ値） */
  setDuration(duration: number): void;
  /** 再生位置（秒）が届いた */
  progress(position: number): void;
  /** 再生中の曲が、最後まで再生された（続けて再生する曲へ切り替わった場合を含む） */
  finish(): void;
  /** 再生をやめた（停止・キューの終わり・再生の失敗）。スキップには数えない */
  reset(): void;
}

interface Session {
  trackId: string;
  duration: number;
  /** 鳴らした時間の合計（秒） */
  listened: number;
  /** 最後に届いた再生位置（秒） */
  position: number;
  /** 再生回数に数えたか */
  counted: boolean;
}

/** 再生回数に数えるのに必要な、聴いた時間（秒） */
function playedThreshold(duration: number): number {
  return duration > 0 ? Math.min(duration / 2, PLAYED_SECONDS_LIMIT) : PLAYED_SECONDS_LIMIT;
}

export function createPlayTracker(callbacks: PlayTrackerCallbacks): PlayTracker {
  /** 再生中の曲（頭から再生し始めてから、終わるか別の曲へ移るまで） */
  let session: Session | null = null;

  function countIfPlayed(current: Session): void {
    if (current.counted || current.listened < playedThreshold(current.duration)) return;
    current.counted = true;
    callbacks.onPlayed(current.trackId);
  }

  return {
    begin(trackId, duration) {
      const previous = session;
      if (
        previous &&
        previous.trackId !== trackId &&
        !previous.counted &&
        previous.listened >= SKIP_MIN_SECONDS
      ) {
        callbacks.onSkipped(previous.trackId);
      }
      session = { trackId, duration, listened: 0, position: 0, counted: false };
    },

    setDuration(duration) {
      if (!session) return;
      session.duration = duration;
      countIfPlayed(session);
    },

    progress(position) {
      if (!session) return;
      const step = position - session.position;
      session.position = position;
      if (step > 0 && step <= MAX_PROGRESS_STEP_SECONDS) {
        session.listened += step;
        countIfPlayed(session);
      }
    },

    finish() {
      const current = session;
      session = null;
      // 長さが分からない曲は、聴いた時間では判定できないため、終わりまで再生した時に数える
      if (current && !current.counted && current.duration <= 0) {
        callbacks.onPlayed(current.trackId);
      }
    },

    reset() {
      session = null;
    }
  };
}
