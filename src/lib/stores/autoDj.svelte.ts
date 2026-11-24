/**
 * Auto DJ（再生キューの最後の曲になったら、曲を足して再生を続ける）
 *
 * キューの最後の曲を再生している間に曲を足しておくため、次の曲の先読み（ギャップレス再生・
 * クロスフェード）も、足した曲に対して行われる。曲の元は、ライブラリの全曲か、プレイリスト・
 * オートプレイリスト1つ（オートプレイリストなら、足す時点で条件に合う曲）。選び方は`#lib/utils/autoDj`。
 */
import { untrack } from 'svelte';
import type { QueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import { tracksQueryOptions } from '#lib/queries/tracks.js';
import type { Track } from '#lib/types/models.js';
import { AUTO_DJ_BATCH_SIZE, pickAutoDjTracks } from '#lib/utils/autoDj.js';
import { addToQueue, player } from './player.svelte.js';
import { playbackAids } from './playbackAids.svelte.js';

/** Auto DJが曲を選ぶプレイリスト・オートプレイリスト */
export interface AutoDjSource {
  kind: 'playlist' | 'smartPlaylist';
  id: string;
}

/**
 * 設定に保存してある曲の元のIDを、プレイリスト・オートプレイリストのどちらかにする
 *
 * 設定はIDだけを持つ（IDは重ならない。プレイリストから分ける前に選んだオートプレイリストも、
 * そのまま使える。ADR-127）。オートプレイリストの一覧にあればオートプレイリスト、なければプレイリスト。
 * オートプレイリストの一覧をまだ読み込んでいない間は、どちらか決められないためundefinedを返す
 * （その間は曲を足さない。プレイリストとして取り違えて取得しない）。
 * @param smartPlaylistIds オートプレイリストのID（読み込む前はundefined）
 */
export function resolveAutoDjSource(
  id: string | null,
  smartPlaylistIds: readonly string[] | undefined
): AutoDjSource | null | undefined {
  if (id === null) return null;
  if (smartPlaylistIds === undefined) return undefined;
  return { kind: smartPlaylistIds.includes(id) ? 'smartPlaylist' : 'playlist', id };
}

const sameSource = (a: AutoDjSource | null | undefined, b: AutoDjSource | null | undefined) =>
  a === b || (a != null && b != null && a.kind === b.kind && a.id === b.id);

export interface AutoDjOptions {
  /** Auto DJが有効かを返す（リアクティブな値を読む） */
  enabled: () => boolean;
  /**
   * 曲を選ぶプレイリスト・オートプレイリストを返す（nullはライブラリ全体、undefinedはまだ決められない。
   * 決まるまでは曲を足さない。リアクティブな値を読む）
   */
  source: () => AutoDjSource | null | undefined;
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
    options.source() !== undefined &&
    player.currentTrack !== null &&
    player.repeatMode === 'off' &&
    !playbackAids.stopAfterCurrent &&
    player.currentTrackIndex === player.playQueue.length - 1;

  /**
   * 曲の元の曲を取得する
   *
   * プレイリストの曲は、キャッシュを通さずに取得する（オートプレイリストは、足す時点で条件に合う曲を
   * 使う。キャッシュへ入れると、開いているオートプレイリストの画面の曲が、聴いている途中で入れ替わる）。
   */
  function fetchCandidates(source: AutoDjSource | null): Promise<Track[]> {
    if (source?.kind === 'playlist') return commands.getPlaylistTracks(source.id);
    if (source?.kind === 'smartPlaylist') return commands.getSmartPlaylistTracks(source.id);
    return options.queryClient.fetchQuery(tracksQueryOptions());
  }

  async function fill(): Promise<void> {
    const source = options.source();
    if (isFilling || source === undefined) return;
    isFilling = true;
    // 取得を待つ間に曲の元が変わったか（変わった時の呼び出しは、取得中のため無視している）
    let isSourceChanged = false;
    try {
      const candidates = await fetchCandidates(source);
      isSourceChanged = !sameSource(options.source(), source);
      // 取得を待つ間に状況が変わっていたら、足さない（必要なら、次の変更でもう一度呼ばれる）
      if (isSourceChanged || !needsTracks()) return;

      const queue = player.playQueue.map((entry) => entry.track);
      const picked = pickAutoDjTracks(candidates, queue, AUTO_DJ_BATCH_SIZE);
      if (picked.length > 0) addToQueue(picked);
    } catch (error) {
      // 足せなくても、再生は続ける（キューの終わりで止まる）
      console.error('Failed to fetch Auto DJ tracks:', error);
    } finally {
      isFilling = false;
      // 曲の元が変わっていたら、新しい元で足し直す
      if (isSourceChanged && needsTracks()) void fill();
    }
  }

  // コンポーネントの外でも動かし、戻り値の関数で止めるため$effect.rootで作る
  return $effect.root(() => {
    $effect(() => {
      // 曲の元を変えた時も、足し直せるように読んでおく
      options.source();
      if (needsTracks()) untrack(() => void fill());
    });
  });
}
