<script lang="ts">
  import { commands } from '$lib/bindings';
  import type { Track, Metadata } from '$lib/types/models';
  import {
    useUpdateTrackMetadataMutation,
    useUpdateTrackMetadataWithFileMutation,
    useUpdateMultipleTracksMutation
  } from '$lib/queries/tracks';
  import {
    validateYear,
    validateFieldLength,
    combineValidationResults,
    toSafeString
  } from '$lib/utils/validation';

  interface Props {
    tracks: Track[];
    onClose: () => void;
    onSave?: () => void;
  }

  let { tracks, onClose, onSave }: Props = $props();

  // ミューテーション
  let updateMetadataMutation = $derived(useUpdateTrackMetadataMutation());
  let updateMetadataWithFileMutation = $derived(useUpdateTrackMetadataWithFileMutation());
  let updateMultipleTracksMutation = $derived(useUpdateMultipleTracksMutation());

  // 単一トラック編集か複数トラック編集かを判定
  const isSingleEdit = $derived(tracks.length === 1);
  const isMultipleEdit = $derived(tracks.length > 1);

  // フォーム状態
  let title = $state('');
  let artist = $state('');
  let album = $state('');
  let genre = $state('');
  let year = $state<number | null>(null);

  // エラーとローディング状態
  let error = $state<string | null>(null);
  let isLoading = $state(false);
  let validationError = $state<string | null>(null);

  // ファイルも更新するかどうか
  let updateFile = $state(true);

  /**
   * 初期値を設定（単一トラック編集の場合）
   */
  $effect(() => {
    if (isSingleEdit && tracks[0]) {
      const track = tracks[0];
      title = track.title || '';
      artist = track.artist || '';
      album = track.album || '';
      genre = track.genre || '';
      year = track.year;
    }
  });

  /**
   * メタデータをバリデーション
   */
  async function validateMetadata(metadata: Metadata): Promise<boolean> {
    // フロントエンドでのバリデーション
    const validationResults = [
      validateYear(metadata.year),
      validateFieldLength(metadata.title, 'タイトル', 255),
      validateFieldLength(metadata.artist, 'アーティスト', 255),
      validateFieldLength(metadata.album, 'アルバム', 255),
      validateFieldLength(metadata.genre, 'ジャンル', 100)
    ];

    const combined = combineValidationResults(validationResults);
    if (!combined.valid) {
      validationError = combined.errors.join(', ');
      return false;
    }

    // バックエンドでのバリデーション
    try {
      await commands.validateMetadataCommand(metadata);
      validationError = null;
      return true;
    } catch (e) {
      validationError = String(e);
      return false;
    }
  }

  /**
   * フォームを保存
   */
  async function handleSave() {
    error = null;
    validationError = null;
    isLoading = true;

    try {
      // 入力値をサニタイズしてメタデータオブジェクトを作成
      const metadata: Metadata = {
        title: title ? toSafeString(title, 255) : undefined,
        artist: artist ? toSafeString(artist, 255) : undefined,
        album: album ? toSafeString(album, 255) : undefined,
        genre: genre ? toSafeString(genre, 100) : undefined,
        year: year || undefined
      };

      // バリデーション
      const isValid = await validateMetadata(metadata);
      if (!isValid) {
        isLoading = false;
        return;
      }

      if (isSingleEdit) {
        // 単一トラック編集
        const trackId = tracks[0].id;

        if (updateFile) {
          await updateMetadataWithFileMutation.mutateAsync({ trackId, metadata });
        } else {
          await updateMetadataMutation.mutateAsync({ trackId, metadata });
        }
      } else {
        // 複数トラック編集（データベースのみ）
        const trackIds = tracks.map((t) => t.id);

        // 空でないフィールドのみを含むメタデータを作成
        const updateMetadata: Metadata = {};
        if (title) updateMetadata.title = title;
        if (artist) updateMetadata.artist = artist;
        if (album) updateMetadata.album = album;
        if (genre) updateMetadata.genre = genre;
        if (year) updateMetadata.year = year;

        await updateMultipleTracksMutation.mutateAsync({
          trackIds,
          metadata: updateMetadata
        });
      }

      // キャッシュ無効化はミューテーションのonSuccessで自動実行

      // 成功コールバック
      if (onSave) {
        onSave();
      }

      // ダイアログを閉じる
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      isLoading = false;
    }
  }

  /**
   * キャンセル
   */
  function handleCancel() {
    onClose();
  }

  /**
   * 年の入力を処理
   */
  function handleYearInput(event: Event) {
    const target = event.target as HTMLInputElement;
    const value = target.value;

    if (value === '') {
      year = null;
    } else {
      const parsed = parseInt(value, 10);
      if (!isNaN(parsed)) {
        year = parsed;
      }
    }
  }
</script>

<div
  class="custom-modal-backdrop"
  onclick={handleCancel}
  onkeydown={(e) => e.key === 'Escape' && handleCancel()}
  role="button"
  tabindex="0"
>
  <div
    class="modal-content max-w-lg"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div class="flex justify-between items-center p-6 border-b border-border">
      <h3 class="m-0 text-xl font-semibold text-text-primary">
        {#if isSingleEdit}
          メタデータを編集
        {:else}
          {tracks.length}件のトラックを一括編集
        {/if}
      </h3>
      <button class="btn-icon w-8 h-8" onclick={handleCancel} aria-label="閉じる">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-5 h-5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M6 18L18 6M6 6l12 12"
          />
        </svg>
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-6">
      {#if isMultipleEdit}
        <div class="message-info mb-4">
          空欄のフィールドは変更されません。変更したいフィールドのみ入力してください。
        </div>
      {/if}

      <form onsubmit={(e) => e.preventDefault()}>
        <div class="form-group">
          <label for="title" class="form-label">タイトル</label>
          <input
            id="title"
            type="text"
            class="form-input"
            bind:value={title}
            placeholder={isMultipleEdit ? '変更しない' : 'タイトルを入力'}
            disabled={isLoading}
          />
        </div>

        <div class="form-group">
          <label for="artist" class="form-label">アーティスト</label>
          <input
            id="artist"
            type="text"
            class="form-input"
            bind:value={artist}
            placeholder={isMultipleEdit ? '変更しない' : 'アーティストを入力'}
            disabled={isLoading}
          />
        </div>

        <div class="form-group">
          <label for="album" class="form-label">アルバム</label>
          <input
            id="album"
            type="text"
            class="form-input"
            bind:value={album}
            placeholder={isMultipleEdit ? '変更しない' : 'アルバムを入力'}
            disabled={isLoading}
          />
        </div>

        <div class="form-group">
          <label for="genre" class="form-label">ジャンル</label>
          <input
            id="genre"
            type="text"
            class="form-input"
            bind:value={genre}
            placeholder={isMultipleEdit ? '変更しない' : 'ジャンルを入力'}
            disabled={isLoading}
          />
        </div>

        <div class="form-group">
          <label for="year" class="form-label">年</label>
          <input
            id="year"
            type="number"
            class="form-input"
            value={year ?? ''}
            oninput={handleYearInput}
            placeholder={isMultipleEdit ? '変更しない' : '例: 2023'}
            min="1000"
            max="9999"
            disabled={isLoading}
          />
        </div>

        {#if isSingleEdit}
          <div class="form-group">
            <label class="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                class="w-4 h-4"
                bind:checked={updateFile}
                disabled={isLoading}
              />
              <span class="text-text-secondary">ファイル自体のメタデータも更新する</span>
            </label>
          </div>
        {/if}

        {#if validationError}
          <div class="message-error mt-4">{validationError}</div>
        {/if}

        {#if error}
          <div class="message-error mt-4">{error}</div>
        {/if}
      </form>
    </div>

    <div class="flex justify-end gap-3 p-6 border-t border-border">
      <button class="btn-secondary" onclick={handleCancel} disabled={isLoading}>
        キャンセル
      </button>
      <button class="btn-primary" onclick={handleSave} disabled={isLoading}>
        {#if isLoading}
          保存中...
        {:else}
          保存
        {/if}
      </button>
    </div>
  </div>
</div>
