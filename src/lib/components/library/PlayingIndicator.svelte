<script lang="ts">
  import { player } from '#lib/stores/player.svelte.js';

  // Props
  interface Props {
    size?: 'small' | 'medium' | 'large';
  }

  let { size = 'small' }: Props = $props();
</script>

<!-- app.cssの.playing-indicatorスタイルを使用 -->
<div class="playing-indicator {size}" class:animating={player.isPlaying}>
  <span class="bar"></span>
  <span class="bar"></span>
  <span class="bar"></span>
</div>
