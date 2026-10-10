<!--
  @component SmartPlaylistDialog
  自動プレイリスト（条件で曲を集めるプレイリスト）の編集画面。新しく作る時と、条件を変える時に使う。

  - 条件は、項目・比べ方・値の組の一覧。「すべての条件に合う曲」か「いずれかの条件に合う曲」を選ぶ
  - 曲数の上限と並び順を付けられる（上限は、並び順の先頭から選ぶ）
  - 入力を変えるたびに、条件に合う曲数を出す（数えるのはバックエンド）

  入力と条件（`SmartRules`）の変換・検証は`#lib/utils/smartPlaylist`が行う。
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import type { MatchMode, Playlist, SmartRules } from '#lib/types/models.js';
  import {
    useCreateSmartPlaylistMutation,
    useSmartPlaylistCountQuery,
    useUpdateSmartPlaylistMutation
  } from '#lib/queries/playlists.js';
  import {
    MAX_RULES,
    ORDER_FIELDS,
    RULE_FIELDS,
    RULE_FIELD_IDS,
    changeRuleField,
    initialOrder,
    isDirected,
    needsValue,
    newRuleDraft,
    newSmartRulesForm,
    opsFor,
    orderFieldLabel,
    ruleFieldLabel,
    ruleOpLabel,
    toSmartRules,
    toSmartRulesForm,
    type RuleDraft,
    type RuleField
  } from '#lib/utils/smartPlaylist.js';
  import { validatePlaylistName, toSafeString } from '#lib/utils/validation.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 条件を変えるプレイリスト（nullなら、新しく作る） */
    playlist: Playlist | null;
    onClose: () => void;
  }

  let { playlist, onClose }: Props = $props();

  /** 条件に合う曲数を数え直すまでの、入力の待ち時間（ミリ秒） */
  const COUNT_DEBOUNCE_MS = 300;
  const matchModes: MatchMode[] = ['all', 'any'];
  const ratings = [0, 1, 2, 3, 4, 5];

  // 入力は、開いた時点の内容から始める（開いている間に一覧が更新されても変えない）
  // svelte-ignore state_referenced_locally
  const target = playlist;
  const initial = target?.rules ? toSmartRulesForm(target.rules) : newSmartRulesForm();

  let name = $state(target?.name ?? '');
  let matchMode = $state(initial.matchMode);
  let drafts = $state<RuleDraft[]>([...initial.drafts]);
  let isLimited = $state(initial.isLimited);
  let limit = $state(initial.limit);
  let order = $state(initial.order);
  let nextKey = initial.drafts.length;

  /** 保存しようとしたか（値を入れていない条件の指摘は、保存しようとしてから出す） */
  let hasTriedSaving = $state(false);

  const result = $derived(toSmartRules({ matchMode, drafts, isLimited, limit, order }));
  const nameError = $derived(validatePlaylistName(name.trim()).error ?? null);
  const inputError = $derived.by(() => {
    if (result.error === null) return null;
    if (result.error === 'valueRequired' && !hasTriedSaving) return null;
    return m.smartPlaylist.errors[result.error];
  });
  const invalidRuleKey = $derived(
    inputError !== null && result.rules === null ? result.ruleKey : null
  );

  // 条件に合う曲数（入力が落ち着いてから数える）
  let countedRules = $state.raw<SmartRules | null>(null);
  $effect(() => {
    const rules = result.rules;
    if (rules === null) {
      countedRules = null;
      return;
    }
    const timer = setTimeout(() => (countedRules = rules), COUNT_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });
  const countQuery = $derived(countedRules ? useSmartPlaylistCountQuery(countedRules) : null);
  const matchCount = $derived(countQuery?.data ?? null);

  const createMutation = useCreateSmartPlaylistMutation();
  const updateMutation = useUpdateSmartPlaylistMutation();
  const isSaving = $derived(createMutation.isPending || updateMutation.isPending);

  function addRule() {
    drafts.push(newRuleDraft(nextKey++));
  }

  function removeRule(key: number) {
    drafts = drafts.filter((draft) => draft.key !== key);
  }

  function setField(index: number, field: RuleField) {
    drafts[index] = changeRuleField(drafts[index], field);
  }

  async function handleSave() {
    hasTriedSaving = true;
    if (nameError !== null || result.rules === null) return;
    const newName = toSafeString(name.trim(), 100);

    try {
      if (target) {
        await updateMutation.mutateAsync({ playlist: target, name: newName, rules: result.rules });
      } else {
        const created = await createMutation.mutateAsync({ name: newName, rules: result.rules });
        goto(resolve(`playlists/${created.id}`));
      }
      onClose();
    } catch (error) {
      // 失敗はトーストで通知される。画面は閉じずに、入力を残す
      console.error('自動プレイリストの保存に失敗しました:', error);
    }
  }
</script>

<Modal
  open
  {onClose}
  title={target ? m.smartPlaylist.editTitle : m.smartPlaylist.newTitle}
  dismissible={!isSaving}
  class="max-w-3xl"
>
  <form
    id="smart-playlist-form"
    onsubmit={(e) => {
      e.preventDefault();
      handleSave();
    }}
  >
    <div class="form-group">
      <label for="smart-playlist-name" class="form-label">{m.smartPlaylist.name}</label>
      <input
        id="smart-playlist-name"
        type="text"
        class="form-input"
        bind:value={name}
        maxlength="100"
        autocomplete="off"
        disabled={isSaving}
        data-autofocus
      />
    </div>

    <div class="section-header">
      <h3 class="section-title">{m.smartPlaylist.rules}</h3>
      {#if drafts.length > 1}
        <label class="inline">
          <span class="sr-only">{m.smartPlaylist.match}</span>
          <select class="form-input compact" bind:value={matchMode} disabled={isSaving}>
            {#each matchModes as mode (mode)}
              <option value={mode}>{m.smartPlaylist.matchModes[mode]}</option>
            {/each}
          </select>
        </label>
      {/if}
    </div>

    <ul class="rules">
      {#each drafts as draft, index (draft.key)}
        {@const definition = RULE_FIELDS[draft.field]}
        <li class="rule" class:invalid={invalidRuleKey === draft.key}>
          <select
            class="form-input rule-field"
            value={draft.field}
            onchange={(e) => setField(index, e.currentTarget.value as RuleField)}
            aria-label={m.smartPlaylist.ruleField}
            disabled={isSaving}
          >
            {#each RULE_FIELD_IDS as field (field)}
              <option value={field}>{ruleFieldLabel(field)}</option>
            {/each}
          </select>
          <select
            class="form-input rule-op"
            bind:value={draft.op}
            aria-label={m.smartPlaylist.ruleOp}
            disabled={isSaving}
          >
            {#each opsFor(draft.field) as op (op)}
              <option value={op}>{ruleOpLabel(draft.field, op)}</option>
            {/each}
          </select>

          <div class="rule-value">
            {#if !needsValue(draft)}
              <!-- 値のいらない比べ方（空かどうか・お気に入りかどうか） -->
            {:else if definition.kind === 'text'}
              <input
                type="text"
                class="form-input"
                bind:value={draft.value}
                aria-label={m.smartPlaylist.ruleValue}
                autocomplete="off"
                spellcheck="false"
                disabled={isSaving}
              />
            {:else if definition.input === 'rating'}
              <select
                class="form-input compact"
                bind:value={draft.value}
                aria-label={m.smartPlaylist.ruleValue}
                disabled={isSaving}
              >
                {#each ratings as rating (rating)}
                  <option value={String(rating)}>
                    {rating === 0 ? m.smartPlaylist.noRating : '★'.repeat(rating)}
                  </option>
                {/each}
              </select>
              {#if draft.op === 'between'}
                <span class="unit">〜</span>
                <select
                  class="form-input compact"
                  bind:value={draft.valueTo}
                  aria-label={m.smartPlaylist.ruleValueTo}
                  disabled={isSaving}
                >
                  {#each ratings as rating (rating)}
                    <option value={String(rating)}>
                      {rating === 0 ? m.smartPlaylist.noRating : '★'.repeat(rating)}
                    </option>
                  {/each}
                </select>
              {/if}
            {:else}
              <input
                type="text"
                inputmode={definition.input === 'minutes' ? 'decimal' : 'numeric'}
                class="form-input number"
                bind:value={draft.value}
                aria-label={m.smartPlaylist.ruleValue}
                autocomplete="off"
                disabled={isSaving}
              />
              {#if draft.op === 'between'}
                <span class="unit">〜</span>
                <input
                  type="text"
                  inputmode={definition.input === 'minutes' ? 'decimal' : 'numeric'}
                  class="form-input number"
                  bind:value={draft.valueTo}
                  aria-label={m.smartPlaylist.ruleValueTo}
                  autocomplete="off"
                  disabled={isSaving}
                />
              {/if}
              {#if definition.kind === 'date'}
                <span class="unit">{m.smartPlaylist.days}</span>
              {:else if definition.input === 'minutes'}
                <span class="unit">{m.smartPlaylist.minutesUnit}</span>
              {:else if draft.field === 'bitrate'}
                <span class="unit">kbps</span>
              {/if}
            {/if}
          </div>

          <button
            type="button"
            class="btn-icon remove"
            onclick={() => removeRule(draft.key)}
            title={m.smartPlaylist.removeRule}
            aria-label={m.smartPlaylist.removeRule}
            disabled={isSaving}
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
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </li>
      {/each}
    </ul>

    {#if drafts.length === 0}
      <p class="hint">{m.smartPlaylist.noRules}</p>
    {/if}

    <button
      type="button"
      class="btn-secondary add"
      onclick={addRule}
      disabled={isSaving || drafts.length >= MAX_RULES}
    >
      {m.smartPlaylist.addRule}
    </button>
    {#if drafts.some((draft) => RULE_FIELDS[draft.field].kind === 'text' && needsValue(draft))}
      <p class="hint">{m.smartPlaylist.textHint}</p>
    {/if}

    <div class="section-header">
      <h3 class="section-title">{m.smartPlaylist.order}</h3>
    </div>
    <div class="options">
      <select
        class="form-input compact"
        value={order.field}
        onchange={(e) =>
          (order = initialOrder(e.currentTarget.value as (typeof ORDER_FIELDS)[number]))}
        aria-label={m.smartPlaylist.order}
        disabled={isSaving}
      >
        {#each ORDER_FIELDS as field (field)}
          <option value={field}>{orderFieldLabel(field)}</option>
        {/each}
      </select>
      {#if isDirected(order.field)}
        <select
          class="form-input compact"
          value={order.descending ? 'descending' : 'ascending'}
          onchange={(e) =>
            (order = { ...order, descending: e.currentTarget.value === 'descending' })}
          aria-label={m.smartPlaylist.order}
          disabled={isSaving}
        >
          <option value="ascending">{m.smartPlaylist.ascending}</option>
          <option value="descending">{m.smartPlaylist.descending}</option>
        </select>
      {/if}
      <label class="check">
        <input type="checkbox" class="w-4 h-4" bind:checked={isLimited} disabled={isSaving} />
        {m.smartPlaylist.limit}
      </label>
      {#if isLimited}
        <input
          type="text"
          inputmode="numeric"
          class="form-input number"
          bind:value={limit}
          aria-label={m.smartPlaylist.limit}
          autocomplete="off"
          disabled={isSaving}
        />
        <span class="unit">{m.smartPlaylist.limitUnit}</span>
      {/if}
    </div>
    {#if isLimited}
      <p class="hint">{m.smartPlaylist.orderHint}</p>
    {/if}
  </form>

  {#if hasTriedSaving && nameError}
    <div class="message-error mt-3" role="alert">{nameError}</div>
  {:else if inputError}
    <div class="message-error mt-3" role="alert">{inputError}</div>
  {/if}

  <p class="count" aria-live="polite">
    {#if result.rules === null}
      &nbsp;
    {:else if matchCount === null}
      {m.smartPlaylist.counting}
    {:else}
      {m.smartPlaylist.matchCount(matchCount)}
    {/if}
  </p>

  {#snippet footer()}
    <button type="button" class="btn-secondary" onclick={onClose} disabled={isSaving}>
      {m.common.cancel}
    </button>
    <button type="submit" form="smart-playlist-form" class="btn-primary" disabled={isSaving}>
      {#if isSaving}
        {m.common.saving}
      {:else}
        {target ? m.common.save : m.smartPlaylist.create}
      {/if}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .section-header {
    @apply flex items-center justify-between gap-3 mt-2 mb-2;
    min-height: 2.25rem;
  }

  .section-title {
    @apply m-0 text-sm font-semibold text-text-secondary;
  }

  .inline {
    @apply flex items-center gap-2;
  }

  .rules {
    @apply list-none m-0 p-0 flex flex-col gap-2;
  }

  /* 1つの条件（項目・比べ方・値・削除） */
  .rule {
    @apply grid items-center gap-2 p-2 rounded border border-border bg-base-300;
    grid-template-columns: minmax(8rem, 11rem) minmax(8rem, 11rem) minmax(0, 1fr) auto;
  }

  .rule.invalid {
    @apply border-error;
  }

  .rule-value {
    @apply flex items-center gap-2 min-w-0;
  }

  /* 選ぶ内容に合わせた幅にする（`.form-input`は、既定で幅いっぱいになる） */
  .compact {
    @apply w-auto;
  }

  .number {
    @apply w-24 text-right;
  }

  .unit {
    @apply text-sm text-text-muted whitespace-nowrap;
  }

  .remove {
    @apply w-8 h-8 p-0;
  }

  .add {
    @apply mt-2;
  }

  .options {
    @apply flex flex-wrap items-center gap-x-3 gap-y-2;
  }

  .check {
    @apply flex items-center gap-2 ml-3 text-sm text-text-secondary cursor-pointer whitespace-nowrap;
  }

  .hint {
    @apply mt-2 mb-0 text-xs text-text-muted;
  }

  .count {
    @apply mt-4 mb-0 text-sm text-text-secondary;
  }
</style>
