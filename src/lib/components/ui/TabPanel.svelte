<!--
  @component TabPanel
  `Tabs`の中の、タブごとの内容。選ばれていない間は隠す（入力した内容は残る）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Tabs } from 'bits-ui';

  interface Props {
    /** 対応するタブ */
    value: string;
    /**
     * Tabキーで内容そのものへ移れるようにするか。入力の部品がない内容（情報の表示だけ）で
     * trueにする（スクリーンリーダーで読めるようにするため）
     */
    focusable?: boolean;
    children: Snippet;
  }

  let { value, focusable = false, children }: Props = $props();
</script>

<Tabs.Content {value} tabindex={focusable ? 0 : -1}>
  {@render children()}
</Tabs.Content>
