<!--
  @component Tabs
  タブ。Bits UIのTabsを包む。左右の矢印キー・Home・Endでタブを移り、移った先のタブがそのまま選ばれる。
  中には、タブごとの内容を`TabPanel`で置く（選ばれていない内容は、隠すだけで残る）。
-->
<script lang="ts" generics="T extends string">
  import type { Snippet } from 'svelte';
  import { Tabs } from 'bits-ui';

  interface Props {
    /** 選ばれているタブ */
    value: T;
    /** タブの並び */
    tabs: { value: T; label: string }[];
    /** タブを切り替えられなくする（処理中など） */
    disabled?: boolean;
    /** タブの並びを出すか（出さない時は、選ばれている内容だけが出る） */
    showList?: boolean;
    children: Snippet;
  }

  let { value = $bindable(), tabs, disabled = false, showList = true, children }: Props = $props();
</script>

<Tabs.Root {value} onValueChange={(next) => (value = next as T)}>
  {#if showList}
    <Tabs.List>
      {#snippet child({ props })}
        <div {...props} class="tab-list">
          {#each tabs as tab (tab.value)}
            <Tabs.Trigger value={tab.value} {disabled}>
              {#snippet child({ props: triggerProps })}
                <button {...triggerProps} type="button" class="tab">{tab.label}</button>
              {/snippet}
            </Tabs.Trigger>
          {/each}
        </div>
      {/snippet}
    </Tabs.List>
  {/if}
  {@render children()}
</Tabs.Root>

<style>
  @reference "../../../app.css";

  .tab-list {
    @apply flex gap-1 mb-4 border-b border-border;
  }

  .tab {
    @apply bg-transparent border-none px-4 py-2 text-sm text-text-muted cursor-pointer border-b-2 border-transparent transition-colors;
    margin-bottom: -1px;
  }

  .tab:hover:not(:disabled) {
    @apply text-text-primary;
  }

  .tab:disabled {
    @apply cursor-not-allowed;
  }

  /* 選ばれているタブ（Bits UIが付ける） */
  .tab[data-state='active'] {
    @apply text-primary border-primary font-semibold;
  }
</style>
