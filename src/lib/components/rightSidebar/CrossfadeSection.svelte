<!--
  @component CrossfadeSection
  サウンドのパネル（`SoundPanel`）の曲のつなぎ方のまとまり。ギャップレス再生とクロスフェードを、それぞれ見出しの
  スイッチでオン・オフし、その場で設定（`settings.json`）へ保存する。クロスフェードの秒数は、オフの間も覚えておく。
  同じアルバムの続きの曲では重ねない設定も、ここで変える。
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

<SidePanelSection title={m.soundPanel.gapless}>
  {#snippet actions()}
    <SidePanelToggle
      checked={gapless}
      label={m.soundPanel.gapless}
      title={gapless ? m.soundPanel.turnOffGapless : m.soundPanel.turnOnGapless}
      disabled={current === null}
      onchange={(gaplessPlayback) => settings.update({ gaplessPlayback })}
    />
  {/snippet}
  <p class="hint">{m.soundPanel.gaplessHint}</p>
</SidePanelSection>

<SidePanelSection title={m.soundPanel.crossfade}>
  {#snippet actions()}
    <SidePanelToggle
      checked={crossfade}
      label={m.soundPanel.crossfade}
      title={crossfade ? m.soundPanel.turnOffCrossfade : m.soundPanel.turnOnCrossfade}
      disabled={current === null}
      onchange={(crossfadeEnabled) => settings.update({ crossfadeEnabled })}
    />
  {/snippet}
  <SidePanelSlider
    label={m.soundPanel.crossfadeDuration}
    value={current?.crossfadeSeconds ?? MIN_CROSSFADE_SECONDS}
    min={MIN_CROSSFADE_SECONDS}
    max={MAX_CROSSFADE_SECONDS}
    disabled={current === null || !crossfade}
    format={m.soundPanel.crossfadeSeconds}
    hint={m.soundPanel.crossfadeHint}
    onchange={(crossfadeSeconds) => settings.update({ crossfadeSeconds })}
  />
  <div class="option">
    <SidePanelCheckbox
      label={m.soundPanel.crossfadeSkipAlbumTracks}
      checked={current?.crossfadeSkipAlbumTracks ?? true}
      hint={m.soundPanel.crossfadeSkipAlbumTracksHint}
      disabled={current === null || !crossfade}
      onchange={(crossfadeSkipAlbumTracks) => settings.update({ crossfadeSkipAlbumTracks })}
    />
  </div>
</SidePanelSection>

<style>
  @reference "../../../app.css";

  .hint {
    @apply m-0 text-[0.6875rem] leading-snug text-text-muted;
  }

  .option {
    @apply mt-3;
  }
</style>
