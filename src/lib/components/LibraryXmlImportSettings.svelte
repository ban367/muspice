<!--
  @component LibraryXmlImportSettings
  ほかのプレーヤー（MusicBee・iTunes / ミュージック）が書き出したライブラリのXMLから、再生回数・
  追加した日時などと、プレイリストを取り込む操作（設定ウィンドウ用）。
  「XMLを選んで取り込む」を押すと、Rust側がファイルを選ぶダイアログを開く。
-->
<script lang="ts">
  import { useImportLibraryXmlMutation } from '#lib/queries/tracks.js';
  import type { LibraryXmlImportResult } from '#lib/types/models.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const importMutation = useImportLibraryXmlMutation();

  let includePlaylists = $state(true);
  // 直前の結果（対応が付かなかった曲を表示する）
  let lastResult = $state.raw<LibraryXmlImportResult | null>(null);
  // 直前の取り込みで、プレイリストも取り込んだか（結果の表示に使う）
  let lastIncludedPlaylists = $state(false);

  async function handleImport() {
    if (importMutation.isPending) return;
    const withPlaylists = includePlaylists;
    try {
      const result = await importMutation.mutateAsync(withPlaylists);
      // ファイルを選ばなかった場合は、前の結果を残す
      if (result) {
        lastResult = result;
        lastIncludedPlaylists = withPlaylists;
      }
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }
</script>

<section class="settings-section">
  <h4 class="subsection-title">{m.libraryXmlImport.title}</h4>
  <p class="setting-description">{m.libraryXmlImport.description}</p>
  <p class="setting-description">{m.libraryXmlImport.rules}</p>
  <p class="setting-description">{m.libraryXmlImport.musicBeeHint}</p>

  <label class="flex items-center gap-2 mt-3 text-sm text-text-secondary cursor-pointer">
    <input
      type="checkbox"
      bind:checked={includePlaylists}
      disabled={importMutation.isPending}
      class="w-4 h-4"
    />
    {m.libraryXmlImport.includePlaylists}
  </label>

  <button
    class="btn-secondary text-sm mt-3"
    onclick={handleImport}
    disabled={importMutation.isPending}
  >
    {importMutation.isPending ? m.libraryXmlImport.importing : m.libraryXmlImport.choose}
  </button>

  {#if lastResult}
    <div class="result" role="status">
      <p class="m-0 font-semibold text-text-primary break-all">{lastResult.fileName}</p>
      <p class="result-line">
        {m.libraryXmlImport.result(
          lastResult.updatedCount,
          lastResult.matchedCount,
          lastResult.trackCount
        )}
      </p>
      {#if lastIncludedPlaylists}
        <p class="result-line">
          {m.libraryXmlImport.playlists(lastResult.playlistCount, lastResult.skippedPlaylistCount)}
        </p>
      {/if}
      {#if lastResult.unmatchedCount > 0}
        <p class="result-line">{m.libraryXmlImport.unmatched(lastResult.unmatchedCount)}</p>
        <ul class="unmatched-list">
          {#each lastResult.unmatched as location, index (index)}
            <li>{location}</li>
          {/each}
          {#if lastResult.unmatchedCount > lastResult.unmatched.length}
            <li class="text-text-muted">
              {m.libraryXmlImport.unmatchedMore(
                lastResult.unmatchedCount - lastResult.unmatched.length
              )}
            </li>
          {/if}
        </ul>
      {/if}
    </div>
  {/if}
</section>

<style>
  @reference "../../app.css";

  /* 設定画面（settings/+page.svelte）の項目と同じ見た目にする */
  .settings-section {
    @apply max-w-xl;
  }

  .subsection-title {
    @apply text-base font-semibold mt-8 mb-4;
  }

  .setting-description {
    @apply text-xs text-text-muted mt-1 m-0;
  }

  .result {
    @apply mt-4 text-sm;
  }

  .result-line {
    @apply mt-1 mb-0 text-text-secondary;
  }

  /* 対応が付かなかった曲の場所（長い一覧は、この中でスクロールする） */
  .unmatched-list {
    @apply list-none mt-2 mb-0 p-3 max-h-40 overflow-y-auto rounded bg-base-300 text-xs text-text-secondary font-mono break-all;
  }

  .unmatched-list li + li {
    @apply mt-1;
  }
</style>
