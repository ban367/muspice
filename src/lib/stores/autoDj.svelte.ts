/**
 * Auto DJ（再生キューの最後の曲になったら、曲を足して再生を続ける）
 *
 * キューの最後の曲を再生している間に曲を足しておくため、次の曲の先読み（ギャップレス再生・
 * クロスフェード）も、足した曲に対して行われる。曲の元は、ライブラリの全曲か、プレイリスト1つ
 * （自動プレイリストなら、足す時点で条件に合う曲）。選び方は`#lib/utils/autoDj`。
 */
import { untrack } from 'svelte';
import type { QueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import { queryKeys } from '#lib/queries/keys.js';
import { CACHE_POLICY } from '#lib/queries/shared.js';
import type { Track } from '#lib/types/models.js';
import { AUTO_DJ_BATCH_SIZE, pickAutoDjTracks } from '#lib/utils/autoDj.js';
import { addToQueue, player } from './player.svelte.js';
import { playbackAids } from './playbackAids.svelte.js';

export interface AutoDjOptions {
  /** Auto DJが有効かを返す（リアクティブな値を読む） */
  enabled: () => boolean;
  /** 曲を選ぶプレイリストのIDを返す（nullはライブラリ全体。リアクティブな値を読む） */
  playlistId: () => string | null;
  /** ライブラリの全曲の一覧のキャッシュ（曲の画面と共有する） */
  queryClient: QueryClient;
}

/**
 * Auto DJを始める
 *
 * 次の条件をすべて満たす間、曲を足す: 有効・再生する曲がある・キューの最後の曲・リピートなし・
 * 「この曲が終わったら停止」でない。
 * @returns 止める関数
 */
export function watchAutoDj(options: AutoDjOptions): () => void {
  let isFilling = false;

  const needsTracks = () =>
    options.enabled() &&
    player.currentTrack !== null &&
    player.repeatMode === 'off' &&
    !playbackAids.stopAfterCurrent &&
    player.currentTrackIndex === player.playQueue.length - 1;

  /** 曲の元の曲を取得する */
  function fetchCandidates(playlistId: string | null): Promise<Track[]> {
    if (playlistId !== null) return commands.getPlaylistTracks(playlistId);
    return options.queryClient.fetchQuery({
      queryKey: queryKeys.tracks.list,
      queryFn: () => commands.getAllTracks(),
      staleTime: CACHE_POLICY.library.staleTime
    });
  }

  async function fill(): Promise<void> {
    if (isFilling) return;
    isFilling = true;
    try {
      const playlistId = options.playlistId();
      const candidates = await fetchCandidates(playlistId);
      // 取得を待つ間に状況が変わっていたら、足さない（必要なら、次の変更でもう一度呼ばれる）
      if (!needsTracks() || options.playlistId() !== playlistId) return;

      const picked = pickAutoDjTracks(candidates, player.playQueue, AUTO_DJ_BATCH_SIZE);
      if (picked.length > 0) addToQueue(picked);
    } catch (error) {
      // 足せなくても、再生は続ける（キューの終わりで止まる）
      console.error('Auto DJの曲の取得に失敗しました:', error);
    } finally {
      isFilling = false;
    }
  }

  // コンポーネントの外でも動かし、戻り値の関数で止めるため$effect.rootで作る
  return $effect.root(() => {
    $effect(() => {
      // 曲の元を変えた時も、足し直せるように読んでおく
      options.playlistId();
      if (needsTracks()) untrack(() => void fill());
    });
  });
}
