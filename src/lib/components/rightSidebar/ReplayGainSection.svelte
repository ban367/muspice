<!--
  @component ReplayGainSection
  サウンドのパネル（`SoundPanel`）のリプレイゲイン（音量の正規化）のまとまり。補正の単位（オフ / トラック / アルバム）と
  プリアンプを変え、その場で設定（`settings.json`）へ保存する。補正は再生エンジンがかける（`normalization.rs`）。
-->
<script lang="ts">
  import { MAX_NORMALIZATION_PREAMP_DB } from '#lib/bindings.js';
  import { useUpdateSettings } from '#lib/queries/settings.js';
  import type { VolumeNormalization } from '#lib/types/models.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import SidePanelSection from './SidePanelSection.svelte';
  import SidePanelOptions from './SidePanelOptions.svelte';
  import SidePanelSlider from './SidePanelSlider.svelte';

  const MODES: VolumeNormalization[] = ['off', 'track', 'album'];

  const settings = useUpdateSettings();
  const current = $derived(settings.current);
  const mode = $derived(current?.volumeNormalization ?? 'off');
  const modeOptions = $derived(
    MODES.map((value) => ({ value, label: m.soundPanel.replayGainModes[value] }))
  );
</script>

<SidePanelSection title={m.soundPanel.replayGain}>
  <div class="fields">
    <SidePanelOptions
      label={m.soundPanel.replayGain}
      options={modeOptions}
      value={mode}
      disabled={current === null}
      onchange={(volumeNormalization) => settings.update({ volumeNormalization })}
    />
    <p class="hint">{m.soundPanel.replayGainHint}</p>
    <SidePanelSlider
      label={m.soundPanel.preamp}
      value={current?.normalizationPreampDb ?? 0}
      min={-MAX_NORMALIZATION_PREAMP_DB}
      max={MAX_NORMALIZATION_PREAMP_DB}
      disabled={current === null || mode === 'off'}
      format={m.soundPanel.preampValue}
      hint={m.soundPanel.preampHint}
      onchange={(normalizationPreampDb) => settings.update({ normalizationPreampDb })}
    />
  </div>
</SidePanelSection>

<style>
  @reference "../../../app.css";

  .fields {
    @apply flex flex-col gap-3;
  }

  .hint {
    @apply -mt-1.5 mb-0 text-[0.6875rem] leading-snug text-text-muted;
  }
</style>
