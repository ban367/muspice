<!--
  @component RatingStars
  評価（半分の星の単位。0〜10）の星。星にポインタを置くと、その位置までを塗って選んだ結果を示す
  （3つ目の星の左半分に置くと、星2.5まで、右半分に置くと星3までが塗られる。ADR-082）。
  星を選ぶとその評価にし、今の評価と同じ位置を選ぶと評価を外す。キーボードで押した時は、
  その星までの評価にする。
-->
<script lang="ts">
  import { m } from '#lib/i18n/i18n.svelte.js';
  import { ratingAt, starFill } from '#lib/utils/rating.js';

  interface Props {
    /** 評価（半分の星の単位。0〜10。0は評価なし） */
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

  /** ポインタを置いている位置の評価（その位置までを塗る） */
  let hovered = $state<number | null>(null);

  /** 選んだ評価と、選んだ時点の評価（保存した結果が`rating`に届くまで、選んだ方を表示する） */
  let pending = $state.raw<{ from: number; to: number } | null>(null);

  // 保存した結果が届く（`rating`が選んだ時点の値から変わる）までは、選んだ評価を表示する
  const current = $derived(pending !== null && pending.from === rating ? pending.to : rating);
  const shown = $derived(hovered ?? current);

  // 結果が届いたら、選んだ評価は捨てる（後で`rating`が元の値に戻った時に、古い選択を表示しない）
  $effect(() => {
    if (pending !== null && pending.from !== rating) {
      pending = null;
    }
  });

  /** ポインタの位置から、選ぶ評価を求める（キーボードで押した時は、その星まで） */
  function valueAt(star: number, event: MouseEvent): number {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    if (event.detail === 0 || rect.width === 0) return star * 2;
    return ratingAt(star, (event.clientX - rect.left) / rect.width);
  }

  async function select(star: number, event: MouseEvent) {
    // 行のクリック（選択）として扱わせない
    event.stopPropagation();

    const value = valueAt(star, event);
    const next = current === value ? 0 : value;
    pending = { from: rating, to: next };
    // 選んだ結果を見せるため、ポインタを動かすまでポインタの位置では塗らない
    hovered = null;

    try {
      await onChange(next);
    } catch {
      // 保存できなかった（通知は呼び出し側が行う）ので、元の評価の表示に戻す
      pending = null;
    }
  }

  /** 読み上げる押下の状態（半分だけ塗った星は`mixed`） */
  function pressed(star: number): boolean | 'mixed' {
    const fill = starFill(star, current);
    return fill === 'half' ? 'mixed' : fill === 'full';
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
    {@const fill = starFill(star, shown)}
    <button
      type="button"
      class="star"
      class:filled={fill === 'full'}
      class:half={fill === 'half'}
      onmousemove={(e) => (hovered = valueAt(star, e))}
      onclick={(e) => select(star, e)}
      title={m.library.stars(star)}
      aria-label={m.library.stars(star)}
      aria-pressed={pressed(star)}
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

  /* 半分の星: 左半分だけを塗る（文字の形で背景の色を切り抜く） */
  .star.half {
    @apply bg-linear-to-r from-warning from-50% to-base-400 to-50% bg-clip-text text-transparent;
  }

  .star:hover {
    @apply scale-125;
  }
</style>
