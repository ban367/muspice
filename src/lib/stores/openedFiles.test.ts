import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { OpenedFiles } from '#lib/bindings.js';
import type { Track } from '#lib/types/models.js';

/** 「開かれた」の知らせを聞いている関数 */
const listeners = new Set<() => void>();

vi.mock('#lib/bindings.js', () => ({
  commands: {
    takeOpenedFiles: vi.fn()
  },
  events: {
    tracksOpened: {
      listen: vi.fn(async (listener: () => void) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      })
    }
  }
}));

import { commands } from '#lib/bindings.js';
import { notifications } from './error.svelte.js';
import { watchOpenedFiles } from './openedFiles.js';
import { player, resetPlayer } from './player.svelte.js';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function makeTrack(id: string): Track {
  return { id, title: id, duration: 180 } as Track;
}

function opened(overrides: Partial<OpenedFiles> = {}): OpenedFiles {
  return { tracks: [], errors: [], ...overrides };
}

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  resetPlayer();
  notifications.clear();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  // 受け取った分は残らない（指定した1回の後は、空を返す）
  vi.mocked(commands.takeOpenedFiles).mockResolvedValue(opened());
});

describe('watchOpenedFiles', () => {
  it('起動の途中に開かれていた曲を、始めた時に受け取って再生する', async () => {
    vi.mocked(commands.takeOpenedFiles).mockResolvedValueOnce(
      opened({ tracks: [makeTrack('a'), makeTrack('b')] })
    );

    const stop = watchOpenedFiles();
    await flush();

    expect(player.currentTrack?.id).toBe('a');
    expect(player.playQueue.map((track) => track.id)).toEqual(['a', 'b']);
    stop();
  });

  it('聞き始める手続きの間に開かれた曲も、取りこぼさない', async () => {
    // 始めた時にはまだなく、聞き始める手続きが済んだ時に届いている
    vi.mocked(commands.takeOpenedFiles)
      .mockResolvedValueOnce(opened())
      .mockResolvedValueOnce(opened({ tracks: [makeTrack('late')] }));

    const stop = watchOpenedFiles();
    await flush();

    expect(commands.takeOpenedFiles).toHaveBeenCalledTimes(2);
    expect(player.currentTrack?.id).toBe('late');
    stop();
  });

  it('開かれたと知らされたら、曲を受け取って再生キューを置き換える', async () => {
    const stop = watchOpenedFiles();
    await flush();
    expect(player.currentTrack).toBeNull();

    vi.mocked(commands.takeOpenedFiles).mockResolvedValueOnce(opened({ tracks: [makeTrack('c')] }));
    for (const listener of listeners) listener();
    await flush();
    expect(player.playQueue.map((track) => track.id)).toEqual(['c']);

    // やめた後は、聞かない
    stop();
    await flush();
    expect(listeners.size).toBe(0);
  });

  it('開けなかったファイルを知らせる（開けた曲は再生する）', async () => {
    vi.mocked(commands.takeOpenedFiles).mockResolvedValueOnce(
      opened({
        tracks: [makeTrack('a')],
        errors: ['x.flac: 読めません', 'y.flac: 読めません']
      })
    );

    const stop = watchOpenedFiles();
    await flush();

    expect(player.currentTrack?.id).toBe('a');
    expect(notifications.items).toHaveLength(1);
    expect(notifications.items[0].message).toContain('x.flac: 読めません');
    expect(notifications.items[0].message).toContain('1');
    stop();
  });

  it('受け取りに失敗したら、エラーとして知らせる', async () => {
    vi.mocked(commands.takeOpenedFiles).mockRejectedValueOnce({ code: 'LOCK', message: '失敗' });

    const stop = watchOpenedFiles();
    await flush();

    expect(player.currentTrack).toBeNull();
    expect(notifications.items).toHaveLength(1);
    stop();
  });
});
