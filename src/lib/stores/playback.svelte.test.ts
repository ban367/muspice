import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Track, VolumeNormalization } from '#lib/types/models.js';
import { commands } from '#lib/bindings.js';
import { incrementPlayCount } from '#lib/queries/tracks.js';
import {
  createPlaybackController,
  mediaErrorMessage,
  type PlaybackController,
  type PlaybackControllerOptions
} from './playback.svelte.js';
import {
  player,
  playTrackFromQueue,
  removeFromQueue,
  resetPlayer,
  toggleRepeat
} from './player.svelte.js';
import { notifications } from './error.svelte.js';
import { setNormalizationGain } from './equalizer.svelte.js';

vi.mock('#lib/bindings.js', () => ({
  commands: {
    getTrackFilePath: vi.fn(async (trackId: string) => `/music/${trackId}.mp3`),
    setCurrentTrack: vi.fn(async () => null)
  }
}));
vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`
}));
vi.mock('#lib/queries/tracks.js', () => ({
  incrementPlayCount: vi.fn(async () => {})
}));
vi.mock('./equalizer.svelte.js', () => ({
  initializeEqualizer: vi.fn(async () => {}),
  cleanupEqualizer: vi.fn(async () => {}),
  resumeAudioContext: vi.fn(async () => {}),
  isEqualizerInitialized: () => true,
  setNormalizationGain: vi.fn()
}));

/** テスト用のaudio要素（再生状態とイベントだけを再現する） */
class FakeAudio extends EventTarget {
  src = '';
  preload = '';
  currentTime = 0;
  duration = 200;
  volume = 1;
  paused = true;
  error: { code: number } | null = null;

  play = vi.fn(async () => {
    if (this.paused) {
      this.paused = false;
      this.dispatchEvent(new Event('play'));
    }
  });

  pause = vi.fn(() => {
    if (!this.paused) {
      this.paused = true;
      this.dispatchEvent(new Event('pause'));
    }
  });

  /** 読み込みを解除する（`removeAttribute('src')`と`load()`の組み合わせだけを再現する） */
  removeAttribute = vi.fn((name: string) => {
    if (name === 'src') this.src = '';
  });
  load = vi.fn();

  /** 再生位置を進めてtimeupdateを発火する */
  advanceTo(time: number) {
    this.currentTime = time;
    this.dispatchEvent(new Event('timeupdate'));
  }

  /** 最後まで再生し終えたことにする */
  finish() {
    this.paused = true;
    this.dispatchEvent(new Event('pause'));
    this.dispatchEvent(new Event('ended'));
  }
}

function makeTrack(id: string): Track {
  return { id, title: id } as Track;
}

const tracks = ['t1', 't2', 't3'].map(makeTrack);

/** 非同期の読み込み（パス取得→再生→通知）が終わるのを待つ */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

/** 1つ目のデッキ（ギャップレス再生が無効の間は、こちらだけを使う） */
let audio: FakeAudio;
/** 2つ目のデッキ（次の曲の先読みに使う） */
let standbyAudio: FakeAudio;
let controller: PlaybackController;

function createController(options: PlaybackControllerOptions = {}): PlaybackController {
  return createPlaybackController(
    [audio, standbyAudio] as unknown as [HTMLAudioElement, HTMLAudioElement],
    options
  );
}

beforeEach(() => {
  resetPlayer();
  player.isShuffleEnabled = false;
  player.repeatMode = 'off';
  player.volume = 1;
  notifications.clear();
  vi.clearAllMocks();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  audio = new FakeAudio();
  standbyAudio = new FakeAudio();
  controller = createController();
  // 状態の監視（$effect）はマイクロタスクで始まるため、ここで反映しておく
  flushSync();
});

afterEach(() => {
  controller.destroy();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('トラックの読み込み', () => {
  it('再生するトラックが決まると読み込んで再生し、バックエンドへ通知する', async () => {
    playTrackFromQueue(tracks, 1);
    await flush();

    expect(audio.src).toBe(`asset://localhost/${encodeURIComponent('/music/t2.mp3')}`);
    expect(audio.paused).toBe(false);
    expect(player.isPlaying).toBe(true);
    expect(commands.setCurrentTrack).toHaveBeenCalledWith('t2');
    expect(incrementPlayCount).toHaveBeenCalledWith('t2');
  });

  it('パスの取得中に別のトラックへ切り替わったら、古いトラックは読み込まない', async () => {
    let resolveFirst: (path: string) => void = () => {};
    vi.mocked(commands.getTrackFilePath)
      .mockImplementationOnce(() => new Promise((resolve) => (resolveFirst = resolve)))
      .mockImplementationOnce(async () => '/music/t2.mp3');

    playTrackFromQueue(tracks, 0);
    // 1曲目のパスを取得している間に、2曲目へ切り替える
    flushSync();
    playTrackFromQueue(tracks, 1);
    await flush();
    resolveFirst('/music/t1.mp3');
    await flush();

    expect(audio.src).toContain('t2.mp3');
    expect(commands.setCurrentTrack).toHaveBeenCalledTimes(1);
    expect(commands.setCurrentTrack).toHaveBeenCalledWith('t2');
  });

  it('同じ処理の中で続けてトラックが変わった場合は、最後のトラックだけを読み込む', async () => {
    playTrackFromQueue(tracks, 0);
    playTrackFromQueue(tracks, 2);
    await flush();

    expect(commands.getTrackFilePath).toHaveBeenCalledTimes(1);
    expect(commands.getTrackFilePath).toHaveBeenCalledWith('t3');
    expect(audio.src).toContain('t3.mp3');
  });

  it('読み込みに失敗したらエラーを通知し、再生中の表示を解除する', async () => {
    vi.mocked(commands.getTrackFilePath).mockRejectedValueOnce({
      code: 'NOT_FOUND',
      message: '指定されたトラックが見つかりません'
    });

    playTrackFromQueue(tracks, 0);
    await flush();

    expect(player.isPlaying).toBe(false);
    expect(notifications.items.at(-1)?.message).toBe(
      'トラックの再生に失敗しました: 指定されたトラックが見つかりません'
    );
  });

  it('キューが空になったら再生を止める', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    resetPlayer();
    flushSync();

    expect(audio.pause).toHaveBeenCalled();
    expect(audio.paused).toBe(true);
  });
});

describe('前へ・次へ', () => {
  it('3秒を超えて再生しているときの「前へ」は、再生中のトラックを頭から再生し直す', async () => {
    playTrackFromQueue(tracks, 1);
    await flush();
    audio.advanceTo(10);

    controller.previous();

    expect(player.currentTrack?.id).toBe('t2');
    expect(audio.currentTime).toBe(0);
    expect(player.currentTime).toBe(0);
    expect(audio.paused).toBe(false);
  });

  it('再生開始から3秒以内の「前へ」は前のトラックへ移る', async () => {
    playTrackFromQueue(tracks, 1);
    await flush();
    audio.advanceTo(2);

    controller.previous();
    await flush();

    expect(player.currentTrack?.id).toBe('t1');
    expect(audio.src).toContain('t1.mp3');
  });

  it('1曲リピート中の「次へ」は同じトラックを頭から再生し直す', async () => {
    player.repeatMode = 'one';
    playTrackFromQueue(tracks, 0);
    await flush();
    audio.advanceTo(42);

    controller.next();

    expect(player.currentTrack?.id).toBe('t1');
    expect(audio.currentTime).toBe(0);
  });
});

describe('トラックの終了', () => {
  it('次のトラックがあれば続けて再生する', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    audio.finish();
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(audio.src).toContain('t2.mp3');
    expect(player.isPlaying).toBe(true);
  });

  it('リピートなしでキューの最後まで再生したら終了する', async () => {
    playTrackFromQueue(tracks, 2);
    await flush();

    audio.finish();

    expect(player.currentTrack).toBeNull();
    expect(player.isPlaying).toBe(false);
  });

  it('1曲リピートでは同じトラックを頭から再生し直す', async () => {
    player.repeatMode = 'one';
    playTrackFromQueue(tracks, 0);
    await flush();
    audio.advanceTo(200);

    audio.finish();

    expect(player.currentTrack?.id).toBe('t1');
    expect(audio.currentTime).toBe(0);
    expect(audio.paused).toBe(false);
  });

  it('1曲だけのキューを全曲リピートしても止まらずに再生し直す', async () => {
    player.repeatMode = 'all';
    playTrackFromQueue([makeTrack('only')], 0);
    await flush();
    audio.advanceTo(200);

    audio.finish();

    expect(player.currentTrack?.id).toBe('only');
    expect(audio.currentTime).toBe(0);
    expect(audio.paused).toBe(false);
  });
});

describe('再生位置と音量', () => {
  it('シーク位置は0から曲の長さの範囲に収める', () => {
    controller.seek(500);
    expect(audio.currentTime).toBe(200);

    controller.seekBy(-1000);
    expect(audio.currentTime).toBe(0);
    expect(player.currentTime).toBe(0);
  });

  it('シークバーをドラッグ中は再生位置の通知で表示を上書きしない', () => {
    controller.setScrubbing(true);
    player.currentTime = 50;
    audio.advanceTo(10);
    expect(player.currentTime).toBe(50);

    controller.setScrubbing(false);
    audio.advanceTo(11);
    expect(player.currentTime).toBe(11);
  });

  it('音量の変更をaudio要素に反映する', () => {
    player.volume = 0.25;
    flushSync();
    expect(audio.volume).toBe(0.25);
  });

  it('破棄した後はaudio要素のイベントを処理しない', () => {
    controller.destroy();
    audio.advanceTo(30);
    expect(player.currentTime).toBe(0);
  });

  it('破棄した後は再生状態の変化に反応しない', async () => {
    controller.destroy();

    playTrackFromQueue(tracks, 0);
    player.volume = 0.5;
    await flush();

    expect(commands.getTrackFilePath).not.toHaveBeenCalled();
    expect(audio.volume).toBe(1);
  });
});

describe('mediaErrorMessage', () => {
  it('再生の中断は表示しない', () => {
    expect(mediaErrorMessage({ code: 1 })).toBeNull();
  });

  it('エラーコードごとのメッセージを返す', () => {
    expect(mediaErrorMessage({ code: 3 })).toContain('デコードエラー');
    expect(mediaErrorMessage({ code: 4 })).toContain('未対応のフォーマット');
    expect(mediaErrorMessage(null)).toBe('再生エラーが発生しました');
  });
});

describe('音量の正規化', () => {
  /** dBを倍率にする */
  const fromDb = (db: number) => 10 ** (db / 20);

  function taggedTrack(id: string, trackGain: number, albumGain: number): Track {
    return {
      id,
      title: id,
      replayGain: { trackGain, trackPeak: null, albumGain, albumPeak: null }
    } as Track;
  }

  /** デッキに最後に設定された補正の倍率 */
  const lastGain = (deck: number) =>
    vi
      .mocked(setNormalizationGain)
      .mock.calls.filter(([index]) => index === deck)
      .at(-1)?.[1];

  it('再生中の曲と設定から補正量を決め、曲や設定が変わると追随する', () => {
    // 既定のコントローラーの代わりに、設定を変えられるコントローラーを使う
    controller.destroy();
    let mode = $state<VolumeNormalization>('track');
    controller = createController({ normalizationMode: () => mode });

    playTrackFromQueue([taggedTrack('a', -6, -9), taggedTrack('b', -3, -9)], 0);
    flushSync();
    expect(lastGain(0)).toBeCloseTo(fromDb(-6));

    mode = 'album';
    flushSync();
    expect(lastGain(0)).toBeCloseTo(fromDb(-9));

    mode = 'track';
    controller.next();
    flushSync();
    expect(lastGain(0)).toBeCloseTo(fromDb(-3));

    mode = 'off';
    flushSync();
    expect(lastGain(0)).toBe(1);
  });

  it('設定を渡さない場合・曲がない場合は補正しない', () => {
    playTrackFromQueue([taggedTrack('a', -6, -9)], 0);
    flushSync();
    expect(lastGain(0)).toBe(1);

    resetPlayer();
    flushSync();
    expect(lastGain(0)).toBe(1);
  });

  it('先読みした曲の補正量は、先読みしたデッキに設定する', async () => {
    controller.destroy();
    controller = createController({ normalizationMode: () => 'track', gapless: () => true });

    playTrackFromQueue([taggedTrack('a', -6, -9), taggedTrack('b', -3, -9)], 0);
    await flush();

    expect(lastGain(0)).toBeCloseTo(fromDb(-6));
    expect(lastGain(1)).toBeCloseTo(fromDb(-3));
  });
});

describe('ギャップレス再生', () => {
  beforeEach(() => {
    controller.destroy();
    controller = createController({ gapless: () => true });
    flushSync();
  });

  /** パスを取得したトラックのID */
  const requestedPaths = () => vi.mocked(commands.getTrackFilePath).mock.calls.map(([id]) => id);

  it('再生中に次の曲をもう一方のデッキへ先読みする（再生はしない）', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    expect(audio.src).toContain('t1.mp3');
    expect(standbyAudio.src).toContain('t2.mp3');
    expect(standbyAudio.preload).toBe('auto');
    expect(standbyAudio.paused).toBe(true);
    expect(commands.setCurrentTrack).toHaveBeenCalledTimes(1);
  });

  it('曲が終わると、先読みしたデッキで読み込み直さずに続けて再生する', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    audio.finish();
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(standbyAudio.paused).toBe(false);
    expect(player.isPlaying).toBe(true);
    expect(commands.setCurrentTrack).toHaveBeenLastCalledWith('t2');
    expect(incrementPlayCount).toHaveBeenLastCalledWith('t2');
    // t2は読み込み直さず、空いたデッキに次のt3を先読みする
    expect(requestedPaths()).toEqual(['t1', 't2', 't3']);
    expect(audio.src).toContain('t3.mp3');
    expect(audio.paused).toBe(true);
  });

  it('曲の終わりの直前に次の曲を再生し始め、前の曲は最後まで鳴らす', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    vi.useFakeTimers();

    audio.advanceTo(199.5);
    expect(standbyAudio.paused).toBe(true);
    audio.pause.mockClear();

    // タイマーが発火するまでに再生が進む
    audio.currentTime = 199.98;
    await vi.advanceTimersByTimeAsync(500);

    expect(standbyAudio.paused).toBe(false);
    expect(player.currentTrack?.id).toBe('t2');
    expect(player.currentTime).toBe(0);
    // 前の曲は止めない
    expect(audio.pause).not.toHaveBeenCalled();
    expect(audio.paused).toBe(false);

    // 前の曲の再生位置の通知は、表示に反映しない
    audio.advanceTo(199.99);
    expect(player.currentTime).toBe(0);

    // 前の曲が鳴り終わってから、次の曲を先読みする
    expect(audio.src).toContain('t1.mp3');
    audio.finish();
    await vi.advanceTimersByTimeAsync(0);
    expect(audio.src).toContain('t3.mp3');
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('タイマーが早く発火しても、曲の終わりの直前まで待ち直す', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    vi.useFakeTimers();

    audio.advanceTo(199.5);
    // 再生が遅れていて、まだ0.3秒残っている
    audio.currentTime = 199.7;
    await vi.advanceTimersByTimeAsync(500);
    expect(standbyAudio.paused).toBe(true);

    audio.currentTime = 199.99;
    await vi.advanceTimersByTimeAsync(300);
    expect(standbyAudio.paused).toBe(false);
  });

  it('一時停止・シークしたら、用意した切り替えを取りやめる', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    vi.useFakeTimers();

    audio.advanceTo(199.5);
    await controller.togglePlayPause();
    audio.currentTime = 199.99;
    await vi.advanceTimersByTimeAsync(1000);
    expect(standbyAudio.paused).toBe(true);
    expect(player.currentTrack?.id).toBe('t1');

    await controller.togglePlayPause();
    audio.advanceTo(199.5);
    controller.seek(10);
    await vi.advanceTimersByTimeAsync(1000);
    expect(standbyAudio.paused).toBe(true);
  });

  it('「次へ」で先読みした曲へ移るときは、読み込み直さずに切り替える', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    audio.advanceTo(42);

    controller.next();
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(standbyAudio.paused).toBe(false);
    expect(standbyAudio.currentTime).toBe(0);
    expect(audio.paused).toBe(true);
    expect(requestedPaths()).toEqual(['t1', 't2', 't3']);
  });

  it('キューの操作で次の曲が変わったら、先読みし直す', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    expect(standbyAudio.src).toContain('t2.mp3');

    removeFromQueue('t2');
    await flush();
    expect(standbyAudio.src).toContain('t3.mp3');

    // 1曲リピートでは再生中の曲を先読みする
    toggleRepeat(); // all
    toggleRepeat(); // one
    await flush();
    expect(standbyAudio.src).toContain('t1.mp3');

    vi.mocked(incrementPlayCount).mockClear();
    audio.advanceTo(200);
    audio.finish();
    await flush();
    expect(player.currentTrack?.id).toBe('t1');
    expect(standbyAudio.paused).toBe(false);
    // 同じ曲の繰り返しは、従来（頭からの再生し直し）と同じく再生回数に数えない
    expect(incrementPlayCount).not.toHaveBeenCalled();
    // 鳴り終わったデッキは、次の繰り返しに備えて頭へ戻しておく
    expect(audio.currentTime).toBe(0);
  });

  it('次の曲がなければ先読みを解除する', async () => {
    playTrackFromQueue(tracks, 1);
    await flush();
    expect(standbyAudio.src).toContain('t3.mp3');

    controller.next();
    await flush();
    // t3を再生中で、次はない
    expect(player.currentTrack?.id).toBe('t3');
    expect(audio.removeAttribute).toHaveBeenCalledWith('src');
    expect(audio.load).toHaveBeenCalled();
  });

  it('先読みに失敗したら、曲の終わりで通常どおり読み込む', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    vi.mocked(commands.getTrackFilePath)
      .mockImplementationOnce(async () => '/music/t1.mp3')
      .mockRejectedValueOnce(new Error('先読みの失敗'));

    playTrackFromQueue(tracks, 0);
    await flush();
    expect(standbyAudio.src).toBe('');

    audio.finish();
    await flush();
    expect(player.currentTrack?.id).toBe('t2');
    expect(audio.src).toContain('t2.mp3');
    expect(audio.paused).toBe(false);
  });

  it('先読みしたファイルを再生できない場合は、エラーを表示せずに通常の読み込みへ戻す', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    playTrackFromQueue(tracks, 0);
    await flush();

    standbyAudio.error = { code: 4 };
    standbyAudio.dispatchEvent(new Event('error'));
    expect(notifications.items).toHaveLength(0);

    audio.finish();
    await flush();
    expect(audio.src).toContain('t2.mp3');
  });

  it('ギャップレス再生を無効にすると先読みを解除し、曲の終わりで読み込む', async () => {
    controller.destroy();
    let gapless = $state(true);
    controller = createController({ gapless: () => gapless });

    playTrackFromQueue(tracks, 0);
    await flush();
    expect(standbyAudio.src).toContain('t2.mp3');

    gapless = false;
    flushSync();
    expect(standbyAudio.src).toBe('');
    expect(standbyAudio.load).toHaveBeenCalled();

    audio.finish();
    await flush();
    expect(audio.src).toContain('t2.mp3');
  });
});
