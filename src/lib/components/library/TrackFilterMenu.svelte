<!--
  @component TrackFilterMenu
  曲の一覧のフィルタ（評価・年・お気に入り）。ボタンを押すと、条件の入力欄を出す。
  条件を変えるたびに、`onChange`で新しい条件を渡す（一覧は、入力のたびに絞り込まれる）。
-->
<script lang="ts">
  import { Popover } from '#lib/components/ui/index.js';
  import { EMPTY_FILTERS, countActiveFilters, type TrackFilters } from '#lib/utils/trackFilter.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    filters: TrackFilters;
    onChange: (filters: TrackFilters) => void;
  }

  let { filters, onChange }: Props = $props();

  const activeCount = $derived(countActiveFilters(filters));

  const ratings = [1, 2, 3, 4, 5];

  /** 年の入力欄の値を読む（空・4桁までの数字でなければ、指定なし） */
  function parseYear(value: string): number | null {
    const trimmed = value.trim();
    return /^\d{1,4}$/.test(trimmed) ? Number(trimmed) : null;
  }
</script>

<Popover label={m.library.filters.button} class="w-64">
  {#snippet trigger(props)}
    <button
      {...props}
      type="button"
      class="filter-button"
      class:active={activeCount > 0}
      title={m.library.filters.button}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="w-4 h-4"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z"
        />
      </svg>
      <span>{m.library.filters.button}</span>
      {#if activeCount > 0}
        <span class="badge">{activeCount}</span>
      {/if}
    </button>
  {/snippet}

  <div class="form-group">
    <label for="filter-rating" class="form-label">{m.fields.rating}</label>
    <select
      id="filter-rating"
      class="form-input"
      value={filters.minRating}
      onchange={(e) => onChange({ ...filters, minRating: Number(e.currentTarget.value) })}
    >
      <option value={0}>{m.library.filters.anyRating}</option>
      {#each ratings as rating (rating)}
        <option value={rating}>{m.library.filters.ratingAtLeast(rating)}</option>
      {/each}
    </select>
  </div>

  <div class="form-group">
    <span class="form-label" id="filter-year-label">{m.fields.year}</span>
    <div class="flex items-center gap-2" role="group" aria-labelledby="filter-year-label">
      <input
        type="text"
        inputmode="numeric"
        class="form-input year-input"
        value={filters.yearFrom ?? ''}
        oninput={(e) => onChange({ ...filters, yearFrom: parseYear(e.currentTarget.value) })}
        placeholder={m.library.filters.yearFrom}
        aria-label={m.library.filters.yearFrom}
      />
      <span class="text-text-muted">〜</span>
      <input
        type="text"
        inputmode="numeric"
        class="form-input year-input"
        value={filters.yearTo ?? ''}
        oninput={(e) => onChange({ ...filters, yearTo: parseYear(e.currentTarget.value) })}
        placeholder={m.library.filters.yearTo}
        aria-label={m.library.filters.yearTo}
      />
    </div>
  </div>

  <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
    <input
      type="checkbox"
      class="w-4 h-4"
      checked={filters.favoritesOnly}
      onchange={(e) => onChange({ ...filters, favoritesOnly: e.currentTarget.checked })}
    />
    {m.library.filters.favoritesOnly}
  </label>

  <div class="flex justify-end mt-4">
    <button
      type="button"
      class="btn-secondary text-xs"
      onclick={() => onChange(EMPTY_FILTERS)}
      disabled={activeCount === 0}
    >
      {m.library.filters.clear}
    </button>
  </div>
</Popover>

<style>
  @reference "../../../app.css";

  .filter-button {
    @apply flex items-center gap-1.5 h-9 px-3 bg-base-400 border border-border rounded-md text-sm text-text-muted cursor-pointer transition-colors;
  }

  .filter-button:hover {
    @apply text-text-primary;
  }

  .filter-button.active {
    @apply border-primary text-primary;
  }

  .badge {
    @apply flex items-center justify-center min-w-4 h-4 px-1 rounded-full bg-primary text-white text-xs leading-none;
  }

  .year-input {
    @apply w-20 text-center;
  }
</style>
