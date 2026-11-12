<!--
  @component MusicFolderSetup
  ライブラリが空の画面の、音楽フォルダ（新しい曲を置く宛先）の選択。

  既定のフォルダ（OSの音楽フォルダの中の`Muspice`。なければ作る。ADR-083）か、音楽を置いている
  フォルダ（Rust側のダイアログ）を選ぶ。中の曲の読み込みは、
  インポートダイアログに表示する（`importer.setMusicFolder`）。音楽フォルダを設定してあれば、
  そのフォルダと、曲を足す方法を出す。
-->
<script lang="ts">
  import { useMusicFolderQuery } from '#lib/queries/musicFolder.js';
  import { importer } from '#lib/stores/importer.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const musicFolderQuery = useMusicFolderQuery();
  const info = $derived(musicFolderQuery.data);
</script>

{#if info?.folder}
  <div class="music-folder-setup">
    <p class="setup-title">{m.musicFolder.emptyFolder(info.folder.path)}</p>
    <p class="setup-hint">{m.musicFolder.emptyFolderHint}</p>
  </div>
{:else if info}
  <div class="music-folder-setup">
    <p class="setup-title">{m.musicFolder.setupTitle}</p>
    <p class="setup-hint">{m.musicFolder.setupHint}</p>
    <div class="setup-actions">
      {#if info.defaultFolder}
        <button
          type="button"
          class="btn-primary"
          onclick={() => importer.setMusicFolder('default')}
          disabled={importer.isImporting}
          title={info.defaultFolder}
        >
          {m.musicFolder.useDefault}
        </button>
      {/if}
      <button
        type="button"
        class={info.defaultFolder ? 'btn-secondary' : 'btn-primary'}
        onclick={() => importer.setMusicFolder('pick')}
        disabled={importer.isImporting}
      >
        {m.musicFolder.pick}
      </button>
    </div>
    {#if info.defaultFolder}
      <p class="default-folder">{info.defaultFolder}</p>
    {/if}
  </div>
{/if}

<style>
  @reference "../../../app.css";

  .music-folder-setup {
    @apply mt-8 w-full max-w-md p-5 rounded-lg border border-border bg-base-200 text-left;
  }

  .setup-title {
    @apply m-0 text-sm font-medium text-text-primary;
  }

  .setup-hint {
    @apply mt-1 mb-0 text-xs text-text-muted;
  }

  .setup-actions {
    @apply flex flex-wrap gap-2 mt-4;
  }

  /* 既定のフォルダの場所（最初の選択肢で作る・使うフォルダ） */
  .default-folder {
    @apply mt-2 mb-0 text-xs text-text-muted font-mono truncate;
  }
</style>
