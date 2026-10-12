import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PlaybackEvent, Track } from '#lib/types/models.js';
import type { QueryClient } from '@tanstack/svelte-query';
import type { RestoredPlaybackState } from '#lib/bindings.js';
import { commands } from '#lib/bindings.js';
import { recordPlay, recordSkip } from '#lib/queries/tracks.js';
import { equalizer } from './equalizer.svelte.js';
import { createPlaybackController, type PlaybackController } from './playback.svelte.js';
import { playbackAids, SLEEP_FADE_SECONDS } from './playbackAids.svelte.js';
import {
  player,
  playTrackFromQueue,
  removeFromQueue,
  resetPlayer,
  toggleRepeat,
  toggleShuffle
} from './player.svelte.js';
import { notifications } from './error.svelte.js';

/** エンジンからの通知を受け取るリスナー（`events.playbackEvent.listen`で登録されたもの） */
const listeners = vi.hoisted(() => new Set<(event: { payload: unknown }) => void>());

vi.mock('#lib/bindings.js', () => ({
  commands: {
    playbackPlay: vi.fn(async () => ({ duration: 200 })),
    playbackSetNext: vi.fn(async () => null),
    playbackPause: vi.fn(async () => null),
    playbackResume: vi.fn(async () => null),
    playbackSeek: vi.fn(async () => null),
    playbackSetVolume: vi.fn(async () => null),
    playbackSetEqualizer: vi.fn(async () => null),
    playbackStop: vi.fn(async () => null),
    setNowPlaying: vi.fn(async () => null),
    setCurrentTrack: vi.fn(async () => null),
    getPlaybackState: vi.fn(async () => ({
      volume: 1,
      shuffle: false,
      repeat: 'off',
      queue: [],
      originalTrackIds: null,
      currentIndex: null
    })),
    savePlaybackState: vi.fn(async () => null)
  },
  events: {
    playbackEvent: {
      listen: vi.fn(async (listener: (event: { payload: unknown }) => void) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      })
    }
  }
}));
vi.mock('#lib/queries/tracks.js', () => ({
  recordPlay: vi.fn(async () => {}),
  recordSkip: vi.fn(async () => {})
}));

function makeTrack(id: string): Track {
  return { id, title: id, duration: 180 } as Track;
}

const tracks = ['t1', 't2', 't3'].map(makeTrack);

/** 非同期の処理（コマンドの結果待ち→通知）が終わるのを待つ */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

/** エンジンからの通知を届ける */
function emit(event: PlaybackEvent): void {
  for (const listener of [...listeners]) listener({ payload: event });
  flushSync();
}

/** 直近の`playbackPlay`に渡した番号（その曲についての通知に付ける） */
function lastPlayToken(): number {
  return vi.mocked(commands.playbackPlay).mock.lastCall![1];
}

/** 直近の`playbackSetNext`に渡した、トラックIDと番号 */
function lastNext(): [string | null, number] {
  return vi.mocked(commands.playbackSetNext).mock.lastCall!;
}

let controller: PlaybackController;
let gapless: boolean;
let crossfadeSeconds: number;

/** 保存してある再生状態（何も指定しなければ、何も再生していなかった状態） */
function savedState(overrides: Partial<RestoredPlaybackState> = {}): RestoredPlaybackState {
  return {
    volume: 1,
    shuffle: false,
    repeat: 'off',
    queue: [],
    originalTrackIds: null,
    currentIndex: null,
    ...overrides
  };
}

/** 再生コントローラーを作り直す（保存してある再生状態を変えてから、起動し直す場合） */
async function recreateController(): Promise<void> {
  controller.destroy();
  resetPlayer();
  vi.clearAllMocks();
  controller = createPlaybackController({
    gapless: () => gapless,
    crossfadeSeconds: () => crossfadeSeconds
  });
  flushSync();
  await flush();
}

beforeEach(async () => {
  resetPlayer();
  playbackAids.reset();
  player.isShuffleEnabled = false;
  player.repeatMode = 'off';
  player.volume = 1;
  notifications.clear();
  listeners.clear();
  vi.clearAllMocks();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  // テスト環境にはlocalStorageがないため、イコライザの設定の保存は警告になる
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  equalizer.reset();
  equalizer.setEnabled(false);
  gapless = false;
  crossfadeSeconds = 0;
  controller = createPlaybackController({
    gapless: () => gapless,
    crossfadeSeconds: () => crossfadeSeconds
  });
  // 状態の監視（$effect）はマイクロタスクで始まるため、ここで反映しておく
  flushSync();
  // 前回の再生状態の復元（非同期）が済むのを待つ
  await flush();
});

afterEach(() => {
  controller.destroy();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('トラックの再生', () => {
  it('再生するトラックが決まるとエンジンで再生し、バックエンドへ通知する', async () => {
    playTrackFromQueue(tracks, 1);
    await flush();

    expect(commands.playbackPlay).toHaveBeenCalledWith('t2', expect.any(Number));
    expect(player.isPlaying).toBe(true);
    expect(player.duration).toBe(200);
    expect(commands.setCurrentTrack).toHaveBeenCalledWith('t2');
    // 再生を始めただけでは、再生回数に数えない
    expect(recordPlay).not.toHaveBeenCalled();
  });

  it('エンジンが曲の長さを返さない場合は、トラックの長さを使う', async () => {
    vi.mocked(commands.playbackPlay).mockResolvedValueOnce({ duration: null });

    playTrackFromQueue(tracks, 0);
    await flush();

    expect(player.duration).toBe(180);
  });

  it('再生に失敗したら通知し、再生中の表示にしない', async () => {
    vi.mocked(commands.playbackPlay).mockRejectedValueOnce({
      code: 'PLAYBACK',
      message: '対応していない形式のファイルです'
    });

    playTrackFromQueue(tracks, 0);
    await flush();

    expect(player.isPlaying).toBe(false);
    expect(notifications.items).toHaveLength(1);
    expect(notifications.items[0].message).toContain('トラックの再生に失敗しました');

    // 再生ボタンで、頭から再生し直す
    await controller.togglePlayPause();
    expect(commands.playbackPlay).toHaveBeenCalledTimes(2);
    expect(player.isPlaying).toBe(true);
  });

  it('続けてトラックを変えた場合は、最後のトラックの結果だけを反映する', async () => {
    let resolveFirst: (info: { duration: number }) => void = () => {};
    vi.mocked(commands.playbackPlay).mockImplementationOnce(
      () => new Promise((resolve) => (resolveFirst = resolve))
    );

    playTrackFromQueue(tracks, 0);
    flushSync();
    playTrackFromQueue(tracks, 2);
    await flush();
    resolveFirst({ duration: 999 });
    await flush();

    // エンジンは番号の大きい要求を新しい要求として扱う
    const [first, second] = vi.mocked(commands.playbackPlay).mock.calls;
    expect([first[0], second[0]]).toEqual(['t1', 't3']);
    expect(second[1]).toBeGreaterThan(first[1]);
    expect(player.duration).toBe(200);
    expect(commands.setCurrentTrack).toHaveBeenCalledTimes(1);
    expect(commands.setCurrentTrack).toHaveBeenCalledWith('t3');
  });

  it('再生中と同じトラックを選び直しても、再生し直さない', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    playTrackFromQueue([...tracks], 0);
    await flush();

    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
  });

  it('キューが空になったら再生を止める', async () => {
    playTrackFromQueue([tracks[0]], 0);
    await flush();

    removeFromQueue('t1');
    flushSync();

    expect(commands.playbackStop).toHaveBeenCalledTimes(1);
    expect(player.currentTrack).toBeNull();
  });

  it('音量をエンジンに伝える', () => {
    expect(commands.playbackSetVolume).toHaveBeenLastCalledWith(1);

    player.volume = 0.3;
    flushSync();

    expect(commands.playbackSetVolume).toHaveBeenLastCalledWith(0.3);
  });
});

describe('エンジンからの通知', () => {
  it('再生位置を反映する（前の曲の通知・ドラッグ中の通知は無視する）', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    const token = lastPlayToken();

    emit({ type: 'position', token, position: 12.5 });
    expect(player.currentTime).toBe(12.5);

    emit({ type: 'position', token: token - 1, position: 99 });
    expect(player.currentTime).toBe(12.5);

    controller.setScrubbing(true);
    emit({ type: 'position', token, position: 13 });
    expect(player.currentTime).toBe(12.5);
  });

  it('曲が終わったら、キューの次の曲を再生する', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t2', expect.any(Number));
    expect(commands.setCurrentTrack).toHaveBeenLastCalledWith('t2');
  });

  it('キューの最後の曲が終わったら、再生を終える', async () => {
    playTrackFromQueue(tracks, 2);
    await flush();

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(player.currentTrack).toBeNull();
    expect(player.playQueue).toEqual([]);
    expect(commands.playbackStop).toHaveBeenCalledTimes(1);
  });

  it('1曲リピートでは、曲が終わったら同じ曲を頭から再生し直す', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    toggleRepeat();
    toggleRepeat(); // 1曲リピート

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(commands.playbackPlay).toHaveBeenCalledTimes(2);
    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t1', expect.any(Number));
    expect(player.isPlaying).toBe(true);
  });

  it('前の曲についての終了の通知は無視する', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    const firstToken = lastPlayToken();
    playTrackFromQueue(tracks, 2);
    await flush();

    emit({ type: 'ended', token: firstToken });
    await flush();

    expect(player.currentTrack?.id).toBe('t3');
    expect(commands.playbackPlay).toHaveBeenCalledTimes(2);
  });

  it('再生を続けられなくなったら通知し、再生ボタンで再生し直せる', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    emit({
      type: 'failed',
      token: lastPlayToken(),
      error: { code: 'PLAYBACK', message: 'ファイルを読み取れません' }
    });

    expect(player.isPlaying).toBe(false);
    expect(player.currentTrack?.id).toBe('t1');
    expect(notifications.items).toHaveLength(1);

    await controller.togglePlayPause();
    expect(commands.playbackPlay).toHaveBeenCalledTimes(2);
    expect(player.isPlaying).toBe(true);
  });
});

describe('再生回数とスキップ回数', () => {
  /** 再生位置の通知（0.25秒ごと）を、`from`秒から`to`秒まで届ける */
  function listen(token: number, from: number, to: number): void {
    for (let position = from; position <= to + 1e-9; position += 0.25) {
      for (const listener of [...listeners]) {
        listener({ payload: { type: 'position', token, position } });
      }
    }
    flushSync();
  }

  it('曲の半分を聴いた時に、再生回数に数える', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    // エンジンが返した曲の長さ（200秒）の半分
    listen(lastPlayToken(), 0, 99.75);
    expect(recordPlay).not.toHaveBeenCalled();

    listen(lastPlayToken(), 100, 150);
    expect(recordPlay).toHaveBeenCalledTimes(1);
    expect(recordPlay).toHaveBeenCalledWith('t1', undefined);
    expect(recordSkip).not.toHaveBeenCalled();
  });

  it('シークで飛ばした分は、聴いた時間に含めない', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    const token = lastPlayToken();

    listen(token, 0, 20);
    controller.seek(180);
    listen(token, 180, 200);
    emit({ type: 'ended', token });
    await flush();

    expect(recordPlay).not.toHaveBeenCalled();
    // 最後まで再生したため、スキップにも数えない
    expect(recordSkip).not.toHaveBeenCalled();
  });

  it('数える前に「次へ」で移ったら、スキップに数える', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    listen(lastPlayToken(), 0, 30);

    controller.next();
    await flush();

    expect(recordSkip).toHaveBeenCalledTimes(1);
    expect(recordSkip).toHaveBeenCalledWith('t1', undefined);
    expect(recordPlay).not.toHaveBeenCalled();
  });

  it('再生して2秒に満たない曲・数えた後の曲は、スキップに数えない', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    listen(lastPlayToken(), 0, 1.5);
    controller.next();
    await flush();
    expect(recordSkip).not.toHaveBeenCalled();

    listen(lastPlayToken(), 0, 120);
    expect(recordPlay).toHaveBeenCalledWith('t2', undefined);
    controller.next();
    await flush();
    expect(recordSkip).not.toHaveBeenCalled();
  });

  it('曲が終わって次の曲へ進んでも、スキップに数えない', async () => {
    gapless = true;
    flushSync();
    playTrackFromQueue(tracks, 0);
    await flush();
    const token = lastPlayToken();
    // 終わりの近くへシークして、聴いた時間が足りないまま次の曲へ切り替わった
    listen(token, 0, 10);
    controller.seek(195);
    listen(token, 195, 200);

    emit({ type: 'advanced', token: lastNext()[1], duration: 200 });
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(recordSkip).not.toHaveBeenCalled();
    expect(recordPlay).not.toHaveBeenCalled();
  });

  it('1曲リピートの繰り返しも、半分を聴くたびに数える', async () => {
    gapless = true;
    flushSync();
    playTrackFromQueue(tracks, 0);
    await flush();
    toggleRepeat();
    toggleRepeat(); // 1曲リピート
    flushSync();

    listen(lastPlayToken(), 0, 200);
    expect(recordPlay).toHaveBeenCalledTimes(1);

    const [, nextToken] = lastNext();
    emit({ type: 'advanced', token: nextToken, duration: 200 });
    await flush();
    listen(nextToken, 0, 99);
    expect(recordPlay).toHaveBeenCalledTimes(1);

    listen(nextToken, 99.25, 110);
    expect(recordPlay).toHaveBeenCalledTimes(2);
    expect(recordSkip).not.toHaveBeenCalled();
  });

  it('「前へ」での頭出しは、聴いた時間を数え直す（スキップには数えない）', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    const token = lastPlayToken();
    listen(token, 0, 60);

    // 3秒以上再生しているため、同じ曲の頭へ戻る
    controller.previous();
    flushSync();
    expect(commands.playbackSeek).toHaveBeenLastCalledWith(0);
    listen(token, 0, 60);
    // 戻る前の60秒は含めない
    expect(recordPlay).not.toHaveBeenCalled();

    listen(token, 60.25, 100);
    expect(recordPlay).toHaveBeenCalledTimes(1);
    expect(recordSkip).not.toHaveBeenCalled();
  });

  it('再生を続けられなくなった曲・止めた曲は、スキップに数えない', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    listen(lastPlayToken(), 0, 30);
    emit({
      type: 'failed',
      token: lastPlayToken(),
      error: { code: 'PLAYBACK', message: 'ファイルを読み取れません' }
    });

    playTrackFromQueue(tracks, 1);
    await flush();
    listen(lastPlayToken(), 0, 30);
    resetPlayer();
    flushSync();
    playTrackFromQueue(tracks, 2);
    await flush();

    expect(recordSkip).not.toHaveBeenCalled();
  });

  it('記録した時に反映するキャッシュを渡す', async () => {
    const queryClient = {} as QueryClient;
    controller.destroy();
    resetPlayer();
    controller = createPlaybackController({ queryClient });
    flushSync();
    await flush();

    playTrackFromQueue(tracks, 0);
    await flush();
    listen(lastPlayToken(), 0, 100);

    expect(recordPlay).toHaveBeenCalledWith('t1', queryClient);
  });
});

describe('ギャップレス再生', () => {
  beforeEach(() => {
    gapless = true;
  });

  it('再生を始めたら、次の曲をエンジンに伝える', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    expect(commands.playbackSetNext).toHaveBeenCalledTimes(1);
    expect(lastNext()[0]).toBe('t2');
    expect(lastNext()[1]).toBeGreaterThan(lastPlayToken());
  });

  it('無効の間は、次の曲を伝えない', async () => {
    gapless = false;

    playTrackFromQueue(tracks, 0);
    await flush();

    expect(commands.playbackSetNext).not.toHaveBeenCalled();
  });

  it('クロスフェードが有効なら、ギャップレス再生が無効でも次の曲を伝える', async () => {
    gapless = false;
    crossfadeSeconds = 5;

    playTrackFromQueue(tracks, 0);
    await flush();

    // 重ねる処理はエンジンが行う。重なりが鳴り始めた時点の通知で、キューを進める
    expect(lastNext()[0]).toBe('t2');
    emit({ type: 'advanced', token: lastNext()[1], duration: 240 });
    await flush();
    expect(player.currentTrack?.id).toBe('t2');
    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
  });

  it('キューの次の曲が変わったら伝え直し、次の曲がなくなったら取り消す', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();

    removeFromQueue('t2');
    flushSync();
    expect(lastNext()[0]).toBe('t3');

    removeFromQueue('t3');
    flushSync();
    expect(lastNext()[0]).toBeNull();
    expect(commands.playbackSetNext).toHaveBeenCalledTimes(3);

    // 次の曲が変わらない変更では、伝え直さない
    player.volume = 0.5;
    flushSync();
    expect(commands.playbackSetNext).toHaveBeenCalledTimes(3);
  });

  it('エンジンが次の曲へ切り替わったら、再生し直さずにキューを進める', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    const [, nextToken] = lastNext();

    emit({ type: 'advanced', token: nextToken, duration: 240 });
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(player.currentTime).toBe(0);
    expect(player.duration).toBe(240);
    expect(player.isPlaying).toBe(true);
    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
    expect(commands.setCurrentTrack).toHaveBeenLastCalledWith('t2');
    // その次の曲を伝える
    expect(lastNext()[0]).toBe('t3');

    // 切り替わった後の曲の通知を受け付ける
    emit({ type: 'position', token: nextToken, position: 3 });
    expect(player.currentTime).toBe(3);
  });

  it('1曲リピートでは、同じ曲へ切り替わる', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    toggleRepeat();
    toggleRepeat(); // 1曲リピート
    flushSync();
    expect(lastNext()[0]).toBe('t1');

    emit({ type: 'advanced', token: lastNext()[1], duration: 200 });
    await flush();

    expect(player.currentTrack?.id).toBe('t1');
    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
    // 再生中の曲は変わらないため、もう一度は通知しない
    expect(commands.setCurrentTrack).toHaveBeenCalledTimes(1);
    // 次も同じ曲を伝える（番号は新しくする）
    expect(lastNext()[0]).toBe('t1');
    expect(commands.playbackSetNext).toHaveBeenCalledTimes(3);
  });

  it('伝えた後でキューが変わり、エンジンが古い曲へ切り替わった場合は、キューの次の曲を再生する', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    const [, staleToken] = lastNext();
    // 次の曲（t2）を伝えた後でキューから外したが、エンジンはすでにt2を続けて再生し始めていた
    removeFromQueue('t2');
    flushSync();

    emit({ type: 'advanced', token: staleToken, duration: 200 });
    await flush();

    expect(player.currentTrack?.id).toBe('t3');
    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t3', expect.any(Number));
  });

  it('次の曲の準備に失敗しても、曲の終わりで通常の手順で再生する', async () => {
    vi.mocked(commands.playbackSetNext).mockRejectedValueOnce({
      code: 'NOT_FOUND',
      message: 'ファイルが見つかりません'
    });
    playTrackFromQueue(tracks, 0);
    await flush();
    expect(notifications.items).toHaveLength(0);

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t2', expect.any(Number));
  });
});

describe('この曲が終わったら停止', () => {
  it('曲が終わったら、次の曲へ進めるが再生は始めず、指示を解除する', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    playbackAids.stopAfterCurrent = true;
    flushSync();

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(player.currentTrackIndex).toBe(1);
    expect(player.isPlaying).toBe(false);
    expect(player.currentTime).toBe(0);
    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
    expect(commands.setCurrentTrack).toHaveBeenLastCalledWith('t2');
    expect(playbackAids.stopAfterCurrent).toBe(false);

    // 再生ボタンで、次の曲を頭から再生する
    await controller.togglePlayPause();
    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t2', expect.any(Number));
    expect(player.isPlaying).toBe(true);
  });

  it('有効な間は、ギャップレス再生でも次の曲をエンジンに伝えない', async () => {
    gapless = true;
    playTrackFromQueue(tracks, 0);
    await flush();
    expect(lastNext()[0]).toBe('t2');

    playbackAids.stopAfterCurrent = true;
    flushSync();
    expect(lastNext()[0]).toBeNull();

    // 解除したら、伝え直す
    playbackAids.stopAfterCurrent = false;
    flushSync();
    expect(lastNext()[0]).toBe('t2');
  });

  it('伝えた後で有効になり、エンジンが次の曲へ進んでいた場合も、止める', async () => {
    gapless = true;
    playTrackFromQueue(tracks, 0);
    await flush();
    const [, nextToken] = lastNext();
    // 取り消しがエンジンに届く前に、次の曲へ切り替わった
    playbackAids.stopAfterCurrent = true;
    flushSync();

    emit({ type: 'advanced', token: nextToken, duration: 200 });
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(player.isPlaying).toBe(false);
    expect(commands.playbackStop).toHaveBeenCalled();
    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
  });

  it('キューの最後の曲では、通常の終わり方と同じにする', async () => {
    playTrackFromQueue(tracks, 2);
    await flush();
    playbackAids.stopAfterCurrent = true;
    flushSync();

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(player.currentTrack).toBeNull();
    expect(player.playQueue).toEqual([]);
    expect(playbackAids.stopAfterCurrent).toBe(false);
  });

  it('1曲リピートでも止める（同じ曲のまま、再生し直さない）', async () => {
    toggleRepeat();
    toggleRepeat();
    expect(player.repeatMode).toBe('one');
    playTrackFromQueue(tracks, 0);
    await flush();
    playbackAids.stopAfterCurrent = true;
    flushSync();

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(player.currentTrack?.id).toBe('t1');
    expect(player.isPlaying).toBe(false);
    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
    expect(commands.playbackSeek).not.toHaveBeenCalled();
  });
});

describe('スリープタイマー', () => {
  /** エンジンへ送った音量（送った順） */
  const sentVolumes = (): number[] =>
    vi.mocked(commands.playbackSetVolume).mock.calls.map(([volume]) => volume as number);

  beforeEach(async () => {
    // `flush`（setTimeout）は実際のタイマーのまま、時刻と定期的な確認だけを進められるようにする
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'Date'] });
    player.volume = 0.8;
    playTrackFromQueue(tracks, 0);
    await flush();
    vi.mocked(commands.playbackSetVolume).mockClear();
  });

  it('時間が来たら、音量を少しずつ下げてから一時停止し、音量を設定の値に戻す', async () => {
    playbackAids.startSleepTimer(1, false);
    flushSync();

    vi.advanceTimersByTime(59_000);
    expect(playbackAids.isFadingOut).toBe(false);
    expect(commands.playbackSetVolume).not.toHaveBeenCalled();

    vi.advanceTimersByTime(2_000);
    expect(playbackAids.isFadingOut).toBe(true);
    expect(player.isPlaying).toBe(true);

    vi.advanceTimersByTime((SLEEP_FADE_SECONDS / 2) * 1000);
    const fading = sentVolumes();
    expect(fading.length).toBeGreaterThan(5);
    // 設定の音量から、だんだん下がる
    expect(fading[0]).toBeLessThanOrEqual(0.8);
    expect(fading.at(-1)).toBeLessThan(0.5);
    expect([...fading].sort((a, b) => b - a)).toEqual(fading);
    expect(commands.playbackPause).not.toHaveBeenCalled();

    vi.advanceTimersByTime(SLEEP_FADE_SECONDS * 1000);
    await flush();

    expect(commands.playbackPause).toHaveBeenCalledTimes(1);
    expect(player.isPlaying).toBe(false);
    // 音量の設定は変えず、エンジンの音量は設定の値に戻す
    expect(player.volume).toBe(0.8);
    expect(sentVolumes().at(-1)).toBe(0.8);
    expect(playbackAids.sleepTimer).toBeNull();
    expect(playbackAids.isFadingOut).toBe(false);

    // 止めた後は、何も送らない
    const count = sentVolumes().length;
    vi.advanceTimersByTime(60_000);
    expect(sentVolumes()).toHaveLength(count);
  });

  it('「曲の終わりまで再生する」場合は、時間が来たら「この曲が終わったら停止」にする', async () => {
    playbackAids.startSleepTimer(1, true);
    flushSync();

    vi.advanceTimersByTime(61_000);
    flushSync();

    expect(playbackAids.stopAfterCurrent).toBe(true);
    expect(playbackAids.sleepTimer).toBeNull();
    expect(commands.playbackPause).not.toHaveBeenCalled();
    expect(commands.playbackSetVolume).not.toHaveBeenCalled();

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();
    expect(player.currentTrack?.id).toBe('t2');
    expect(player.isPlaying).toBe(false);
  });

  it('音量を下げている途中で解除したら、音量を戻して再生を続ける', () => {
    playbackAids.startSleepTimer(1, false);
    flushSync();
    vi.advanceTimersByTime(63_000);
    expect(playbackAids.isFadingOut).toBe(true);

    playbackAids.cancelSleepTimer();
    flushSync();
    expect(sentVolumes().at(-1)).toBe(0.8);

    const count = sentVolumes().length;
    vi.advanceTimersByTime(60_000);
    expect(sentVolumes()).toHaveLength(count);
    expect(commands.playbackPause).not.toHaveBeenCalled();
    expect(player.isPlaying).toBe(true);
  });

  it('音量を下げている途中で自分で一時停止したら、音量を戻してタイマーを終える', async () => {
    playbackAids.startSleepTimer(1, false);
    flushSync();
    vi.advanceTimersByTime(63_000);

    await controller.togglePlayPause();

    expect(commands.playbackPause).toHaveBeenCalledTimes(1);
    expect(sentVolumes().at(-1)).toBe(0.8);
    expect(playbackAids.sleepTimer).toBeNull();
    expect(playbackAids.isFadingOut).toBe(false);
  });

  it('時間が来た時に再生していなければ、何もせずに解除する', async () => {
    await controller.togglePlayPause();
    playbackAids.startSleepTimer(1, false);
    flushSync();

    vi.advanceTimersByTime(61_000);

    expect(playbackAids.sleepTimer).toBeNull();
    expect(playbackAids.isFadingOut).toBe(false);
    expect(commands.playbackPause).toHaveBeenCalledTimes(1);
    expect(commands.playbackSetVolume).not.toHaveBeenCalled();
  });

  it('再生位置の通知でも、時間が来たかを確かめる', () => {
    playbackAids.startSleepTimer(1, false);
    flushSync();
    // 定期的な確認が動かないまま（ウィンドウが隠れている間など）、時間だけが過ぎた
    vi.setSystemTime(Date.now() + 61_000);
    expect(playbackAids.isFadingOut).toBe(false);

    emit({ type: 'position', token: lastPlayToken(), position: 61 });

    expect(playbackAids.isFadingOut).toBe(true);
  });

  it('設定し直すと、時間を数え直す', () => {
    playbackAids.startSleepTimer(1, false);
    flushSync();
    vi.advanceTimersByTime(50_000);
    playbackAids.startSleepTimer(1, false);
    flushSync();

    vi.advanceTimersByTime(50_000);
    expect(playbackAids.isFadingOut).toBe(false);
    vi.advanceTimersByTime(11_000);
    expect(playbackAids.isFadingOut).toBe(true);
  });
});

describe('イコライザ', () => {
  it('保存してある設定をエンジンへ送り、変更のたびに送り直す', () => {
    // 作成した時点で送る（無効・すべて0dB）
    expect(commands.playbackSetEqualizer).toHaveBeenLastCalledWith(
      false,
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );

    equalizer.setEnabled(true);
    equalizer.setBandGain(1000, 6);
    flushSync();
    expect(commands.playbackSetEqualizer).toHaveBeenLastCalledWith(
      true,
      [0, 0, 0, 0, 0, 6, 0, 0, 0, 0]
    );

    // プリセットは、31Hzから16kHzの順に送る
    equalizer.applyPreset('bass_boost');
    flushSync();
    expect(commands.playbackSetEqualizer).toHaveBeenLastCalledWith(
      true,
      [8, 6, 4, 2, 0, 0, 0, 0, 0, 0]
    );
  });

  it('破棄した後は、変更を送らない', () => {
    controller.destroy();
    vi.clearAllMocks();

    equalizer.setBandGain(31, -3);
    flushSync();

    expect(commands.playbackSetEqualizer).not.toHaveBeenCalled();
  });
});

describe('操作', () => {
  beforeEach(async () => {
    playTrackFromQueue(tracks, 1);
    await flush();
  });

  it('再生/一時停止を切り替える', async () => {
    await controller.togglePlayPause();
    expect(commands.playbackPause).toHaveBeenCalledTimes(1);
    expect(player.isPlaying).toBe(false);

    await controller.togglePlayPause();
    expect(commands.playbackResume).toHaveBeenCalledTimes(1);
    expect(player.isPlaying).toBe(true);
  });

  it('トラックがなければ、再生/一時停止は何もしない', async () => {
    resetPlayer();
    flushSync();
    vi.clearAllMocks();

    await controller.togglePlayPause();

    expect(commands.playbackPlay).not.toHaveBeenCalled();
    expect(commands.playbackResume).not.toHaveBeenCalled();
  });

  it('曲の長さの範囲でシークする', () => {
    controller.seek(50);
    expect(commands.playbackSeek).toHaveBeenLastCalledWith(50);
    expect(player.currentTime).toBe(50);

    controller.seek(999);
    expect(commands.playbackSeek).toHaveBeenLastCalledWith(200);

    controller.seekBy(-500);
    expect(commands.playbackSeek).toHaveBeenLastCalledWith(0);
  });

  it('シークバーのドラッグ中は、間隔を空けて最後の位置だけをエンジンへ送る', () => {
    vi.useFakeTimers();
    controller.setScrubbing(true);

    controller.seek(10);
    controller.seek(20);
    controller.seek(30);
    // 表示はすぐに追従する
    expect(player.currentTime).toBe(30);
    expect(commands.playbackSeek).not.toHaveBeenCalled();

    vi.advanceTimersByTime(100);
    expect(commands.playbackSeek).toHaveBeenCalledTimes(1);
    expect(commands.playbackSeek).toHaveBeenLastCalledWith(30);

    // ドラッグを終えたら、残りをすぐに送る
    controller.seek(40);
    controller.setScrubbing(false);
    expect(commands.playbackSeek).toHaveBeenCalledTimes(2);
    expect(commands.playbackSeek).toHaveBeenLastCalledWith(40);

    vi.advanceTimersByTime(1000);
    expect(commands.playbackSeek).toHaveBeenCalledTimes(2);
  });

  it('次へ・前へでキューを移動する', async () => {
    controller.next();
    await flush();
    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t3', expect.any(Number));

    controller.previous();
    await flush();
    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t2', expect.any(Number));
  });

  it('3秒以上再生している場合の「前へ」は、再生中のトラックの頭へ戻る', async () => {
    emit({ type: 'position', token: lastPlayToken(), position: 30 });
    await controller.togglePlayPause(); // 一時停止中でも、頭から再生を始める

    controller.previous();

    expect(player.currentTrack?.id).toBe('t2');
    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
    expect(commands.playbackSeek).toHaveBeenLastCalledWith(0);
    expect(commands.playbackResume).toHaveBeenCalledTimes(1);
    expect(player.currentTime).toBe(0);
    expect(player.isPlaying).toBe(true);
  });

  it('破棄すると再生を止め、通知の購読を解除して、再生状態をリセットする', async () => {
    controller.destroy();
    await flush();

    expect(commands.playbackStop).toHaveBeenCalled();
    expect(listeners.size).toBe(0);
    expect(player.currentTrack).toBeNull();

    // 破棄した後に届いた通知は無視する
    player.currentTime = 0;
    emit({ type: 'position', token: lastPlayToken(), position: 5 });
    expect(player.currentTime).toBe(0);
  });
});

describe('OSのNow Playing', () => {
  /** OSへ最後に伝えた内容 */
  const lastNowPlaying = () => vi.mocked(commands.setNowPlaying).mock.lastCall?.[0];

  it('再生を始めたら、曲・再生中かどうか・位置・長さを伝える', async () => {
    playTrackFromQueue(tracks, 1);
    await flush();
    flushSync();
    await flush();

    expect(lastNowPlaying()).toEqual({ trackId: 't2', playing: true, position: 0, duration: 200 });
  });

  it('一時停止・シーク・曲の切り替わりを伝え、再生が進むだけでは伝え直さない', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    flushSync();
    await flush();
    vi.mocked(commands.setNowPlaying).mockClear();

    emit({ type: 'position', token: lastPlayToken(), position: 0.25 });
    await flush();
    expect(commands.setNowPlaying).not.toHaveBeenCalled();

    controller.seek(120);
    flushSync();
    await flush();
    expect(lastNowPlaying()).toMatchObject({ trackId: 't1', playing: true, position: 120 });

    await controller.togglePlayPause();
    flushSync();
    await flush();
    expect(lastNowPlaying()).toMatchObject({ trackId: 't1', playing: false });

    controller.next();
    await flush();
    flushSync();
    await flush();
    expect(lastNowPlaying()).toMatchObject({ trackId: 't2', playing: true, position: 0 });
  });

  it('キューの最後の曲が終わったら、再生している曲がないことを伝える', async () => {
    playTrackFromQueue(tracks, 2);
    await flush();
    flushSync();
    await flush();

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(lastNowPlaying()).toBeNull();
  });

  it('起動して復元した曲は、一時停止中として伝える', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValueOnce(
      savedState({ queue: tracks, currentIndex: 1 })
    );
    await recreateController();
    flushSync();
    await flush();
    expect(player.currentTrack?.id).toBe('t2');
    expect(lastNowPlaying()).toMatchObject({ trackId: 't2', playing: false, position: 0 });

    await controller.togglePlayPause();
    flushSync();
    await flush();
    expect(lastNowPlaying()).toMatchObject({ trackId: 't2', playing: true });
  });

  it('破棄したら、再生している曲がないことを伝える', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    flushSync();
    await flush();

    controller.destroy();
    await flush();

    expect(lastNowPlaying()).toBeNull();
  });
});

describe('再生状態の復元', () => {
  it('前回のキューと再生していた曲を戻すが、再生は始めない', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValueOnce(
      savedState({ volume: 0.4, repeat: 'all', queue: tracks, currentIndex: 1 })
    );
    await recreateController();

    expect(player.currentTrack?.id).toBe('t2');
    expect(player.playQueue.map((t) => t.id)).toEqual(['t1', 't2', 't3']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.duration).toBe(180);
    expect(player.volume).toBe(0.4);
    expect(player.repeatMode).toBe('all');
    expect(player.isPlaying).toBe(false);
    expect(commands.playbackPlay).not.toHaveBeenCalled();
    expect(commands.playbackSetNext).not.toHaveBeenCalled();
  });

  it('復元した曲は、再生ボタンで頭から再生する', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValueOnce(
      savedState({ queue: tracks, currentIndex: 1 })
    );
    await recreateController();

    // エンジンが曲を持っていない間は、シークしても位置を動かさない
    controller.seek(50);
    expect(player.currentTime).toBe(0);
    expect(commands.playbackSeek).not.toHaveBeenCalled();

    await controller.togglePlayPause();

    expect(commands.playbackPlay).toHaveBeenCalledWith('t2', expect.any(Number));
    expect(player.isPlaying).toBe(true);
    expect(commands.setCurrentTrack).toHaveBeenCalledWith('t2');
  });

  it('復元した後の「次へ」は、キューの次の曲を再生する', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValueOnce(
      savedState({ queue: tracks, currentIndex: 0 })
    );
    await recreateController();

    controller.next();
    await flush();

    expect(player.currentTrack?.id).toBe('t2');
    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t2', expect.any(Number));
  });

  it('シャッフルしていた場合は、シャッフルする前の順も戻す', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValueOnce(
      savedState({
        shuffle: true,
        queue: [tracks[2], tracks[0], tracks[1]],
        originalTrackIds: ['t1', 't2', 't3'],
        currentIndex: 0
      })
    );
    await recreateController();

    expect(player.isShuffleEnabled).toBe(true);
    expect(player.playQueue.map((t) => t.id)).toEqual(['t3', 't1', 't2']);
    expect(player.originalQueue.map((t) => t.id)).toEqual(['t1', 't2', 't3']);
    expect(player.currentTrack?.id).toBe('t3');
  });

  it('復元を待つ間に再生を始めていたら、キューは上書きしない', async () => {
    let resolveState: (state: RestoredPlaybackState) => void = () => {};
    vi.mocked(commands.getPlaybackState).mockImplementationOnce(
      () => new Promise((resolve) => (resolveState = resolve))
    );
    controller.destroy();
    resetPlayer();
    vi.clearAllMocks();
    controller = createPlaybackController();
    flushSync();

    playTrackFromQueue([tracks[2]], 0);
    await flush();
    resolveState(savedState({ volume: 0.2, queue: tracks, currentIndex: 0 }));
    await flush();

    expect(player.currentTrack?.id).toBe('t3');
    expect(player.playQueue.map((t) => t.id)).toEqual(['t3']);
    expect(commands.playbackPlay).toHaveBeenCalledTimes(1);
    // 音量などは戻す
    expect(player.volume).toBe(0.2);
  });

  it('再生していた曲がなければ（キューの終わりまで再生した後など）、何も戻さない', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValueOnce(
      savedState({ queue: tracks, currentIndex: null })
    );
    await recreateController();

    expect(player.currentTrack).toBeNull();
    expect(player.playQueue).toEqual([]);
  });

  it('復元に失敗しても、再生はできる', async () => {
    vi.mocked(commands.getPlaybackState).mockRejectedValueOnce({ code: 'IO', message: 'x' });
    await recreateController();

    playTrackFromQueue(tracks, 0);
    await flush();

    expect(commands.playbackPlay).toHaveBeenCalledWith('t1', expect.any(Number));
    expect(notifications.items).toHaveLength(0);
  });
});

describe('再生状態の保存', () => {
  /** 保存のたびに渡した内容 */
  const saves = () => vi.mocked(commands.savePlaybackState).mock.calls;

  it('復元した直後の状態は保存しない', async () => {
    vi.useFakeTimers();
    vi.advanceTimersByTime(1000);

    expect(commands.savePlaybackState).not.toHaveBeenCalled();
  });

  it('キューが変わったら、少し待ってからキューと合わせて保存する', async () => {
    playTrackFromQueue(tracks, 1);
    await flush();
    expect(commands.savePlaybackState).not.toHaveBeenCalled();

    await new Promise((resolve) => setTimeout(resolve, 350));

    expect(saves()).toEqual([
      [
        { volume: 1, shuffle: false, repeat: 'off', currentIndex: 1 },
        { trackIds: ['t1', 't2', 't3'], originalTrackIds: null }
      ]
    ]);
  });

  it('曲の切り替わり・音量・リピートの変更では、キューを送らない', async () => {
    playTrackFromQueue(tracks, 0);
    await new Promise((resolve) => setTimeout(resolve, 350));
    vi.mocked(commands.savePlaybackState).mockClear();
    vi.useFakeTimers();

    // 続けての変更は、まとめて1回で保存する
    player.volume = 0.5;
    flushSync();
    player.volume = 0.3;
    toggleRepeat();
    controller.next();
    flushSync();
    vi.advanceTimersByTime(300);

    expect(saves()).toEqual([
      [{ volume: 0.3, shuffle: false, repeat: 'all', currentIndex: 1 }, null]
    ]);
  });

  it('シャッフルが有効な間は、シャッフルする前の順も保存する', async () => {
    playTrackFromQueue(tracks, 0);
    await new Promise((resolve) => setTimeout(resolve, 350));
    vi.mocked(commands.savePlaybackState).mockClear();
    vi.useFakeTimers();

    toggleShuffle();
    flushSync();
    vi.advanceTimersByTime(300);

    const [cursor, queue] = saves()[0];
    expect(cursor.shuffle).toBe(true);
    expect(queue?.trackIds[0]).toBe('t1');
    expect([...(queue?.trackIds ?? [])].sort()).toEqual(['t1', 't2', 't3']);
    expect(queue?.originalTrackIds).toEqual(['t1', 't2', 't3']);
  });

  it('破棄する時は、待っている変更をすぐに保存し、リセットした後の空の状態は保存しない', async () => {
    playTrackFromQueue(tracks, 2);
    await flush();

    controller.destroy();
    await new Promise((resolve) => setTimeout(resolve, 350));

    expect(saves()).toHaveLength(1);
    expect(saves()[0][0].currentIndex).toBe(2);
    expect(saves()[0][1]?.trackIds).toEqual(['t1', 't2', 't3']);
  });
});
