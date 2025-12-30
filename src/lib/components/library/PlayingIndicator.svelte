<script lang="ts">
  import { isPlaying } from '$lib/stores/player';

  // Props
  interface Props {
    size?: 'small' | 'medium' | 'large';
  }

  let { size = 'small' }: Props = $props();
</script>

<!-- app.cssの.playing-indicatorスタイルを使用 -->
<div class="playing-indicator {size}" class:animating={$isPlaying}>
  <span class="bar"></span>
  <span class="bar"></span>
  <span class="bar"></span>
</div>
