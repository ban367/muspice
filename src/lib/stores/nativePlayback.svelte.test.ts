import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PlaybackEvent, Track } from '#lib/types/models.js';
import { commands } from '#lib/bindings.js';
import { incrementPlayCount } from '#lib/queries/tracks.js';
import { createNativePlaybackController } from './nativePlayback.svelte.js';
import type { PlaybackController } from './playback.svelte.js';
import {
  player,
  playTrackFromQueue,
  removeFromQueue,
  resetPlayer,
  toggleRepeat
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
    playbackStop: vi.fn(async () => null),
    setCurrentTrack: vi.fn(async () => null)
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
  incrementPlayCount: vi.fn(async () => {})
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

beforeEach(() => {
  resetPlayer();
  player.isShuffleEnabled = false;
  player.repeatMode = 'off';
  player.volume = 1;
  notifications.clear();
  listeners.clear();
  vi.clearAllMocks();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  gapless = false;
  controller = createNativePlaybackController({ gapless: () => gapless });
  // 状態の監視（$effect）はマイクロタスクで始まるため、ここで反映しておく
  flushSync();
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
    expect(incrementPlayCount).toHaveBeenCalledWith('t2');
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
    expect(incrementPlayCount).not.toHaveBeenCalled();

    // 再生ボタンで、頭から再生し直す
    await controller.togglePlayPause();
    expect(commands.playbackPlay).toHaveBeenCalledTimes(2);
    expect(player.isPlaying).toBe(true);
    expect(incrementPlayCount).toHaveBeenCalledWith('t1');
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
    expect(incrementPlayCount).toHaveBeenCalledTimes(1);
    expect(incrementPlayCount).toHaveBeenCalledWith('t3');
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
    expect(incrementPlayCount).toHaveBeenLastCalledWith('t2');
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

  it('1曲リピートでは、曲が終わったら同じ曲を頭から再生し直す（再生回数は記録しない）', async () => {
    playTrackFromQueue(tracks, 0);
    await flush();
    toggleRepeat();
    toggleRepeat(); // 1曲リピート

    emit({ type: 'ended', token: lastPlayToken() });
    await flush();

    expect(commands.playbackPlay).toHaveBeenCalledTimes(2);
    expect(commands.playbackPlay).toHaveBeenLastCalledWith('t1', expect.any(Number));
    expect(incrementPlayCount).toHaveBeenCalledTimes(1);
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
    // 同じ曲の再生し直しのため、再生回数はもう一度記録しない
    expect(incrementPlayCount).toHaveBeenCalledTimes(1);
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
    expect(incrementPlayCount).toHaveBeenLastCalledWith('t2');
    // その次の曲を伝える
    expect(lastNext()[0]).toBe('t3');

    // 切り替わった後の曲の通知を受け付ける
    emit({ type: 'position', token: nextToken, position: 3 });
    expect(player.currentTime).toBe(3);
  });

  it('1曲リピートでは同じ曲へ切り替わり、再生回数は記録しない', async () => {
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
    expect(incrementPlayCount).toHaveBeenCalledTimes(1);
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
    vi.spyOn(console, 'warn').mockImplementation(() => {});
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
