import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { commands } from '#lib/bindings.js';
import { incrementPlayCount } from '#lib/queries/tracks.js';
import {
  createPlaybackController,
  mediaErrorMessage,
  type PlaybackController
} from './playback.svelte.js';
import { player, playTrackFromQueue, resetPlayer } from './player.svelte.js';
import { notifications } from './error.svelte.js';

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
  isEqualizerInitialized: () => true
}));

/** テスト用のaudio要素（再生状態とイベントだけを再現する） */
class FakeAudio extends EventTarget {
  src = '';
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

let audio: FakeAudio;
let controller: PlaybackController;

beforeEach(() => {
  resetPlayer();
  player.isShuffleEnabled = false;
  player.repeatMode = 'off';
  player.volume = 1;
  notifications.clear();
  vi.clearAllMocks();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  audio = new FakeAudio();
  controller = createPlaybackController(audio as unknown as HTMLAudioElement);
  // 状態の監視（$effect）はマイクロタスクで始まるため、ここで反映しておく
  flushSync();
});

afterEach(() => {
  controller.destroy();
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
