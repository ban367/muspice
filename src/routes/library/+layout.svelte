<script lang="ts">
  import { page } from '$app/stores';

  let { children } = $props();

  // 現在のパスからアクティブなタブを判定
  const activeTab = $derived.by(() => {
    const path = $page.url.pathname;
    if (path.startsWith('/library/albums')) return 'albums';
    if (path.startsWith('/library/artists')) return 'artists';
    if (path.startsWith('/library/genres')) return 'genres';
    if (path.startsWith('/library/recent')) return 'recent';
    if (path.startsWith('/library/mostplayed')) return 'mostplayed';
    return 'songs';
  });

  // ナビゲーションタブ
  const browseTabs = [
    { id: 'songs', label: '曲', href: '/library/songs', icon: 'music' },
    { id: 'albums', label: 'アルバム', href: '/library/albums', icon: 'album' },
    { id: 'artists', label: 'アーティスト', href: '/library/artists', icon: 'artist' },
    { id: 'genres', label: 'ジャンル', href: '/library/genres', icon: 'genre' }
  ] as const;
</script>

<div class="library-layout">
  <!-- ブラウズタブ -->
  <nav class="browse-tabs">
    {#each browseTabs as tab (tab.id)}
      <a
        href={tab.href}
        class="browse-tab"
        class:active={activeTab === tab.id}
      >
        {#if tab.icon === 'music'}
          <svg xmlns="http://www.w3.org/2000/svg" class="tab-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path d="M9 18V5l12-2v13" />
            <circle cx="6" cy="18" r="3" />
            <circle cx="18" cy="16" r="3" />
          </svg>
        {:else if tab.icon === 'album'}
          <svg xmlns="http://www.w3.org/2000/svg" class="tab-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        {:else if tab.icon === 'artist'}
          <svg xmlns="http://www.w3.org/2000/svg" class="tab-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
            <circle cx="12" cy="7" r="4" />
          </svg>
        {:else if tab.icon === 'genre'}
          <svg xmlns="http://www.w3.org/2000/svg" class="tab-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
            <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
          </svg>
        {/if}
        <span>{tab.label}</span>
      </a>
    {/each}
  </nav>

  <!-- ページコンテンツ -->
  <div class="library-content">
    {@render children()}
  </div>
</div>

<style>
@reference "../../app.css";
  .library-layout {
    @apply flex flex-col h-full;
  }

  .browse-tabs {
    @apply flex gap-1 p-2 bg-base-200 border-b border-border;
  }

  .browse-tab {
    @apply flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium text-text-secondary transition-all duration-200;
  }

  .browse-tab:hover {
    @apply bg-surface-hover text-text-primary;
  }

  .browse-tab.active {
    @apply bg-primary/20 text-primary;
  }

  .tab-icon {
    @apply w-4 h-4;
  }

  .library-content {
    @apply flex-1 overflow-hidden;
  }
</style>
