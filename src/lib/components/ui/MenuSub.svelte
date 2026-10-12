<!--
  @component MenuSub
  サブメニュー。項目にポインターを乗せるか、右矢印キー・Enterキーで開く。
  中には`MenuItem`などを置く。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ContextMenu } from 'bits-ui';

  interface Props {
    label: string;
    /** 先頭に出すアイコン（`menu-icon`のクラスを付けたsvg） */
    icon?: Snippet;
    children: Snippet;
  }

  let { label, icon, children }: Props = $props();
</script>

<ContextMenu.Sub>
  <ContextMenu.SubTrigger>
    {#snippet child({ props })}
      <div {...props} class="menu-item">
        {@render icon?.()}
        <span class="menu-label">{label}</span>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="menu-arrow"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          aria-hidden="true"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </div>
    {/snippet}
  </ContextMenu.SubTrigger>
  <!-- 親のメニューの中に置くと、親のスクロールの範囲に切り取られるため、bodyへ出す -->
  <ContextMenu.Portal>
    <ContextMenu.SubContent sideOffset={4} collisionPadding={10}>
      {#snippet child({ props, wrapperProps })}
        <div {...wrapperProps}>
          <div {...props} class="menu-panel">
            {@render children()}
          </div>
        </div>
      {/snippet}
    </ContextMenu.SubContent>
  </ContextMenu.Portal>
</ContextMenu.Sub>
