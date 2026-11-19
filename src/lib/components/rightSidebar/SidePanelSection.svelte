<!--
  @component SidePanelSection
  右サイドバーのパネルの本文の、見出しの付いたまとまり（再生キューの「次に再生」・イコライザ・曲情報の項目など）。
  見出しの右端には、まとまりの操作（`actions`。イコライザのオン・オフなど）を置ける。見出しの高さは、操作の有無で変えない。
  - `variant="label"`（既定）: 小さい見出しで、中身の種類を示す（曲情報の項目・再生キューの一覧など）
  - `variant="group"`: 設定のまとまり（サウンドのパネル）。見出しを本文の文字より大きくし、続けて並べたまとまりの間に
    区切りの線を引く。中身（`children`）のないまとまり（見出しのスイッチだけのもの）も置ける
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** 見出し */
    title: string;
    /** 見出しの右端に置く操作 */
    actions?: Snippet;
    /** 見出しの見た目（`label`: 中身の種類の小さい見出し、`group`: 区切りの付いた設定のまとまり） */
    variant?: 'label' | 'group';
    /**
     * 中身の左右の余白をなくすか（行に自分で余白を持つ一覧。行の文字の位置は、見出しの左の余白（16px）にそろえる）
     */
    flush?: boolean;
    /** 本文の残りの高さを使うか（中身が自分でスクロールする一覧。パネルの`scroll={false}`と組み合わせる） */
    fill?: boolean;
    children?: Snippet;
  }

  let {
    title,
    actions,
    variant = 'label',
    flush = false,
    fill = false,
    children
  }: Props = $props();

  const headingId = $props.id();
</script>

<section class="section" class:group={variant === 'group'} class:fill aria-labelledby={headingId}>
  <div class="section-header">
    <h3 class="section-heading" id={headingId}>{title}</h3>
    {#if actions}
      <div class="section-actions">{@render actions()}</div>
    {/if}
  </div>
  {#if children}
    <div class="section-body" class:flush class:fill>
      {@render children()}
    </div>
  {/if}
</section>

<style>
  @reference "../../../app.css";

  .section.fill {
    @apply flex-1 min-h-0 flex flex-col;
  }

  .section-header {
    @apply flex items-center justify-between gap-2 h-9 px-4 shrink-0;
  }

  /* 名前は、左のサイドバーの見出しの共通クラス（`app.css`の`section-title`。左右の余白がある）と別にする */
  .section-heading {
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

  /* 設定のまとまり: 見出しを、本文の項目の名前（12px）より大きく、濃くする */
  .section.group .section-header {
    @apply h-12;
  }

  .section.group .section-heading {
    @apply text-sm normal-case tracking-normal text-text-primary;
  }

  .section.group .section-body {
    @apply pb-4;
  }

  /* 続けて並べたまとまりの間の区切り（パネルの幅いっぱいに引く。2つ目以降の上に引く） */
  .section.group:not(:first-child) {
    @apply border-t border-border;
  }
</style>
