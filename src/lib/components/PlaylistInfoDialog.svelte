<!--
  @component PlaylistInfoDialog
  プレイリストの情報（名前と説明）の編集画面。
  プレイリストの右クリックのメニューと、プレイリストの画面から開く（`playlistInfoDialog.open(id)`）。
-->
<script lang="ts">
  import type { Playlist } from '#lib/types/models.js';
  import { useUpdatePlaylistInfoMutation } from '#lib/queries/playlists.js';
  import {
    MAX_PLAYLIST_DESCRIPTION_LENGTH,
    validatePlaylistName,
    toSafeString
  } from '#lib/utils/validation.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    playlist: Playlist;
    onClose: () => void;
  }

  let { playlist, onClose }: Props = $props();

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
  const isSaving = $derived(updateMutation.isPending);

  async function handleSave() {
    hasTriedSaving = true;
    if (nameError !== null || descriptionError !== null) return;

    try {
      await updateMutation.mutateAsync({
        playlist: target,
        name: toSafeString(name.trim(), 100),
        description: description.trim()
      });
      onClose();
    } catch (error) {
      // 失敗はトーストで通知される。画面は閉じずに、入力を残す
      console.error('プレイリストの情報の変更に失敗しました:', error);
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
