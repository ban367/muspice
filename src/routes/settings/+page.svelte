<script lang="ts">
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

  // 設定セクションの型
  type SettingsSection = 'general' | 'appearance' | 'playback' | 'library';

  // 現在選択されているセクション
  let activeSection: SettingsSection = $state('general');

  // ダミー設定の型
  interface Settings {
    general: {
      language: 'ja' | 'en';
      startupBehavior: 'last' | 'home';
    };
    appearance: {
      theme: 'dark' | 'light' | 'system';
      accentColor: string;
    };
    playback: {
      gapless: boolean;
      crossfade: number;
      normalize: boolean;
    };
    library: {
      autoScanInterval: number;
      watchFolders: string[];
    };
  }

  // デフォルト設定
  function getDefaultSettings(): Settings {
    return {
      general: {
        language: 'ja',
        startupBehavior: 'last'
      },
      appearance: {
        theme: 'dark',
        accentColor: '#3b82f6'
      },
      playback: {
        gapless: true,
        crossfade: 0,
        normalize: false
      },
      library: {
        autoScanInterval: 30,
        watchFolders: []
      }
    };
  }

  // 現在保存されている設定（初期値）
  let savedSettings: Settings = $state(getDefaultSettings());

  // 編集中の設定（適用前の一時状態）
  let pendingSettings: Settings = $state(getDefaultSettings());

  // 変更があるかどうか
  const hasChanges = $derived(JSON.stringify(savedSettings) !== JSON.stringify(pendingSettings));

  // セクション定義
  const sections = [
    { id: 'general' as const, label: '一般', icon: 'settings' },
    { id: 'appearance' as const, label: '外観', icon: 'palette' },
    { id: 'playback' as const, label: '再生', icon: 'play' },
    { id: 'library' as const, label: 'ライブラリ', icon: 'library' }
  ];

  // 適用ボタン
  async function applySettings() {
    savedSettings = structuredClone(pendingSettings);
    // TODO: 設定を永続化（Tauri invoke）
    // await invoke('save_settings', { settings: savedSettings });
    console.log('設定を適用しました:', savedSettings);
  }

  // キャンセルボタン
  async function cancel() {
    const window = getCurrentWebviewWindow();
    await window.close();
  }
</script>

<div class="settings-container">
  <!-- サイドバー -->
  <nav class="settings-sidebar">
    <h2 class="settings-header">設定</h2>
    <ul class="settings-nav">
      {#each sections as section (section.id)}
        <li>
          <button
            class="settings-nav-item"
            class:active={activeSection === section.id}
            onclick={() => (activeSection = section.id)}
          >
            {#if section.icon === 'settings'}
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
            {:else if section.icon === 'palette'}
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
            {:else if section.icon === 'play'}
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
                  d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"
                />
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
            {:else if section.icon === 'library'}
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
                  d="M8 14v3m4-3v3m4-3v3M3 21h18M3 10h18M3 7l9-4 9 4M4 10h16v11H4V10z"
                />
              </svg>
            {/if}
            <span>{section.label}</span>
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <!-- メインコンテンツ -->
  <div class="settings-main">
    <div class="settings-content">
      {#if activeSection === 'general'}
        <section class="settings-section">
          <h3 class="section-title">一般</h3>

          <div class="setting-item">
            <label class="setting-label" for="language">言語</label>
            <select
              id="language"
              class="setting-select"
              bind:value={pendingSettings.general.language}
            >
              <option value="ja">日本語</option>
              <option value="en">English</option>
            </select>
          </div>

          <div class="setting-item">
            <label class="setting-label" for="startup">起動時の動作</label>
            <select
              id="startup"
              class="setting-select"
              bind:value={pendingSettings.general.startupBehavior}
            >
              <option value="last">前回の状態を復元</option>
              <option value="home">ホームを表示</option>
            </select>
          </div>
        </section>
      {:else if activeSection === 'appearance'}
        <section class="settings-section">
          <h3 class="section-title">外観</h3>

          <div class="setting-item">
            <label class="setting-label" for="theme">テーマ</label>
            <select id="theme" class="setting-select" bind:value={pendingSettings.appearance.theme}>
              <option value="dark">ダーク</option>
              <option value="light">ライト</option>
              <option value="system">システム設定に従う</option>
            </select>
          </div>

          <div class="setting-item">
            <label class="setting-label" for="accent">アクセントカラー</label>
            <input
              type="color"
              id="accent"
              class="setting-color"
              bind:value={pendingSettings.appearance.accentColor}
            />
          </div>
        </section>
      {:else if activeSection === 'playback'}
        <section class="settings-section">
          <h3 class="section-title">再生</h3>

          <div class="setting-item">
            <label class="setting-label">
              <input
                type="checkbox"
                class="setting-checkbox"
                bind:checked={pendingSettings.playback.gapless}
              />
              ギャップレス再生
            </label>
            <p class="setting-description">曲間の無音を除去して連続再生します</p>
          </div>

          <div class="setting-item">
            <label class="setting-label" for="crossfade">クロスフェード</label>
            <div class="setting-slider-container">
              <input
                type="range"
                id="crossfade"
                class="setting-slider"
                min="0"
                max="12"
                step="1"
                bind:value={pendingSettings.playback.crossfade}
              />
              <span class="setting-slider-value">{pendingSettings.playback.crossfade}秒</span>
            </div>
          </div>

          <div class="setting-item">
            <label class="setting-label">
              <input
                type="checkbox"
                class="setting-checkbox"
                bind:checked={pendingSettings.playback.normalize}
              />
              音量正規化（ReplayGain）
            </label>
            <p class="setting-description">曲ごとの音量差を軽減します</p>
          </div>
        </section>
      {:else if activeSection === 'library'}
        <section class="settings-section">
          <h3 class="section-title">ライブラリ</h3>

          <div class="setting-item">
            <label class="setting-label" for="autoscan">自動スキャン間隔</label>
            <select
              id="autoscan"
              class="setting-select"
              bind:value={pendingSettings.library.autoScanInterval}
            >
              <option value={0}>無効</option>
              <option value={15}>15分</option>
              <option value={30}>30分</option>
              <option value={60}>1時間</option>
              <option value={360}>6時間</option>
            </select>
          </div>

          <div class="setting-item">
            <span class="setting-label">監視フォルダ</span>
            <p class="setting-description">
              現在の監視フォルダはありません。<br />
              フォルダを追加するには「ファイル」メニューから「フォルダをインポート」を選択してください。
            </p>
          </div>
        </section>
      {/if}
    </div>

    <!-- フッター -->
    <footer class="settings-footer">
      <button class="btn-secondary" onclick={cancel}>キャンセル</button>
      <button class="btn-primary" onclick={applySettings} disabled={!hasChanges}> 適用 </button>
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

  .setting-checkbox {
    @apply w-4 h-4 mr-2 accent-primary;
  }

  .setting-color {
    @apply w-12 h-8 p-0 border border-border rounded cursor-pointer;
  }

  .setting-slider-container {
    @apply flex items-center gap-4;
  }

  .setting-slider {
    @apply flex-1 max-w-xs h-2 accent-primary cursor-pointer;
  }

  .setting-slider-value {
    @apply text-sm text-text-secondary w-12;
  }

  .settings-footer {
    @apply flex justify-end gap-3 p-4 border-t border-border bg-base-200;
  }
</style>
