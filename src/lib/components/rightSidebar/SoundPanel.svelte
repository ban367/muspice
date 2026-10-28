<!--
  @component SoundPanel
  右サイドバーのサウンドのパネル。音の加工の設定を、まとまり（`SidePanelSection`）ごとに縦に並べる。
  今はイコライザだけ。同じ種類の設定（再生速度・音量の正規化など）を足す時は、まとまりのコンポーネントを作って並べる。
-->
<script lang="ts">
  import SidePanel from './SidePanel.svelte';
  import EqualizerSection from './EqualizerSection.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';
</script>

<SidePanel title={m.rightSidebar.soundTitle}>
  <EqualizerSection />
</SidePanel>
