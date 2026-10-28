<script lang="ts">
  import { fly } from 'svelte/transition';
  import { ui, type RightSidebarPanel } from '#lib/stores/ui.svelte.js';
  import QueuePanel from './rightSidebar/QueuePanel.svelte';
  import SoundPanel from './rightSidebar/SoundPanel.svelte';
  import AlbumInfoPanel from './rightSidebar/AlbumInfoPanel.svelte';
  import TrackInfoPanel from './rightSidebar/TrackInfoPanel.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // パネルを開く/切り替え
  function openPanel(panel: RightSidebarPanel) {
    if (ui.isRightSidebarExpanded && ui.activeRightSidebarPanel === panel) {
      // 同じパネルをクリックした場合は閉じる
      ui.isRightSidebarExpanded = false;
    } else {
      // パネルを切り替えて開く
      ui.activeRightSidebarPanel = panel;
      ui.isRightSidebarExpanded = true;
    }
  }

  // 閉じる（バックドロップクリック時）
  function closeByBackdrop() {
    // 横に並べている間（固定時）は、バックドロップを出さない
    if (ui.isRightSidebarDocked) return;
    ui.isRightSidebarExpanded = false;
  }

  // ピン状態をトグル
  function togglePin() {
    ui.isRightSidebarPinned = !ui.isRightSidebarPinned;
  }
</script>

<!-- アイコンバー（常に右端に固定表示） -->
<div class="icon-bar">
  <!-- 上部アイコン -->
  <div class="icon-bar-top">
    <!-- 再生キューボタン -->
    <button
      class="icon-button"
      class:active={ui.isRightSidebarExpanded && ui.activeRightSidebarPanel === 'queue'}
      onclick={() => openPanel('queue')}
      title={m.rightSidebar.queueTitle}
      aria-label={m.rightSidebar.openQueue}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M4 6h16M4 10h16M4 14h10M4 18h7" stroke-linecap="round" />
      </svg>
    </button>

    <!-- サウンドボタン（イコライザなど、音の加工の設定） -->
    <button
      class="icon-button"
      class:active={ui.isRightSidebarExpanded && ui.activeRightSidebarPanel === 'sound'}
      onclick={() => openPanel('sound')}
      title={m.rightSidebar.soundTitle}
      aria-label={m.rightSidebar.openSound}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <line x1="4" y1="21" x2="4" y2="14" stroke-linecap="round" />
        <line x1="4" y1="10" x2="4" y2="3" stroke-linecap="round" />
        <line x1="12" y1="21" x2="12" y2="12" stroke-linecap="round" />
        <line x1="12" y1="8" x2="12" y2="3" stroke-linecap="round" />
        <line x1="20" y1="21" x2="20" y2="16" stroke-linecap="round" />
        <line x1="20" y1="12" x2="20" y2="3" stroke-linecap="round" />
        <line x1="1" y1="14" x2="7" y2="14" stroke-linecap="round" />
        <line x1="9" y1="8" x2="15" y2="8" stroke-linecap="round" />
        <line x1="17" y1="16" x2="23" y2="16" stroke-linecap="round" />
      </svg>
    </button>

    <!-- アルバム情報ボタン（再生中の曲と、その曲のアルバム。アルバムの画面と同じディスクの絵） -->
    <button
      class="icon-button"
      class:active={ui.isRightSidebarExpanded && ui.activeRightSidebarPanel === 'album'}
      onclick={() => openPanel('album')}
      title={m.rightSidebar.albumInfoTitle}
      aria-label={m.rightSidebar.openAlbumInfo}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <circle cx="12" cy="12" r="9" />
        <circle cx="12" cy="12" r="2.5" />
      </svg>
    </button>

    <!-- 曲情報ボタン（一覧で選んでいる曲の情報。音符に「i」を添えた絵） -->
    <button
      class="icon-button"
      class:active={ui.isRightSidebarExpanded && ui.activeRightSidebarPanel === 'track'}
      onclick={() => openPanel('track')}
      title={m.rightSidebar.trackInfoTitle}
      aria-label={m.rightSidebar.openTrackInfo}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M8 17V5l8-2" />
        <circle cx="5.5" cy="17" r="2.5" />
        <line x1="19" y1="11" x2="19" y2="11" />
        <line x1="19" y1="15" x2="19" y2="21" />
      </svg>
    </button>
  </div>

  <!-- 下部アイコン（ピンボタン） -->
  <div class="icon-bar-bottom">
    <button
      class="icon-button"
      class:active={ui.isRightSidebarPinned}
      onclick={togglePin}
      title={ui.isRightSidebarPinned ? m.rightSidebar.unpin : m.rightSidebar.pin}
      aria-label={ui.isRightSidebarPinned ? m.rightSidebar.unpin : m.rightSidebar.pin}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill={ui.isRightSidebarPinned ? 'currentColor' : 'none'}
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M12 17v5" stroke-linecap="round" />
        <path d="M5 17h14" stroke-linecap="round" />
        <path
          d="M7 11V7a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v4"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
        <path d="M7 11l-2 6h14l-2-6" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
  </div>
</div>

<!-- バックドロップ（横に並べている間（固定時）は表示しない） -->
{#if ui.isRightSidebarExpanded && !ui.isRightSidebarDocked}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={closeByBackdrop}></div>
{/if}

<!-- パネル（展開時のみ表示、横に並べる時（固定時）はアニメーションなし） -->
{#if ui.isRightSidebarExpanded}
  <aside
    class="side-panel"
    class:pinned={ui.isRightSidebarPinned}
    transition:fly={{ x: 200, duration: ui.isRightSidebarDocked ? 0 : 200 }}
  >
    <!-- 中身は、どのパネルも共通の枠（`rightSidebar/SidePanel`）で作る -->
    {#if ui.activeRightSidebarPanel === 'queue'}
      <QueuePanel />
    {:else if ui.activeRightSidebarPanel === 'sound'}
      <SoundPanel />
    {:else if ui.activeRightSidebarPanel === 'album'}
      <AlbumInfoPanel />
    {:else if ui.activeRightSidebarPanel === 'track'}
      <TrackInfoPanel />
    {/if}
  </aside>
{/if}

<style>
  @reference "../../app.css";

  /* アイコンバー（右端に固定） */
  .icon-bar {
    @apply fixed right-0 top-0 z-40 w-12 h-full
           flex flex-col items-center justify-between py-4
           bg-base-300 border-l border-border;
    /* 下のボタンが再生バーに接しないよう、再生バーの高さに余白を足す */
    padding-bottom: calc(var(--spacing-player-height) + 0.5rem);
  }

  .icon-bar-top {
    @apply flex flex-col items-center gap-2;
  }

  .icon-bar-bottom {
    @apply flex flex-col items-center gap-2;
  }

  .icon-button {
    @apply w-10 h-10 flex items-center justify-center
           bg-transparent border-none rounded-md
           text-text-secondary cursor-pointer
           transition-all duration-200;
  }

  .icon-button:hover {
    @apply bg-surface-active text-text-primary;
  }

  .icon-button.active {
    @apply text-secondary;
  }

  /* バックドロップ（モーダル的に閉じる） */
  .backdrop {
    @apply fixed inset-0 z-20 bg-black/30;
  }

  /* サイドパネル（アイコンバーの左に表示） */
  .side-panel {
    @apply fixed top-0 z-30 h-full
           flex flex-col bg-base-100 border-l border-border;
    right: 3rem; /* アイコンバーの幅 */
    width: var(--spacing-side-panel);
    padding-bottom: var(--spacing-player-height);
  }
</style>
