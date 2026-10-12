<!--
  @component Popover
  ボタンから開くパネル（入力の部品を置く小さな面）。Bits UIのPopoverを包む。
  開くとフォーカスがパネルの中へ移り、Escキー・外側の操作で閉じると、ボタンへ戻る。
  画面の端では、収まる側へ位置を変える。ボタンは`trigger`のスニペットで描画し、
  受け取った属性を`<button>`に広げる。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Popover } from 'bits-ui';

  interface Props {
    /** パネルの名前（スクリーンリーダー向け） */
    label: string;
    /** パネルを出す側 */
    side?: 'top' | 'bottom';
    /** ボタンに対する、パネルの揃え方 */
    align?: 'start' | 'center' | 'end';
    /** ボタンとパネルの間隔（px） */
    sideOffset?: number;
    /** パネルに追加するクラス（幅の指定など） */
    class?: string;
    /** 開くボタン。受け取った属性を`<button>`に広げる */
    trigger: Snippet<[Record<string, unknown>]>;
    children: Snippet;
  }

  let {
    label,
    side = 'bottom',
    align = 'end',
    sideOffset = 8,
    class: className = '',
    trigger,
    children
  }: Props = $props();
</script>

<Popover.Root>
  <Popover.Trigger>
    {#snippet child({ props })}
      {@render trigger(props)}
    {/snippet}
  </Popover.Trigger>
  <Popover.Portal>
    <Popover.Content {side} {align} {sideOffset} collisionPadding={10} aria-label={label}>
      {#snippet child({ props, wrapperProps })}
        <div {...wrapperProps}>
          <div {...props} role="dialog" class={['popover-panel', className]}>
            {@render children()}
          </div>
        </div>
      {/snippet}
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>
