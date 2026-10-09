/**
 * ネイティブの再生エンジン（`src-tauri/src/playback/`）のモック
 *
 * ブラウザでは音を鳴らさず、時計に合わせて再生位置を進め、エンジンと同じ通知
 * （`PlaybackEvent`）を送る。再生・一時停止・シーク・曲の切り替わり（ギャップレス再生・
 * クロスフェード）・曲の終わりを、画面の動きとして確認できる。
 * イコライザ・音量の正規化は、音を鳴らさないため再現しない。
 */
import type { AppError, OutputDevice, PlaybackEvent } from '#lib/types/models.js';

/** 再生位置を通知する間隔（Rustの`POSITION_INTERVAL`と同じ） */
const POSITION_INTERVAL_MS = 250;

/** モックの出力デバイス */
export const MOCK_OUTPUT_DEVICES: OutputDevice[] = [
  { id: 'coreaudio:BuiltInSpeakerDevice', name: 'MacBook Proのスピーカー', isDefault: true },
  { id: 'coreaudio:AppleUSBAudioEngine:DAC:1', name: 'USB Audio DAC', isDefault: false },
  { id: 'coreaudio:00-11-22-33-44-55:output', name: 'AirPods Pro', isDefault: false }
];

export interface MockPlaybackEngineOptions {
  /** エンジンからの通知を送る */
  emit: (event: PlaybackEvent) => void;
  /** 曲の長さ（秒）を返す。再生できない曲（見つからない・ファイルがない）ではエラーを投げる */
  durationOf: (trackId: string) => number | null;
  /** 設定のクロスフェードの秒数を返す（省略時はクロスフェードしない） */
  crossfadeSeconds?: () => number;
}

export interface MockPlaybackEngine {
  play(trackId: string, token: number): { duration: number | null };
  setNext(trackId: string | null, token: number): void;
  pause(): void;
  resume(): void;
  seek(position: number): void;
  stop(): void;
}

interface PlayingTrack {
  token: number;
  trackId: string;
  duration: number | null;
  /** `startedAt`の時点の再生位置（秒） */
  base: number;
  /** 再生を始めた（再開した）時刻（ms）。一時停止中はnull */
  startedAt: number | null;
}

export function createMockPlaybackEngine(options: MockPlaybackEngineOptions): MockPlaybackEngine {
  let current: PlayingTrack | null = null;
  let next: { token: number; trackId: string; duration: number | null } | null = null;
  let latestPlayToken: number | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;

  function position(track: PlayingTrack): number {
    const elapsed = track.startedAt === null ? 0 : (Date.now() - track.startedAt) / 1000;
    return track.base + elapsed;
  }

  function clear(): void {
    current = null;
    next = null;
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }

  /**
   * 続けて再生する曲と重ねる長さ（秒。クロスフェードしない場合は0）
   *
   * エンジンと同じく、設定の秒数を上限に、どちらの曲も長さの半分までにする。
   * 同じ曲の繰り返し（1曲リピート）ではクロスフェードしない。
   */
  function crossfadeLength(track: PlayingTrack): number {
    const setting = options.crossfadeSeconds?.() ?? 0;
    if (setting <= 0 || !next || track.duration === null || next.trackId === track.trackId) {
      return 0;
    }
    return Math.min(setting, track.duration / 2, (next.duration ?? Infinity) / 2);
  }

  /** 時計に合わせて、再生位置の通知・曲の切り替わり・曲の終わりを進める */
  function tick(): void {
    if (!current || current.startedAt === null) return;
    const now = position(current);
    // 曲が切り替わる位置（クロスフェードでは、重なりの始まり）
    const switchAt = current.duration === null ? null : current.duration - crossfadeLength(current);
    if (switchAt === null || now < switchAt) {
      options.emit({ type: 'position', token: current.token, position: now });
      return;
    }

    if (next) {
      // 続けて再生する曲へ切り替わる（切り替わる位置を過ぎた分は、次の曲の再生位置になる）
      current = {
        token: next.token,
        trackId: next.trackId,
        duration: next.duration,
        base: now - switchAt,
        startedAt: Date.now()
      };
      next = null;
      options.emit({ type: 'advanced', token: current.token, duration: current.duration });
      options.emit({ type: 'position', token: current.token, position: current.base });
    } else {
      const { token } = current;
      clear();
      options.emit({ type: 'ended', token });
    }
  }

  return {
    play(trackId, token) {
      // 続けて出した要求が逆の順で届いた場合に、古い要求で新しい再生を止めない
      if (latestPlayToken !== null && token < latestPlayToken) {
        const error: AppError = {
          code: 'PLAYBACK',
          message: '新しい再生の要求があったため、取り消しました'
        };
        throw error;
      }
      latestPlayToken = token;
      clear();
      const duration = options.durationOf(trackId);
      current = { token, trackId, duration, base: 0, startedAt: Date.now() };
      timer = setInterval(tick, POSITION_INTERVAL_MS);
      return { duration };
    },
    setNext(trackId, token) {
      if (trackId === null) {
        next = null;
        return;
      }
      if (!current || (latestPlayToken !== null && token < latestPlayToken)) return;
      next = null;
      next = { token, trackId, duration: options.durationOf(trackId) };
    },
    pause() {
      if (!current || current.startedAt === null) return;
      current = { ...current, base: position(current), startedAt: null };
      options.emit({ type: 'position', token: current.token, position: current.base });
    },
    resume() {
      if (!current || current.startedAt !== null) return;
      current = { ...current, startedAt: Date.now() };
    },
    seek(seconds) {
      if (!current) return;
      const base = Math.max(0, Math.min(seconds, current.duration ?? seconds));
      current = { ...current, base, startedAt: current.startedAt === null ? null : Date.now() };
      options.emit({ type: 'position', token: current.token, position: base });
    },
    stop() {
      clear();
    }
  };
}
