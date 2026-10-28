import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { QueryClient } from '@tanstack/svelte-query';
import type { Track } from '#lib/types/models.js';

vi.mock('#lib/queries/tracks.js', () => ({ fetchGroupTracks: vi.fn() }));
vi.mock('#lib/stores/player.svelte.js', () => ({ playNow: vi.fn() }));

import { fetchGroupTracks } from '#lib/queries/tracks.js';
import { playNow } from '#lib/stores/player.svelte.js';
import { playGroup } from './groupPlayback.js';

const queryClient = {} as QueryClient;
const tracks = [{ id: 'a' }, { id: 'b' }] as Track[];

beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

describe('playGroup', () => {
  it('アルバムの曲を取得して、再生中の曲の次に入れて再生する', async () => {
    vi.mocked(fetchGroupTracks).mockResolvedValue(tracks);

    await playGroup(queryClient, 'album', 'Blue Horizon', 'Aoi Sora');

    expect(fetchGroupTracks).toHaveBeenCalledWith(queryClient, 'album', 'Blue Horizon', 'Aoi Sora');
    expect(playNow).toHaveBeenCalledWith(tracks);
  });

  it('取得に失敗した場合は、再生しない（失敗で例外を投げない）', async () => {
    vi.mocked(fetchGroupTracks).mockRejectedValueOnce(new Error('取得できない'));
    await expect(playGroup(queryClient, 'artist', 'Aoi Sora')).resolves.toBeUndefined();

    expect(fetchGroupTracks).toHaveBeenLastCalledWith(queryClient, 'artist', 'Aoi Sora', null);
    expect(playNow).not.toHaveBeenCalled();
  });
});
