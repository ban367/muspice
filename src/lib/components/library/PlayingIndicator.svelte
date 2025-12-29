<script lang="ts">
  import { isPlaying } from '$lib/stores/player';

  // Props
  interface Props {
    size?: 'small' | 'medium' | 'large';
  }

  let { size = 'small' }: Props = $props();
</script>

<div class="playing-indicator {size}" class:animating={$isPlaying}>
  <span class="bar"></span>
  <span class="bar"></span>
  <span class="bar"></span>
</div>

<style>
  .playing-indicator {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    justify-content: center;
  }

  .playing-indicator.small {
    height: 1rem;
  }

  .playing-indicator.medium {
    height: 1.25rem;
  }

  .playing-indicator.large {
    height: 2rem;
  }

  .playing-indicator .bar {
    width: 3px;
    background-color: #1db954;
    border-radius: 1px;
  }

  .playing-indicator.large .bar {
    width: 4px;
  }

  .playing-indicator .bar:nth-child(1) { height: 40%; }
  .playing-indicator .bar:nth-child(2) { height: 80%; }
  .playing-indicator .bar:nth-child(3) { height: 60%; }

  /* アニメーションは再生中のみ */
  .playing-indicator.animating .bar {
    animation: equalizer 0.5s ease-in-out infinite alternate;
  }

  .playing-indicator.animating .bar:nth-child(1) { animation-delay: 0s; }
  .playing-indicator.animating .bar:nth-child(2) { animation-delay: 0.2s; }
  .playing-indicator.animating .bar:nth-child(3) { animation-delay: 0.4s; }

  @keyframes equalizer {
    0% { height: 20%; }
    100% { height: 100%; }
  }
</style>
