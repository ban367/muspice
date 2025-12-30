<script lang="ts">
  import { page } from '$app/stores';
  import Library from '$lib/components/Library.svelte';

  // ビューモード（全曲/最近再生/よく再生）
  type ViewModeType = 'all' | 'recent' | 'mostplayed';
  const viewMode = $derived.by((): ViewModeType => {
    const view = $page.url.searchParams.get('view');
    if (view === 'recent' || view === 'mostplayed') {
      return view;
    }
    return 'all';
  });
</script>

<div class="h-full overflow-hidden">
  <Library {viewMode} />
</div>
