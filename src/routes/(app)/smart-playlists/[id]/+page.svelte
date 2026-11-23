<!--
  オートプレイリスト（条件で曲を集めるプレイリスト）の画面（ADR-042・ADR-127）。曲の一覧は、ほかの
  曲の一覧と同じ部品（`TrackList`）で表示する。

  - 条件に合う曲を、条件の並び順で出す。曲の並べ替え・削除はできず、条件を編集する
  - 列を押した並べ替えは、表示だけのもの（並び順はオートプレイリストごとに覚える。ADR-039）
  - 開いている間は、曲を取り直さない（評価を下げた曲が、その場で消えない）。条件を変えた時・
    「選び直す」を押した時は、すぐに取り直す
  - 「プレイリストとして保存」で、今の曲を入れた通常のプレイリストを作れる
-->
<script lang="ts">
  import { mapById } from '#lib/utils/collections.js';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { usePlaylistsQuery } from '#lib/queries/playlists.js';
  import {
    useDeleteSmartPlaylistMutation,
    useReshuffleSmartPlaylistsMutation,
    useSaveSmartPlaylistAsPlaylistMutation,
    useSmartPlaylistTracksQuery,
    useSmartPlaylistsQuery
  } from '#lib/queries/smartPlaylists.js';
  import TrackList from '#lib/components/library/TrackList.svelte';
  import PlaylistIcon from '#lib/components/PlaylistIcon.svelte';
  import {
    promptSaveAsPlaylistName,
    smartPlaylistDialog
  } from '#lib/stores/smartPlaylist.svelte.js';
  import { playlistInfoDialog } from '#lib/stores/playlistSidebar.svelte.js';
  import { describeOrder, describeRule, type RulePlaylistNames } from '#lib/utils/smartPlaylist.js';
  import type { Playlist, SmartPlaylist } from '#lib/types/models.js';
  import { playNow } from '#lib/stores/player.svelte.js';
  import { formatTotalDuration } from '#lib/utils/format.js';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import { smartPlaylistSortViewId } from '#lib/stores/trackListView.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // URLからオートプレイリストのIDを取得
  const smartPlaylistId = $derived(page.params.id);

  const smartPlaylistsQuery = useSmartPlaylistsQuery();
  const playlistsQuery = usePlaylistsQuery();
  const deleteMutation = useDeleteSmartPlaylistMutation();
  const reshuffleMutation = useReshuffleSmartPlaylistsMutation();
  const saveAsPlaylistMutation = useSaveSmartPlaylistAsPlaylistMutation();

  const smartPlaylist = $derived(
    smartPlaylistsQuery.data?.find((item) => item.id === smartPlaylistId) ?? null
  );
  const rules = $derived(smartPlaylist?.rules ?? null);
  // 条件を読めなかった場合、バックエンドは「いずれかの条件に合う曲」で条件のないものを返す
  const isUnreadable = $derived(
    rules !== null && rules.matchMode === 'any' && rules.rules.length === 0
  );

  /** 条件にしたプレイリスト・オートプレイリストの名前（条件の説明に出す。消したものはnull） */
  const playlistById = $derived(mapById<Playlist>(playlistsQuery.data ?? []));
  const smartPlaylistById = $derived(mapById<SmartPlaylist>(smartPlaylistsQuery.data ?? []));
  const names: RulePlaylistNames = {
    playlist: (id) => playlistById.get(id)?.name ?? null,
    smartPlaylist: (id) => smartPlaylistById.get(id)?.name ?? null
  };

  // 条件に合う曲（見つかったオートプレイリストの分だけを求める）
  const foundId = $derived(smartPlaylist?.id ?? null);
  const tracksQuery = $derived(foundId === null ? null : useSmartPlaylistTracksQuery(foundId));
  const tracks = $derived(tracksQuery?.data ?? []);
  // 条件に合う曲がない（取得が済んでいる場合だけ）
  const hasNoMatches = $derived(tracksQuery?.data?.length === 0);
  const totalDuration = $derived(tracks.reduce((total, track) => total + (track.duration || 0), 0));

  async function handleDelete() {
    if (!smartPlaylist) return;
    if (!(await confirmDestructive(m.smartPlaylist.confirmDelete(smartPlaylist.name)))) return;

    try {
      await deleteMutation.mutateAsync(smartPlaylist.id);
      goto(resolve('playlists'));
    } catch (error) {
      console.error('Failed to delete smart playlist:', error);
    }
  }

  async function handleSaveAsPlaylist() {
    if (!smartPlaylist) return;
    const id = smartPlaylist.id;
    const name = await promptSaveAsPlaylistName(smartPlaylist.name);
    if (name !== null) saveAsPlaylistMutation.mutate({ smartPlaylistId: id, name });
  }
</script>

<div class="playlist-detail-page">
  {#if smartPlaylistsQuery.isLoading}
    <div class="loading">{m.common.loading}</div>
  {:else if !smartPlaylist || !rules}
    <div class="no-selection">
      <h2>{m.smartPlaylist.notFound}</h2>
      <p>{m.smartPlaylist.notFoundHint}</p>
      <a href={resolve('playlists')} class="back-link">{m.playlists.backToList}</a>
    </div>
  {:else}
    <div class="playlist-header">
      <div class="playlist-info">
        <div class="playlist-icon">
          <PlaylistIcon smart class="w-10 h-10 text-white" />
        </div>
        <div class="playlist-details">
          <h1 class="playlist-title">{smartPlaylist.name}</h1>
          <p class="playlist-meta">
            {m.common.trackCountAndDuration(tracks.length, formatTotalDuration(totalDuration))}
          </p>
          {#if smartPlaylist.description}
            <p class="playlist-description">{smartPlaylist.description}</p>
          {/if}
          <!-- 条件の説明 -->
          <ul class="rule-summary" aria-label={m.smartPlaylist.rules}>
            {#if isUnreadable}
              <li class="rule-chip warning">{m.smartPlaylist.unreadable}</li>
            {:else}
              {#if rules.rules.length > 1}
                <li class="rule-chip mode">{m.smartPlaylist.matchModes[rules.matchMode]}</li>
              {/if}
              {#each rules.rules as rule, index (index)}
                <li class="rule-chip">{describeRule(rule, names)}</li>
              {/each}
              <li class="rule-chip order">{describeOrder(rules)}</li>
            {/if}
          </ul>
        </div>
      </div>
      <div class="playlist-actions">
        <button class="btn-play-all" onclick={() => playNow(tracks)} disabled={tracks.length === 0}>
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
        {#if rules.order.field === 'random' && !isUnreadable}
          <button
            class="btn-action"
            onclick={() => reshuffleMutation.mutate()}
            disabled={reshuffleMutation.isPending}
            title={m.smartPlaylist.reshuffleTitle}
          >
            {m.smartPlaylist.reshuffle}
          </button>
        {/if}
        <button class="btn-action" onclick={() => smartPlaylistDialog.openEdit(smartPlaylist)}>
          {m.smartPlaylist.editRules}
        </button>
        <button
          class="btn-round"
          onclick={handleSaveAsPlaylist}
          disabled={saveAsPlaylistMutation.isPending}
          title={m.smartPlaylist.saveAsPlaylistTitle}
          aria-label={m.smartPlaylist.saveAsPlaylistTitle}
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
              d="M4 6h16M4 10h16M4 14h10m4 0v6m-3-3h6"
            />
          </svg>
        </button>
        <button
          class="btn-round"
          onclick={() => playlistInfoDialog.openSmartPlaylist(smartPlaylist.id)}
          title={m.playlists.editInfo}
          aria-label={m.playlists.editInfo}
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
              d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
            />
          </svg>
        </button>
        <button
          class="btn-round btn-delete"
          onclick={handleDelete}
          title={m.smartPlaylist.deleteTitle}
          aria-label={m.smartPlaylist.deleteTitle}
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
      {:else}
        <!-- トラック一覧（オートプレイリストを切り替えたら、選択・並び順の設定ごと作り直す） -->
        {#key smartPlaylist.id}
          <TrackList
            tracks={tracksQuery?.data ?? null}
            isLoading={tracksQuery?.isLoading ?? false}
            isError={tracksQuery?.isError ?? false}
            error={tracksQuery?.error ?? null}
            label={smartPlaylist.name}
            viewId="playlist"
            sortViewId={smartPlaylistSortViewId(smartPlaylist.id)}
            defaultColumns={['title', 'artist', 'album', 'duration']}
            defaultSort={null}
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

  /* オートプレイリストは、プレイリストと色を変えて見分ける */
  .playlist-icon {
    @apply w-20 h-20 shrink-0 flex items-center justify-center rounded-lg;
    background: linear-gradient(135deg, #f6a13c 0%, #e0567a 100%);
  }

  .playlist-details {
    @apply flex flex-col gap-1 min-w-0;
  }

  /* 条件の説明 */
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

  /* 説明（長い場合は、3行までにする） */
  .playlist-description {
    @apply m-0 text-sm text-text-secondary whitespace-pre-line break-words;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    overflow: hidden;
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

  .btn-round {
    @apply flex items-center justify-center w-10 h-10 p-0 bg-transparent border border-border rounded-full text-text-muted cursor-pointer transition-all;
  }

  .btn-round:hover {
    @apply bg-surface-hover text-text-primary;
  }

  .btn-round.btn-delete:hover {
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

  .empty-playlist p {
    @apply m-1;
  }

  .empty-playlist .hint {
    @apply text-sm text-text-dimmed;
  }
</style>
