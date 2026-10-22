import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { QueryClient } from '@tanstack/svelte-query';
import type { Track } from '#lib/types/models.js';

vi.mock('#lib/queries/tracks.js', () => ({ fetchGroupTracks: vi.fn() }));
vi.mock('#lib/stores/player.svelte.js', () => ({ playTrackFromQueue: vi.fn() }));

import { fetchGroupTracks } from '#lib/queries/tracks.js';
import { playTrackFromQueue } from '#lib/stores/player.svelte.js';
import { playGroup } from './groupPlayback.js';

const queryClient = {} as QueryClient;
const tracks = [{ id: 'a' }, { id: 'b' }] as Track[];

beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

describe('playGroup', () => {
  it('アルバムの曲を取得して、最初の曲から再生する', async () => {
    vi.mocked(fetchGroupTracks).mockResolvedValue(tracks);

    await playGroup(queryClient, 'album', 'Blue Horizon', 'Aoi Sora');

    expect(fetchGroupTracks).toHaveBeenCalledWith(queryClient, 'album', 'Blue Horizon', 'Aoi Sora');
    expect(playTrackFromQueue).toHaveBeenCalledWith(tracks, 0);
  });

  it('曲がない・取得に失敗した場合は、再生しない（失敗で例外を投げない）', async () => {
    vi.mocked(fetchGroupTracks).mockResolvedValueOnce([]);
    await playGroup(queryClient, 'genre', 'Jazz');

    vi.mocked(fetchGroupTracks).mockRejectedValueOnce(new Error('取得できない'));
    await expect(playGroup(queryClient, 'artist', 'Aoi Sora')).resolves.toBeUndefined();

    expect(fetchGroupTracks).toHaveBeenLastCalledWith(queryClient, 'artist', 'Aoi Sora', null);
    expect(playTrackFromQueue).not.toHaveBeenCalled();
  });
});
