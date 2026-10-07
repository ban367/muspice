<!--
  @component RatingStars
  評価（0〜5）の星。星にポインタを置くと、その星までを塗って選んだ結果を示す
  （3つ目に置くと、1つ目から3つ目までが塗られる）。
  星を選ぶとその数の評価にし、今の評価と同じ星を選ぶと評価を外す。
-->
<script lang="ts">
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 評価（0〜5。0は評価なし） */
    rating: number;
    /**
     * 評価を選んだ時に呼ぶ（0は評価を外す）
     *
     * Promiseを返すと、保存に失敗（reject）した時に元の評価の表示へ戻す。
     */
    onChange: (rating: number) => void | Promise<unknown>;
  }

  let { rating, onChange }: Props = $props();

  const STARS = [1, 2, 3, 4, 5] as const;

  /** ポインタを置いている星（その星までを塗る） */
  let hovered = $state<number | null>(null);

  /** 選んだ評価と、選んだ時点の評価（保存した結果が`rating`に届くまで、選んだ方を表示する） */
  let pending = $state.raw<{ from: number; to: number } | null>(null);

  // 保存した結果が届く（`rating`が選んだ時点の値から変わる）までは、選んだ評価を表示する
  const current = $derived(pending !== null && pending.from === rating ? pending.to : rating);
  const filledCount = $derived(hovered ?? current);

  // 結果が届いたら、選んだ評価は捨てる（後で`rating`が元の値に戻った時に、古い選択を表示しない）
  $effect(() => {
    if (pending !== null && pending.from !== rating) {
      pending = null;
    }
  });

  async function select(star: number, event: MouseEvent) {
    // 行のクリック（選択）として扱わせない
    event.stopPropagation();

    const next = current === star ? 0 : star;
    pending = { from: rating, to: next };
    // 選んだ結果を見せるため、別の星へ移るまでポインタの位置では塗らない
    hovered = null;

    try {
      await onChange(next);
    } catch {
      // 保存できなかった（通知は呼び出し側が行う）ので、元の評価の表示に戻す
      pending = null;
    }
  }
</script>

<!-- 星を続けて押しても、行のダブルクリック（再生）として扱わせない -->
<div
  class="rating-stars"
  role="group"
  aria-label={m.fields.rating}
  onmouseleave={() => (hovered = null)}
  ondblclick={(e) => e.stopPropagation()}
>
  {#each STARS as star (star)}
    <button
      type="button"
      class="star"
      class:filled={star <= filledCount}
      onmouseenter={() => (hovered = star)}
      onclick={(e) => select(star, e)}
      title={m.library.stars(star)}
      aria-label={m.library.stars(star)}
      aria-pressed={star <= current}
    >
      ★
    </button>
  {/each}
</div>

<style>
  @reference "../../../app.css";

  .rating-stars {
    @apply inline-flex;
  }

  /* 星の間に隙間を作らず、星から星へ移る間にポインタが外れないようにする */
  .star {
    @apply bg-transparent border-none py-0 px-[0.5px] text-sm leading-none text-base-400 cursor-pointer transition-all;
  }

  .star.filled {
    @apply text-warning;
  }

  .star:hover {
    @apply scale-125;
  }
</style>
