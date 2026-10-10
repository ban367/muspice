<!--
  プレイリストの画面。曲の一覧は、ほかの曲の一覧と同じ部品（`TrackList`）で表示する。

  - 並び順の既定は、プレイリストの順（手動の並び）。その間だけ、行のドラッグで並べ替えられる
  - 表示する列は、どのプレイリストでも共通。並び順は、プレイリストごとに覚える（ADR-039）
  - 選択した曲は、右クリックのメニュー・Deleteキーでプレイリストから外せる
  - 自動プレイリスト（条件で曲を集めるプレイリスト）は、条件に合う曲を条件の並び順で出す。
    曲の並べ替え・削除はできず、条件を編集する（ADR-042）
-->
<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import {
    usePlaylistsQuery,
    usePlaylistTracksQuery,
    useDeletePlaylistMutation,
    useRemoveTrackFromPlaylistMutation,
    useReorderPlaylistTracksMutation,
    useReshuffleSmartPlaylistsMutation
  } from '#lib/queries/playlists.js';
  import TrackList from '#lib/components/library/TrackList.svelte';
  import PlaylistIcon from '#lib/components/PlaylistIcon.svelte';
  import { smartPlaylistDialog } from '#lib/stores/smartPlaylist.svelte.js';
  import { describeOrder, describeRule } from '#lib/utils/smartPlaylist.js';
  import type { Playlist, Track } from '#lib/types/models.js';
  import { playTrackFromQueue } from '#lib/stores/player.svelte.js';
  import { formatTotalDuration } from '#lib/utils/format.js';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import { playlistSortViewId } from '#lib/stores/trackListView.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // URLからプレイリストIDを取得
  const playlistId = $derived(page.params.id);

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const deletePlaylistMutation = useDeletePlaylistMutation();
  const removeTrackMutation = useRemoveTrackFromPlaylistMutation();
  const reorderTracksMutation = useReorderPlaylistTracksMutation();
  const reshuffleMutation = useReshuffleSmartPlaylistsMutation();

  // 選択されたプレイリスト
  const selectedPlaylist = $derived.by(() => {
    if (!playlistId || !playlistsQuery.data) return null;
    return playlistsQuery.data.find((p: Playlist) => p.id === playlistId) || null;
  });

  // 自動プレイリストの条件（通常のプレイリストではnull）
  const smartRules = $derived(selectedPlaylist?.rules ?? null);
  const isSmart = $derived(smartRules !== null);
  // 条件を読めなかった場合、バックエンドは「いずれかの条件に合う曲」で条件のないものを返す
  const isUnreadable = $derived(
    smartRules !== null && smartRules.matchMode === 'any' && smartRules.rules.length === 0
  );

  // プレイリストの曲の情報（見つかったプレイリストの分だけを取得する）
  const selectedPlaylistId = $derived(selectedPlaylist?.id ?? null);
  const tracksQuery = $derived(
    selectedPlaylistId === null ? null : usePlaylistTracksQuery(selectedPlaylistId, isSmart)
  );
  const trackById = $derived(
    new Map((tracksQuery?.data ?? []).map((track: Track) => [track.id, track]))
  );

  // プレイリストのトラック（プレイリストの順）
  // 並びはプレイリストの一覧から取る（曲の削除を、曲の情報の取り直しを待たずに反映する）。
  // 情報をまだ取得していないトラック・ライブラリにないトラックは除く。
  // 自動プレイリストは、条件から求めた曲と並びをそのまま使う
  const playlistTracks = $derived.by(() => {
    if (!selectedPlaylist) return [];
    if (isSmart) return tracksQuery?.data ?? [];
    return selectedPlaylist.tracks
      .map((pt) => trackById.get(pt.trackId))
      .filter((t): t is Track => t !== undefined);
  });
  const trackCount = $derived(
    isSmart ? playlistTracks.length : (selectedPlaylist?.tracks.length ?? 0)
  );
  // 条件に合う曲がない（取得が済んでいる場合だけ）
  const hasNoMatches = $derived(isSmart && tracksQuery?.data?.length === 0);

  /**
   * プレイリストを削除
   */
  async function handleDeletePlaylist() {
    if (!selectedPlaylist) return;

    if (!(await confirmDestructive(m.playlists.confirmDelete(selectedPlaylist.name)))) {
      return;
    }

    try {
      await deletePlaylistMutation.mutateAsync(selectedPlaylist.id);
      goto(resolve('playlists'));
    } catch (error) {
      console.error('プレイリストの削除に失敗しました:', error);
    }
  }

  /**
   * 選択した曲を、プレイリストから外す
   */
  async function handleRemoveTracks(tracks: Track[]) {
    if (!selectedPlaylist) return;
    const playlistId = selectedPlaylist.id;

    try {
      for (const track of tracks) {
        await removeTrackMutation.mutateAsync({ playlistId, trackId: track.id });
      }
    } catch (error) {
      console.error('トラックの削除に失敗しました:', error);
    }
  }

  /**
   * 行のドラッグでの並べ替え
   * @param trackIds - 一覧に出ている曲の、新しい並び
   */
  async function handleReorder(trackIds: string[]) {
    if (!selectedPlaylist) return;

    // 一覧に出ていない曲（情報をまだ取得していない曲）の位置は変えず、出ている曲だけを入れ替える
    const visible = new Set(trackIds);
    const reordered = trackIds[Symbol.iterator]();
    const newOrder = selectedPlaylist.tracks.map((pt) =>
      visible.has(pt.trackId) ? (reordered.next().value as string) : pt.trackId
    );

    try {
      await reorderTracksMutation.mutateAsync({
        playlistId: selectedPlaylist.id,
        trackIds: newOrder
      });
    } catch (error) {
      console.error('トラックの並び替えに失敗しました:', error);
    }
  }

  /**
   * プレイリストの中の位置を指定して、その曲からプレイリストを再生
   */
  function playFromIndex(index: number) {
    if (index < 0 || index >= playlistTracks.length) return;
    playTrackFromQueue(playlistTracks, index);
  }

  /**
   * プレイリストの合計時間を計算
   */
  const totalDuration = $derived(
    playlistTracks.reduce((total, track) => total + (track.duration || 0), 0)
  );
</script>

<div class="playlist-detail-page">
  {#if playlistsQuery.isLoading}
    <div class="loading">{m.common.loading}</div>
  {:else if !selectedPlaylist}
    <div class="no-selection">
      <h2>{m.playlists.notFound}</h2>
      <p>{m.playlists.notFoundHint}</p>
      <a href={resolve('playlists')} class="back-link">{m.playlists.backToList}</a>
    </div>
  {:else}
    <!-- プレイリスト詳細 -->
    <div class="playlist-header">
      <div class="playlist-info">
        <div class="playlist-icon" class:smart={isSmart}>
          <PlaylistIcon smart={isSmart} class="w-10 h-10 text-white" />
        </div>
        <div class="playlist-details">
          <h1 class="playlist-title">{selectedPlaylist.name}</h1>
          <p class="playlist-meta">
            {m.common.trackCountAndDuration(trackCount, formatTotalDuration(totalDuration))}
          </p>
          {#if smartRules}
            <!-- 自動プレイリストの条件の説明 -->
            <ul class="rule-summary" aria-label={m.smartPlaylist.rules}>
              {#if isUnreadable}
                <li class="rule-chip warning">{m.smartPlaylist.unreadable}</li>
              {:else}
                {#if smartRules.rules.length > 1}
                  <li class="rule-chip mode">{m.smartPlaylist.matchModes[smartRules.matchMode]}</li>
                {/if}
                {#each smartRules.rules as rule, index (index)}
                  <li class="rule-chip">{describeRule(rule)}</li>
                {/each}
                <li class="rule-chip order">{describeOrder(smartRules)}</li>
              {/if}
            </ul>
          {/if}
        </div>
      </div>
      <div class="playlist-actions">
        <button
          class="btn-play-all"
          onclick={() => playFromIndex(0)}
          disabled={playlistTracks.length === 0}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="icon-play"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M8 5v14l11-7z" />
          </svg>
          {m.common.playAll}
        </button>
        {#if smartRules}
          {#if smartRules.order.field === 'random' && !isUnreadable}
            <button
              class="btn-action"
              onclick={() => reshuffleMutation.mutate()}
              disabled={reshuffleMutation.isPending}
              title={m.smartPlaylist.reshuffleTitle}
            >
              {m.smartPlaylist.reshuffle}
            </button>
          {/if}
          <button class="btn-action" onclick={() => smartPlaylistDialog.openEdit(selectedPlaylist)}>
            {m.smartPlaylist.editRules}
          </button>
        {/if}
        <button
          class="btn-delete"
          onclick={handleDeletePlaylist}
          title={m.playlists.deletePlaylist}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="icon-delete"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
            />
          </svg>
        </button>
      </div>
    </div>

    <div class="track-list">
      {#if hasNoMatches}
        <div class="empty-playlist">
          <PlaylistIcon smart class="w-16 h-16 mb-4 opacity-50" />
          <p>{m.smartPlaylist.noMatches}</p>
          <p class="hint">{m.smartPlaylist.noMatchesHint}</p>
        </div>
      {:else if !isSmart && selectedPlaylist.tracks.length === 0}
        <div class="empty-playlist">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="icon-empty"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"
            />
          </svg>
          <p>{m.playlists.noTracks}</p>
          <p class="hint">{m.playlists.noTracksHint}</p>
        </div>
      {:else}
        <!-- トラック一覧（プレイリストを切り替えたら、選択・並び順の設定ごと作り直す） -->
        {#key selectedPlaylist.id}
          <TrackList
            tracks={tracksQuery?.data ? playlistTracks : null}
            isLoading={tracksQuery?.isLoading ?? false}
            isError={tracksQuery?.isError ?? false}
            error={tracksQuery?.error ?? null}
            label={selectedPlaylist.name}
            viewId="playlist"
            sortViewId={playlistSortViewId(selectedPlaylist.id)}
            defaultColumns={['title', 'artist', 'album', 'duration']}
            defaultSort={null}
            onReorder={isSmart ? undefined : handleReorder}
            onRemove={isSmart ? undefined : handleRemoveTracks}
            removeLabel={m.playlists.removeFromPlaylist}
          />
        {/key}
      {/if}
    </div>
  {/if}
</div>

<style>
  @reference "../../../../app.css";
  .playlist-detail-page {
    @apply h-full flex flex-col;
  }

  .no-selection,
  .loading {
    @apply flex-1 flex flex-col items-center justify-center text-text-muted text-center p-8;
  }

  .no-selection h2 {
    @apply m-0 mb-2 text-2xl text-text-secondary;
  }

  .no-selection p {
    @apply m-0 text-text-dimmed;
  }

  .back-link {
    @apply mt-4 text-primary hover:underline;
  }

  /* プレイリストヘッダー */
  .playlist-header {
    @apply flex items-center justify-between gap-4 p-6 rounded-lg mb-4;
    background: linear-gradient(135deg, var(--color-base-200) 0%, var(--color-base-300) 100%);
  }

  .playlist-info {
    @apply flex items-center gap-4 min-w-0;
  }

  .playlist-icon {
    @apply w-20 h-20 shrink-0 flex items-center justify-center rounded-lg;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  }

  /* 自動プレイリストは、色を変えて見分ける */
  .playlist-icon.smart {
    background: linear-gradient(135deg, #f6a13c 0%, #e0567a 100%);
  }

  .playlist-details {
    @apply flex flex-col gap-1 min-w-0;
  }

  /* 自動プレイリストの条件の説明 */
  .rule-summary {
    @apply flex flex-wrap gap-1.5 list-none m-0 mt-1 p-0;
  }

  .rule-chip {
    @apply px-2 py-0.5 rounded-full border border-border bg-base-400 text-xs text-text-secondary;
  }

  .rule-chip.mode,
  .rule-chip.order {
    @apply border-transparent bg-transparent text-text-muted px-0;
  }

  .rule-chip.warning {
    @apply border-warning text-warning;
  }

  .playlist-title {
    @apply m-0 text-2xl font-bold text-text-primary;
  }

  .playlist-meta {
    @apply m-0 text-sm text-text-muted;
  }

  .playlist-actions {
    @apply flex items-center gap-2 shrink-0;
  }

  .btn-action {
    @apply h-10 px-4 bg-transparent border border-border rounded-full text-sm text-text-secondary cursor-pointer transition-colors whitespace-nowrap;
  }

  .btn-action:hover:not(:disabled) {
    @apply bg-surface-hover text-text-primary;
  }

  .btn-action:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .btn-play-all {
    @apply flex items-center gap-2 px-6 py-3 bg-success text-white border-none rounded-full text-sm font-semibold cursor-pointer transition-all;
  }

  .btn-play-all:hover:not(:disabled) {
    @apply bg-success/80 scale-102;
  }

  .btn-play-all:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .icon-play {
    @apply w-5 h-5;
  }

  .btn-delete {
    @apply flex items-center justify-center w-10 h-10 p-0 bg-transparent border border-border rounded-full text-text-muted cursor-pointer transition-all;
  }

  .btn-delete:hover {
    @apply bg-error/10 border-error text-error;
  }

  .icon-delete {
    @apply w-5 h-5;
  }

  /* トラックリスト（スクロールは一覧の中で行う） */
  .track-list {
    @apply flex-1 min-h-0;
  }

  /* 空のプレイリスト */
  .empty-playlist {
    @apply flex flex-col items-center justify-center py-16 text-center text-text-muted;
  }

  .icon-empty {
    @apply w-16 h-16 mb-4 opacity-50;
  }

  .empty-playlist p {
    @apply m-1;
  }

  .empty-playlist .hint {
    @apply text-sm text-text-dimmed;
  }
</style>
