<!--
  @component SidePanelSection
  右サイドバーのパネルの本文の、見出しの付いたまとまり（再生キューの「次に再生」・イコライザ・曲情報の項目など）。
  見出しの右端には、まとまりの操作（`actions`。イコライザのオン・オフなど）を置ける。見出しの高さは、操作の有無で変えない。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** 見出し */
    title: string;
    /** 見出しの右端に置く操作 */
    actions?: Snippet;
    /**
     * 中身の左右の余白をなくすか（行に自分で余白を持つ一覧。行の文字の位置は、見出しの左の余白（16px）にそろえる）
     */
    flush?: boolean;
    /** 本文の残りの高さを使うか（中身が自分でスクロールする一覧。パネルの`scroll={false}`と組み合わせる） */
    fill?: boolean;
    children: Snippet;
  }

  let { title, actions, flush = false, fill = false, children }: Props = $props();

  const headingId = $props.id();
</script>

<section class="section" class:fill aria-labelledby={headingId}>
  <div class="section-header">
    <h3 class="section-title" id={headingId}>{title}</h3>
    {#if actions}
      <div class="section-actions">{@render actions()}</div>
    {/if}
  </div>
  <div class="section-body" class:flush class:fill>
    {@render children()}
  </div>
</section>

<style>
  @reference "../../../app.css";

  .section.fill {
    @apply flex-1 min-h-0 flex flex-col;
  }

  .section-header {
    @apply flex items-center justify-between gap-2 h-9 px-4 shrink-0;
  }

  .section-title {
    @apply m-0 min-w-0 text-[0.625rem] font-semibold uppercase tracking-wide text-text-muted truncate;
  }

  .section-actions {
    @apply flex items-center gap-1 shrink-0;
  }

  .section-body {
    @apply px-4 pb-3;
  }

  .section-body.flush {
    @apply px-0;
  }

  .section-body.fill {
    @apply flex-1 min-h-0 pb-0;
  }
</style>
