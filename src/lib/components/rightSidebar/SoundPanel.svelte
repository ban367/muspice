<!--
  @component SoundPanel
  右サイドバーのサウンドのパネル。音の設定を、まとまり（`SidePanelSection`）ごとに、音を処理する順に縦に並べる
  （リプレイゲイン → 曲のつなぎ方（ギャップレス再生・クロスフェード） → イコライザ）。
  リプレイゲインと曲のつなぎ方は、変えた時点で設定（`settings.json`）へ保存する（設定画面のような「適用」はない）。
  イコライザの設定は、localStorageに保存する（`equalizer.svelte.ts`）。
-->
<script lang="ts">
  import SidePanel from './SidePanel.svelte';
  import ReplayGainSection from './ReplayGainSection.svelte';
  import CrossfadeSection from './CrossfadeSection.svelte';
  import EqualizerSection from './EqualizerSection.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';
</script>

<SidePanel title={m.rightSidebar.soundTitle}>
  <ReplayGainSection />
  <CrossfadeSection />
  <EqualizerSection />
</SidePanel>
