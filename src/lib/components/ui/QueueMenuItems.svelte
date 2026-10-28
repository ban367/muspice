<!--
  @component QueueMenuItems
  曲を再生キューへ入れる項目（次のキューに追加して再生・次のキューに追加・キューの末尾に追加・
  キューを置き換えて再生）。曲・アルバムなど・プレイリストのメニューで共通に使う。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import {
    addNextInQueue,
    addToQueue,
    playNow,
    replaceQueueAndPlay
  } from '#lib/stores/player.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import MenuItem from './MenuItem.svelte';

  interface Props {
    /** キューへ入れる曲（この順に入れ、最初の曲から再生する） */
    tracks: readonly Track[];
    /** 曲を取得している間など、まだ選べない */
    disabled?: boolean;
    onClose: () => void;
  }

  let { tracks, disabled = false, onClose }: Props = $props();

  function run(action: (tracks: readonly Track[]) => void) {
    action(tracks);
    onClose();
  }
</script>

<MenuItem onSelect={() => run(playNow)} {disabled}>
  {#snippet icon()}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d="M8 5v14l11-7z" />
    </svg>
  {/snippet}
  {m.common.playNow}
</MenuItem>

<MenuItem onSelect={() => run(addNextInQueue)} {disabled}>
  {#snippet icon()}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M13 5l7 7-7 7M5 5l7 7-7 7"
      />
    </svg>
  {/snippet}
  {m.common.playNext}
</MenuItem>

<MenuItem onSelect={() => run(addToQueue)} {disabled}>
  {#snippet icon()}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M4 6h16M4 10h16M4 14h16M4 18h7"
      />
    </svg>
  {/snippet}
  {m.common.addToQueue}
</MenuItem>

<MenuItem onSelect={() => run(replaceQueueAndPlay)} {disabled}>
  {#snippet icon()}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"
      />
    </svg>
  {/snippet}
  {m.common.replaceQueue}
</MenuItem>
