<!--
  @component ReplayGainSection
  サウンドのパネル（`SoundPanel`）のリプレイゲイン（音量の正規化）のまとまり。見出しのスイッチでオン・オフし、
  補正の単位（トラック / アルバム / 自動）・プリアンプ・ゲインのない曲の音量・クリップ防止を変えて、
  その場で設定（`settings.json`）へ保存する。単位などは、オフの間も覚えておく。
  補正は再生エンジンがかける（`normalization.rs`）。再生中の曲にかけている補正も出す（`appliedReplayGain`）。
-->
<script lang="ts">
  import { MAX_NORMALIZATION_PREAMP_DB } from '#lib/bindings.js';
  import { useUpdateSettings } from '#lib/queries/settings.js';
  import { player } from '#lib/stores/player.svelte.js';
  import type { VolumeNormalization } from '#lib/types/models.js';
  import { appliedReplayGain } from '#lib/utils/replayGain.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import SidePanelSection from './SidePanelSection.svelte';
  import SidePanelCheckbox from './SidePanelCheckbox.svelte';
  import SidePanelOptions from './SidePanelOptions.svelte';
  import SidePanelSlider from './SidePanelSlider.svelte';
  import SidePanelToggle from './SidePanelToggle.svelte';

  const MODES: VolumeNormalization[] = ['track', 'album', 'auto'];

  const settings = useUpdateSettings();
  const current = $derived(settings.current);
  const enabled = $derived(current?.volumeNormalizationEnabled ?? false);
  const isOff = $derived(current === null || !enabled);
  const modeOptions = $derived(
    MODES.map((value) => ({ value, label: m.soundPanel.replayGainModes[value] }))
  );

  // 再生中の曲にかけている補正（正規化がオフ・曲がない時はnull）
  const applied = $derived.by(() => {
    const track = player.currentTrack;
    if (current === null || track === null) return null;
    return appliedReplayGain(track.replayGain, current, player.isShuffling);
  });

  /** 補正の量の表示（小数1桁。-0.0にしない） */
  function formatGain(db: number): string {
    return m.soundPanel.gainValue(Math.round(db * 10) / 10 || 0);
  }
</script>

<SidePanelSection title={m.soundPanel.replayGain} variant="group">
  {#snippet actions()}
    <SidePanelToggle
      checked={enabled}
      label={m.soundPanel.replayGain}
      title={enabled ? m.soundPanel.turnOffReplayGain : m.soundPanel.turnOnReplayGain}
      disabled={current === null}
      onchange={(volumeNormalizationEnabled) => settings.update({ volumeNormalizationEnabled })}
    />
  {/snippet}
  <div class="fields">
    <div class="field">
      <span class="label">{m.soundPanel.replayGainMode}</span>
      <SidePanelOptions
        label={m.soundPanel.replayGainMode}
        options={modeOptions}
        value={current?.volumeNormalization ?? 'auto'}
        disabled={isOff}
        onchange={(volumeNormalization) => settings.update({ volumeNormalization })}
      />
    </div>
    <SidePanelSlider
      label={m.soundPanel.preamp}
      value={current?.normalizationPreampDb ?? 0}
      min={-MAX_NORMALIZATION_PREAMP_DB}
      max={MAX_NORMALIZATION_PREAMP_DB}
      disabled={isOff}
      format={m.soundPanel.preampValue}
      onchange={(normalizationPreampDb) => settings.update({ normalizationPreampDb })}
    />
    <SidePanelSlider
      label={m.soundPanel.untaggedPreamp}
      value={current?.normalizationUntaggedPreampDb ?? 0}
      min={-MAX_NORMALIZATION_PREAMP_DB}
      max={MAX_NORMALIZATION_PREAMP_DB}
      disabled={isOff}
      format={m.soundPanel.preampValue}
      onchange={(normalizationUntaggedPreampDb) =>
        settings.update({ normalizationUntaggedPreampDb })}
    />
    <SidePanelCheckbox
      label={m.soundPanel.preventClipping}
      checked={current?.normalizationPreventClipping ?? true}
      disabled={isOff}
      onchange={(normalizationPreventClipping) => settings.update({ normalizationPreventClipping })}
    />
    {#if applied !== null}
      <div class="current" role="status">
        <div class="current-row">
          <span class="current-label">{m.soundPanel.currentGain}</span>
          <span class="current-value">
            {m.soundPanel.gainDetail(
              formatGain(applied.gainDb),
              m.soundPanel.gainSources[applied.source]
            )}
          </span>
        </div>
        {#if applied.limited}
          <p class="current-note">{m.soundPanel.gainLimited}</p>
        {/if}
      </div>
    {/if}
  </div>
</SidePanelSection>

<style>
  @reference "../../../app.css";

  .fields {
    @apply flex flex-col gap-3;
  }

  .field {
    @apply flex flex-col gap-1.5;
  }

  .label {
    @apply text-xs text-text-secondary;
  }

  /* 再生中の曲にかけている補正 */
  .current {
    @apply px-2 py-1.5 rounded-md bg-surface;
  }

  .current-row {
    @apply flex items-baseline justify-between gap-2;
  }

  .current-note {
    @apply mt-0.5 mb-0 text-[0.6875rem] text-text-muted;
  }

  .current-label {
    @apply text-xs text-text-muted;
  }

  .current-value {
    @apply text-xs text-text-secondary tabular-nums truncate;
  }
</style>
