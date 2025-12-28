<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { Track, Metadata } from '$lib/types/models';
  import { useQueryClient } from '@tanstack/svelte-query';
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

  const queryClient = useQueryClient();

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
      await invoke('validate_metadata_command', { metadata });
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
          await invoke('update_track_metadata_with_file', {
            trackId,
            metadata
          });
        } else {
          await invoke('update_track_metadata', {
            trackId,
            metadata
          });
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

        await invoke('update_multiple_tracks_metadata', {
          trackIds,
          metadata: updateMetadata
        });
      }

      // キャッシュを無効化してライブラリを再取得
      await queryClient.invalidateQueries({ queryKey: ['tracks'] });

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
  class="metadata-editor-overlay"
  onclick={handleCancel}
  onkeydown={(e) => e.key === 'Escape' && handleCancel()}
  role="button"
  tabindex="0"
>
  <div
    class="metadata-editor"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div class="editor-header">
      <h3>
        {#if isSingleEdit}
          メタデータを編集
        {:else}
          {tracks.length}件のトラックを一括編集
        {/if}
      </h3>
      <button class="close-button" onclick={handleCancel} aria-label="閉じる"> ✕ </button>
    </div>

    <div class="editor-content">
      {#if isMultipleEdit}
        <div class="info-message">
          空欄のフィールドは変更されません。変更したいフィールドのみ入力してください。
        </div>
      {/if}

      <form onsubmit={(e) => e.preventDefault()}>
        <div class="form-group">
          <label for="title">タイトル</label>
          <input
            id="title"
            type="text"
            bind:value={title}
            placeholder={isMultipleEdit ? '変更しない' : 'タイトルを入力'}
            disabled={isLoading}
          />
        </div>

        <div class="form-group">
          <label for="artist">アーティスト</label>
          <input
            id="artist"
            type="text"
            bind:value={artist}
            placeholder={isMultipleEdit ? '変更しない' : 'アーティストを入力'}
            disabled={isLoading}
          />
        </div>

        <div class="form-group">
          <label for="album">アルバム</label>
          <input
            id="album"
            type="text"
            bind:value={album}
            placeholder={isMultipleEdit ? '変更しない' : 'アルバムを入力'}
            disabled={isLoading}
          />
        </div>

        <div class="form-group">
          <label for="genre">ジャンル</label>
          <input
            id="genre"
            type="text"
            bind:value={genre}
            placeholder={isMultipleEdit ? '変更しない' : 'ジャンルを入力'}
            disabled={isLoading}
          />
        </div>

        <div class="form-group">
          <label for="year">年</label>
          <input
            id="year"
            type="number"
            value={year ?? ''}
            oninput={handleYearInput}
            placeholder={isMultipleEdit ? '変更しない' : '例: 2023'}
            min="1000"
            max="9999"
            disabled={isLoading}
          />
        </div>

        {#if isSingleEdit}
          <div class="form-group checkbox-group">
            <label>
              <input type="checkbox" bind:checked={updateFile} disabled={isLoading} />
              ファイル自体のメタデータも更新する
            </label>
          </div>
        {/if}

        {#if validationError}
          <div class="validation-error">{validationError}</div>
        {/if}

        {#if error}
          <div class="error-message">{error}</div>
        {/if}
      </form>
    </div>

    <div class="editor-footer">
      <button class="cancel-button" onclick={handleCancel} disabled={isLoading}>
        キャンセル
      </button>
      <button class="save-button" onclick={handleSave} disabled={isLoading}>
        {#if isLoading}
          保存中...
        {:else}
          保存
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .metadata-editor-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .metadata-editor {
    background-color: white;
    border-radius: 8px;
    width: 90%;
    max-width: 500px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  }

  .editor-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1.5rem;
    border-bottom: 1px solid #e0e0e0;
  }

  .editor-header h3 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
  }

  .close-button {
    background: none;
    border: none;
    font-size: 1.5rem;
    color: #666;
    cursor: pointer;
    padding: 0;
    width: 2rem;
    height: 2rem;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    transition: background-color 0.2s;
  }

  .close-button:hover {
    background-color: #f0f0f0;
  }

  .editor-content {
    flex: 1;
    overflow-y: auto;
    padding: 1.5rem;
  }

  .info-message {
    background-color: #e3f2fd;
    color: #1976d2;
    padding: 0.75rem;
    border-radius: 4px;
    margin-bottom: 1rem;
    font-size: 0.9rem;
  }

  .form-group {
    margin-bottom: 1rem;
  }

  .form-group label {
    display: block;
    margin-bottom: 0.5rem;
    font-weight: 500;
    color: #333;
  }

  .form-group input[type='text'],
  .form-group input[type='number'] {
    width: 100%;
    padding: 0.75rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    font-size: 1rem;
    transition: border-color 0.2s;
  }

  .form-group input[type='text']:focus,
  .form-group input[type='number']:focus {
    outline: none;
    border-color: #007bff;
  }

  .form-group input:disabled {
    background-color: #f5f5f5;
    cursor: not-allowed;
  }

  .checkbox-group label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .checkbox-group input[type='checkbox'] {
    width: auto;
    cursor: pointer;
  }

  .validation-error,
  .error-message {
    background-color: #ffebee;
    color: #c62828;
    padding: 0.75rem;
    border-radius: 4px;
    margin-top: 1rem;
    font-size: 0.9rem;
  }

  .editor-footer {
    display: flex;
    justify-content: flex-end;
    gap: 1rem;
    padding: 1.5rem;
    border-top: 1px solid #e0e0e0;
  }

  .cancel-button,
  .save-button {
    padding: 0.75rem 1.5rem;
    border: none;
    border-radius: 4px;
    font-size: 1rem;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .cancel-button {
    background-color: #f5f5f5;
    color: #333;
  }

  .cancel-button:hover:not(:disabled) {
    background-color: #e0e0e0;
  }

  .save-button {
    background-color: #007bff;
    color: white;
  }

  .save-button:hover:not(:disabled) {
    background-color: #0056b3;
  }

  .cancel-button:disabled,
  .save-button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  @media (prefers-color-scheme: dark) {
    .metadata-editor {
      background-color: #1e1e1e;
      color: #fff;
    }

    .editor-header {
      border-bottom-color: #444;
    }

    .close-button {
      color: #aaa;
    }

    .close-button:hover {
      background-color: #333;
    }

    .info-message {
      background-color: #1a237e;
      color: #90caf9;
    }

    .form-group label {
      color: #fff;
    }

    .form-group input[type='text'],
    .form-group input[type='number'] {
      background-color: #2a2a2a;
      border-color: #555;
      color: #fff;
    }

    .form-group input:disabled {
      background-color: #333;
    }

    .validation-error,
    .error-message {
      background-color: #5d1f1f;
      color: #ef5350;
    }

    .editor-footer {
      border-top-color: #444;
    }

    .cancel-button {
      background-color: #333;
      color: #fff;
    }

    .cancel-button:hover:not(:disabled) {
      background-color: #444;
    }
  }
</style>
