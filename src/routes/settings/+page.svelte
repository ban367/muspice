<script lang="ts">
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { useSaveSettingsMutation, useSettingsQuery } from '#lib/queries/settings.js';
  import type {
    Language,
    Settings,
    StartupPage,
    Theme,
    VolumeNormalization
  } from '#lib/types/models.js';
  import { applyAccentColor, applyTheme } from '#lib/utils/theme.js';
  import { applyLanguage, m } from '#lib/i18n/i18n.svelte.js';
  import LibraryFolderSettings from '#lib/components/LibraryFolderSettings.svelte';

  // 既定のアクセントカラー（Rust側のDEFAULT_ACCENT_COLORと同じ）
  const DEFAULT_ACCENT_COLOR = '#3b82f6';
  // クロスフェードの最大の秒数（Rust側のMAX_CROSSFADE_SECONDSと同じ）
  const MAX_CROSSFADE_SECONDS = 12;

  // ライブラリフォルダの定期的な再スキャンの間隔（分。Rust側のLIBRARY_SCAN_INTERVALSと同じ）
  const scanIntervals = [0, 15, 30, 60, 360];

  type SettingsSection = 'general' | 'playback' | 'library' | 'appearance';

  // 選択肢の値（ラベルは表示の言語に合わせてテンプレートで`m`から読む）
  const sections: SettingsSection[] = ['general', 'playback', 'library', 'appearance'];
  const startupPages: StartupPage[] = ['lastOpened', 'songs'];
  const themes: Theme[] = ['dark', 'light', 'system'];
  const volumeNormalizations: VolumeNormalization[] = ['off', 'track', 'album'];

  // 言語の名前は、その言語で表示する（どの言語を表示中でも選べるように）
  const languages: { value: Language; label: string }[] = [
    { value: 'ja', label: '日本語' },
    { value: 'en', label: 'English' }
  ];

  let activeSection: SettingsSection = $state('general');

  const settingsQuery = useSettingsQuery();
  const saveMutation = useSaveSettingsMutation();

  // 編集中の設定（適用前）。保存済みの設定を読み込んだら初期値にする
  let pending = $state<Settings | null>(null);
  $effect.pre(() => {
    if (settingsQuery.data && pending === null) {
      pending = { ...settingsQuery.data };
    }
  });

  const hasChanges = $derived.by(() => {
    const saved = settingsQuery.data;
    if (pending === null || saved === undefined) return false;
    const current = pending;
    // 項目はすべて値（文字列・数値・真偽値）のため、1つずつ比べる
    return (Object.keys(saved) as (keyof Settings)[]).some((key) => current[key] !== saved[key]);
  });

  // 設定ウィンドウにも保存済みの言語・テーマ・アクセントカラーを反映する
  $effect(() => {
    const language = settingsQuery.data?.language;
    if (language) applyLanguage(language);
  });

  $effect(() => {
    if (settingsQuery.data) {
      applyAccentColor(settingsQuery.data.accentColor);
    }
  });

  $effect(() => {
    const theme = settingsQuery.data?.theme;
    if (theme) return applyTheme(theme);
  });

  async function applySettings() {
    if (!pending) return;
    try {
      await saveMutation.mutateAsync({ ...pending });
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }

  async function cancel() {
    await getCurrentWebviewWindow().close();
  }
</script>

<div class="settings-container">
  <!-- サイドバー -->
  <nav class="settings-sidebar">
    <h2 class="settings-header">{m.settings.title}</h2>
    <ul class="settings-nav">
      {#each sections as section (section)}
        <li>
          <button
            class="settings-nav-item"
            class:active={activeSection === section}
            onclick={() => (activeSection = section)}
          >
            {#if section === 'general'}
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
                />
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                />
              </svg>
            {:else if section === 'playback'}
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M11 5L6 9H2v6h4l5 4V5zM15.54 8.46a5 5 0 010 7.07M19.07 4.93a10 10 0 010 14.14"
                />
              </svg>
            {:else if section === 'library'}
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z"
                />
              </svg>
            {:else}
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01"
                />
              </svg>
            {/if}
            {m.settings.sections[section]}
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <!-- メインコンテンツ -->
  <div class="settings-main">
    <div class="settings-content">
      {#if activeSection === 'library'}
        <!-- ライブラリフォルダの操作はすぐに反映する（下の「適用」の対象外） -->
        <LibraryFolderSettings />

        {#if pending}
          <section class="settings-section">
            <h4 class="subsection-title">{m.settings.autoSync}</h4>

            <div class="setting-item">
              <label class="setting-checkbox-label">
                <input
                  type="checkbox"
                  class="setting-checkbox"
                  bind:checked={pending.watchLibraryFolders}
                />
                {m.settings.watchFolders}
              </label>
              <p class="setting-description">
                {m.settings.watchFoldersHint}
              </p>
            </div>

            <div class="setting-item">
              <label class="setting-label" for="scan-interval">{m.settings.scanInterval}</label>
              <select
                id="scan-interval"
                class="setting-select"
                bind:value={pending.libraryScanIntervalMinutes}
              >
                {#each scanIntervals as minutes (minutes)}
                  <option value={minutes}>{m.settings.scanIntervals[minutes]}</option>
                {/each}
              </select>
              <p class="setting-description">
                {m.settings.scanIntervalHint}
              </p>
            </div>
          </section>
        {/if}
      {:else if !pending}
        <p class="setting-description">
          {settingsQuery.isError ? m.settings.loadFailed : m.common.loading}
        </p>
      {:else if activeSection === 'general'}
        <section class="settings-section">
          <h3 class="section-title">{m.settings.sections.general}</h3>

          <div class="setting-item">
            <label class="setting-label" for="language">{m.settings.language}</label>
            <select id="language" class="setting-select" bind:value={pending.language}>
              {#each languages as option (option.value)}
                <option value={option.value}>{option.label}</option>
              {/each}
            </select>
          </div>

          <div class="setting-item">
            <label class="setting-label" for="startup">{m.settings.startupPage}</label>
            <select id="startup" class="setting-select" bind:value={pending.startupPage}>
              {#each startupPages as page (page)}
                <option value={page}>{m.settings.startupPages[page]}</option>
              {/each}
            </select>
          </div>
        </section>
      {:else if activeSection === 'playback'}
        <section class="settings-section">
          <h3 class="section-title">{m.settings.sections.playback}</h3>
          <div class="setting-item">
            <label class="setting-label" for="normalization">{m.settings.volumeNormalization}</label
            >
            <select
              id="normalization"
              class="setting-select"
              bind:value={pending.volumeNormalization}
            >
              {#each volumeNormalizations as mode (mode)}
                <option value={mode}>{m.settings.volumeNormalizations[mode]}</option>
              {/each}
            </select>
            <p class="setting-description">
              {m.settings.volumeNormalizationHint}
            </p>
          </div>

          <div class="setting-item">
            <label class="setting-checkbox-label">
              <input
                type="checkbox"
                class="setting-checkbox"
                bind:checked={pending.gaplessPlayback}
              />
              {m.settings.gaplessPlayback}
            </label>
            <p class="setting-description">
              {m.settings.gaplessPlaybackHint}
            </p>
          </div>

          <div class="setting-item">
            <label class="setting-label" for="crossfade">{m.settings.crossfade}</label>
            <div class="setting-slider-row">
              <input
                type="range"
                id="crossfade"
                class="setting-slider"
                min="0"
                max={MAX_CROSSFADE_SECONDS}
                step="1"
                bind:value={pending.crossfadeSeconds}
              />
              <span class="setting-slider-value">
                {pending.crossfadeSeconds === 0
                  ? m.common.off
                  : m.settings.crossfadeSeconds(pending.crossfadeSeconds)}
              </span>
            </div>
            <p class="setting-description">
              {m.settings.crossfadeHint}
            </p>
          </div>
        </section>
      {:else}
        <section class="settings-section">
          <h3 class="section-title">{m.settings.sections.appearance}</h3>

          <div class="setting-item">
            <label class="setting-label" for="theme">{m.settings.theme}</label>
            <select id="theme" class="setting-select" bind:value={pending.theme}>
              {#each themes as theme (theme)}
                <option value={theme}>{m.settings.themes[theme]}</option>
              {/each}
            </select>
            <p class="setting-description">
              {m.settings.themeHint}
            </p>
          </div>

          <div class="setting-item">
            <label class="setting-label" for="accent">{m.settings.accentColor}</label>
            <div class="setting-color-row">
              <input
                type="color"
                id="accent"
                class="setting-color"
                bind:value={pending.accentColor}
              />
              <button
                type="button"
                class="setting-reset"
                onclick={() => pending && (pending.accentColor = DEFAULT_ACCENT_COLOR)}
                disabled={pending.accentColor === DEFAULT_ACCENT_COLOR}
              >
                {m.settings.resetToDefault}
              </button>
            </div>
            <p class="setting-description">{m.settings.accentColorHint}</p>
          </div>
        </section>
      {/if}
    </div>

    <!-- フッター -->
    <footer class="settings-footer">
      <button class="btn-secondary" onclick={cancel}>{m.common.cancel}</button>
      <button
        class="btn-primary"
        onclick={applySettings}
        disabled={!hasChanges || saveMutation.isPending}
      >
        {saveMutation.isPending ? m.common.saving : m.common.apply}
      </button>
    </footer>
  </div>
</div>

<style>
  @reference "../../app.css";

  .settings-container {
    @apply flex h-full;
  }

  .settings-sidebar {
    @apply w-48 bg-base-200 border-r border-border flex flex-col;
  }

  .settings-header {
    @apply text-lg font-semibold p-4 m-0;
  }

  .settings-nav {
    @apply list-none m-0 p-0 flex-1;
  }

  .settings-nav-item {
    @apply flex items-center gap-3 w-full px-4 py-2.5 text-left text-sm
           bg-transparent border-none cursor-pointer text-text-secondary
           transition-colors duration-150;
  }

  .settings-nav-item:hover {
    @apply bg-surface-hover text-text-primary;
  }

  .settings-nav-item.active {
    @apply bg-primary/20 text-primary;
  }

  .settings-nav-item .icon {
    @apply w-5 h-5 shrink-0;
  }

  .settings-main {
    @apply flex-1 flex flex-col overflow-hidden;
  }

  .settings-content {
    @apply flex-1 overflow-y-auto p-6;
  }

  .settings-section {
    @apply max-w-xl;
  }

  .section-title {
    @apply text-xl font-semibold mb-6 m-0;
  }

  .subsection-title {
    @apply text-base font-semibold mt-8 mb-4;
  }

  .setting-item {
    @apply mb-6;
  }

  .setting-label {
    @apply block text-sm font-medium text-text-primary mb-2;
  }

  .setting-description {
    @apply text-xs text-text-muted mt-1 m-0;
  }

  .setting-select {
    @apply w-full max-w-xs py-2 px-3 bg-base-400 border border-border rounded-md
           text-sm text-text-primary cursor-pointer;
  }

  .setting-select:focus {
    @apply outline-none border-primary;
  }

  .setting-checkbox-label {
    @apply flex items-center text-sm font-medium text-text-primary cursor-pointer;
  }

  .setting-checkbox {
    @apply w-4 h-4 mr-2 accent-primary cursor-pointer;
  }

  .setting-slider-row {
    @apply flex items-center gap-4;
  }

  .setting-slider {
    @apply flex-1 max-w-xs h-2 accent-primary cursor-pointer;
  }

  .setting-slider-value {
    @apply text-sm text-text-secondary w-12;
  }

  .setting-color-row {
    @apply flex items-center gap-3;
  }

  .setting-color {
    @apply w-12 h-8 p-0 border border-border rounded cursor-pointer;
  }

  .setting-reset {
    @apply text-xs text-text-secondary bg-transparent border-none cursor-pointer underline p-0;
  }

  .setting-reset:disabled {
    @apply opacity-50 cursor-default no-underline;
  }

  .settings-footer {
    @apply flex justify-end gap-3 p-4 border-t border-border bg-base-200;
  }
</style>
