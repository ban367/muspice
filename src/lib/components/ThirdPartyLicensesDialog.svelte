<!--
  @component ThirdPartyLicensesDialog
  アプリに組み込んだ依存ライブラリ（Rustのクレート・npmのパッケージ）の、ライセンスの一覧（ADR-091）。
  「Muspice について」から開く。一覧は、ビルドの時に作って成果物に入れてあり、開いた時に読む。
  パッケージを開くと、ライセンスの文を出す（文は長いため、開いたものだけを描く）。
-->
<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import {
    licenseTexts,
    loadLicenseList,
    type LicenseList
  } from '#lib/utils/thirdPartyLicenses.js';

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  /** 読んだ一覧（まだ読んでいなければundefined。一覧のない環境ではnull） */
  let list = $state.raw<LicenseList | null | undefined>(undefined);
  /** 開いているパッケージ（一覧の中の位置） */
  const expanded = new SvelteSet<number>();

  // 初めて開いた時に、一覧を読む
  $effect(() => {
    if (open && list === undefined) {
      void loadLicenseList().then((loaded) => {
        list = loaded;
      });
    }
  });

  function toggle(index: number, isOpen: boolean) {
    if (isOpen) expanded.add(index);
    else expanded.delete(index);
  }
</script>

<Modal {open} {onClose} title={m.licenses.title} class="max-w-3xl">
  <p class="licenses-intro">{m.licenses.intro}</p>
  {#if list === undefined}
    <p class="licenses-note" role="status">{m.common.loading}</p>
  {:else if list === null}
    <p class="licenses-note">{m.licenses.unavailable}</p>
  {:else}
    {#if list.rustMissing}
      <p class="licenses-note text-warning">{m.licenses.rustMissing}</p>
    {/if}
    <p class="licenses-count">{m.licenses.count(list.packages.length)}</p>
    <ul class="licenses-list">
      {#each list.packages as pkg, index (`${pkg.ecosystem}:${pkg.name}@${pkg.version}`)}
        <li>
          <details
            class="license-item"
            ontoggle={(event) => toggle(index, event.currentTarget.open)}
          >
            <summary class="license-summary">
              <span class="license-name">{pkg.name}</span>
              <span class="license-version">{pkg.version}</span>
              <span class="license-expression">{pkg.license}</span>
            </summary>
            {#if expanded.has(index)}
              {#if pkg.repository}
                <p class="license-repository">{pkg.repository}</p>
              {/if}
              {#each licenseTexts(list, pkg) as text, textIndex (textIndex)}
                <pre class="license-text">{text}</pre>
              {/each}
            {/if}
          </details>
        </li>
      {/each}
    </ul>
  {/if}

  {#snippet footer()}
    <button type="button" class="btn-primary" onclick={onClose} data-autofocus>
      {m.common.close}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .licenses-intro {
    @apply text-sm m-0 mb-3;
  }

  .licenses-note {
    @apply text-sm text-text-secondary m-0 mb-3;
  }

  .licenses-count {
    @apply text-xs text-text-muted m-0 mb-2;
  }

  .licenses-list {
    @apply list-none m-0 p-0 flex flex-col;
  }

  .license-item {
    @apply border-b border-border;
  }

  .license-summary {
    @apply flex items-baseline gap-2 py-1.5 text-sm cursor-pointer;
  }

  .license-name {
    @apply font-medium;
  }

  .license-version {
    @apply text-xs text-text-muted;
  }

  .license-expression {
    @apply ml-auto text-xs text-text-secondary text-right;
  }

  .license-repository {
    @apply text-xs text-text-secondary m-0 mb-2 break-all select-text;
  }

  .license-text {
    @apply text-xs whitespace-pre-wrap break-words bg-base-200 rounded p-3 m-0 mb-2 select-text;
  }
</style>
