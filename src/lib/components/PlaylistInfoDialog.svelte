<!--
  @component PlaylistInfoDialog
  プレイリスト・オートプレイリストの情報（名前と説明）の編集画面。
  右クリックのメニューと、プレイリスト・オートプレイリストの画面から開く
  （`playlistInfoDialog.open(id)`・`openSmartPlaylist(id)`）。
-->
<script lang="ts">
  import type { Playlist, SmartPlaylist } from '#lib/types/models.js';
  import { useUpdatePlaylistInfoMutation } from '#lib/queries/playlists.js';
  import { useUpdateSmartPlaylistInfoMutation } from '#lib/queries/smartPlaylists.js';
  import {
    MAX_PLAYLIST_DESCRIPTION_LENGTH,
    validatePlaylistName,
    toSafeString
  } from '#lib/utils/validation.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 編集するものの種類（`playlist`はプレイリスト、`smartPlaylist`はオートプレイリスト） */
    kind: 'playlist' | 'smartPlaylist';
    playlist: Playlist | SmartPlaylist;
    onClose: () => void;
  }

  let { kind, playlist, onClose }: Props = $props();

  // 入力は、開いた時点の内容から始める（開いている間に一覧が更新されても変えない）
  // svelte-ignore state_referenced_locally
  const target = playlist;
  let name = $state(target.name);
  let description = $state(target.description ?? '');
  let hasTriedSaving = $state(false);

  const nameError = $derived(validatePlaylistName(name.trim()).error ?? null);
  const descriptionLength = $derived([...description.trim()].length);
  const descriptionError = $derived(
    descriptionLength > MAX_PLAYLIST_DESCRIPTION_LENGTH ? m.validation.descriptionTooLong : null
  );

  const updateMutation = useUpdatePlaylistInfoMutation();
  const updateSmartMutation = useUpdateSmartPlaylistInfoMutation();
  const isSaving = $derived(updateMutation.isPending || updateSmartMutation.isPending);

  async function handleSave() {
    hasTriedSaving = true;
    if (nameError !== null || descriptionError !== null) return;

    const values = { name: toSafeString(name.trim(), 100), description: description.trim() };
    try {
      if (kind === 'playlist') {
        await updateMutation.mutateAsync({ playlist: target as Playlist, ...values });
      } else {
        await updateSmartMutation.mutateAsync({
          smartPlaylist: target as SmartPlaylist,
          ...values
        });
      }
      onClose();
    } catch (error) {
      // 失敗はトーストで通知される。画面は閉じずに、入力を残す
      console.error('Failed to update playlist info:', error);
    }
  }
</script>

<Modal open {onClose} title={m.playlists.infoTitle} dismissible={!isSaving}>
  <form
    id="playlist-info-form"
    onsubmit={(e) => {
      e.preventDefault();
      handleSave();
    }}
  >
    <div class="form-group">
      <label for="playlist-info-name" class="form-label">{m.playlists.name}</label>
      <input
        id="playlist-info-name"
        type="text"
        class="form-input"
        bind:value={name}
        maxlength="100"
        autocomplete="off"
        disabled={isSaving}
        data-autofocus
      />
    </div>
    <div class="form-group mb-0">
      <label for="playlist-info-description" class="form-label">{m.playlists.description}</label>
      <textarea
        id="playlist-info-description"
        class="form-input description"
        bind:value={description}
        rows="5"
        disabled={isSaving}></textarea>
      <p class="count" class:over={descriptionError !== null}>
        {m.playlists.descriptionCount(descriptionLength, MAX_PLAYLIST_DESCRIPTION_LENGTH)}
      </p>
    </div>
  </form>

  {#if hasTriedSaving && nameError}
    <div class="message-error mt-3" role="alert">{nameError}</div>
  {:else if descriptionError}
    <div class="message-error mt-3" role="alert">{descriptionError}</div>
  {/if}

  {#snippet footer()}
    <button type="button" class="btn-secondary" onclick={onClose} disabled={isSaving}>
      {m.common.cancel}
    </button>
    <button type="submit" form="playlist-info-form" class="btn-primary" disabled={isSaving}>
      {isSaving ? m.common.saving : m.common.save}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .description {
    @apply resize-y leading-relaxed;
    min-height: 6rem;
  }

  .count {
    @apply mt-1 mb-0 text-xs text-text-muted text-right;
  }

  .count.over {
    @apply text-error-light;
  }
</style>
