<!--
  @component CrossfadeSection
  サウンドのパネル（`SoundPanel`）の曲のつなぎ方のまとまり。ギャップレス再生とクロスフェードを、それぞれ見出しの
  スイッチでオン・オフし、その場で設定（`settings.json`）へ保存する。クロスフェードの秒数は、オフの間も覚えておく。
  同じアルバムの続きの曲では重ねない設定も、ここで変える。ギャップレス再生のまとまりは、見出しのスイッチだけ。
  曲をどうつなぐかは再生エンジンが決める（`engine.rs`の`crossfade_plan`）。
-->
<script lang="ts">
  import { MAX_CROSSFADE_SECONDS, MIN_CROSSFADE_SECONDS } from '#lib/bindings.js';
  import { useUpdateSettings } from '#lib/queries/settings.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import SidePanelSection from './SidePanelSection.svelte';
  import SidePanelCheckbox from './SidePanelCheckbox.svelte';
  import SidePanelSlider from './SidePanelSlider.svelte';
  import SidePanelToggle from './SidePanelToggle.svelte';

  const settings = useUpdateSettings();
  const current = $derived(settings.current);
  const gapless = $derived(current?.gaplessPlayback ?? true);
  const crossfade = $derived(current?.crossfadeEnabled ?? false);
</script>

<SidePanelSection title={m.soundPanel.gapless} variant="group">
  {#snippet actions()}
    <SidePanelToggle
      checked={gapless}
      label={m.soundPanel.gapless}
      title={gapless ? m.soundPanel.turnOffGapless : m.soundPanel.turnOnGapless}
      disabled={current === null}
      onchange={(gaplessPlayback) => settings.update({ gaplessPlayback })}
    />
  {/snippet}
</SidePanelSection>

<SidePanelSection title={m.soundPanel.crossfade} variant="group">
  {#snippet actions()}
    <SidePanelToggle
      checked={crossfade}
      label={m.soundPanel.crossfade}
      title={crossfade ? m.soundPanel.turnOffCrossfade : m.soundPanel.turnOnCrossfade}
      disabled={current === null}
      onchange={(crossfadeEnabled) => settings.update({ crossfadeEnabled })}
    />
  {/snippet}
  <div class="fields">
    <SidePanelSlider
      label={m.soundPanel.crossfadeDuration}
      value={current?.crossfadeSeconds ?? MIN_CROSSFADE_SECONDS}
      min={MIN_CROSSFADE_SECONDS}
      max={MAX_CROSSFADE_SECONDS}
      disabled={current === null || !crossfade}
      format={m.soundPanel.crossfadeSeconds}
      onchange={(crossfadeSeconds) => settings.update({ crossfadeSeconds })}
    />
    <SidePanelCheckbox
      label={m.soundPanel.crossfadeSkipAlbumTracks}
      checked={current?.crossfadeSkipAlbumTracks ?? true}
      disabled={current === null || !crossfade}
      onchange={(crossfadeSkipAlbumTracks) => settings.update({ crossfadeSkipAlbumTracks })}
    />
  </div>
</SidePanelSection>

<style>
  @reference "../../../app.css";

  .fields {
    @apply flex flex-col gap-3;
  }
</style>
