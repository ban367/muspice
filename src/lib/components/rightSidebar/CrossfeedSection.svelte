<!--
  @component CrossfeedSection
  サウンドのパネル（`SoundPanel`）のクロスフィードのまとまり。見出しのスイッチでオン・オフし、強さ（弱・中・強）を選んで、
  その場で設定（`settings.json`）へ保存する。強さは、オフの間も覚えておく。
  音にかけるのは再生エンジンの出力のコールバック（`effects.rs`の`Crossfeed`）で、変えた結果はすぐに聞こえる。
-->
<script lang="ts">
  import { useUpdateSettings } from '#lib/queries/settings.js';
  import type { CrossfeedLevel } from '#lib/types/models.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import SidePanelSection from './SidePanelSection.svelte';
  import SidePanelOptions from './SidePanelOptions.svelte';
  import SidePanelToggle from './SidePanelToggle.svelte';

  const LEVELS: CrossfeedLevel[] = ['weak', 'medium', 'strong'];

  const settings = useUpdateSettings();
  const current = $derived(settings.current);
  const enabled = $derived(current?.crossfeedEnabled ?? false);
  const levelOptions = $derived(
    LEVELS.map((value) => ({ value, label: m.soundPanel.crossfeedLevels[value] }))
  );
</script>

<SidePanelSection title={m.soundPanel.crossfeed}>
  {#snippet actions()}
    <SidePanelToggle
      checked={enabled}
      label={m.soundPanel.crossfeed}
      title={enabled ? m.soundPanel.turnOffCrossfeed : m.soundPanel.turnOnCrossfeed}
      disabled={current === null}
      onchange={(crossfeedEnabled) => settings.update({ crossfeedEnabled })}
    />
  {/snippet}
  <div class="field">
    <span class="label">{m.soundPanel.crossfeedLevel}</span>
    <SidePanelOptions
      label={m.soundPanel.crossfeedLevel}
      options={levelOptions}
      value={current?.crossfeedLevel ?? 'medium'}
      disabled={current === null || !enabled}
      onchange={(crossfeedLevel) => settings.update({ crossfeedLevel })}
    />
    <p class="hint">{m.soundPanel.crossfeedHint}</p>
  </div>
</SidePanelSection>

<style>
  @reference "../../../app.css";

  .field {
    @apply flex flex-col gap-1.5;
  }

  .label {
    @apply text-xs text-text-secondary;
  }

  .hint {
    @apply m-0 text-[0.6875rem] leading-snug text-text-muted;
  }
</style>
