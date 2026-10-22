<!--
  @component PlaybackAidsMenu
  再生の補助のメニュー（プレーヤーのボタンから開く）。

  - 再生速度（0.5〜2倍。音程は変えない）
  - この曲が終わったら停止
  - スリープタイマー（時間を選ぶ。時間が来たら音量を下げて止めるか、曲の終わりまで再生してから止める）
  - Auto DJ（再生キューの最後の曲になったら、ライブラリかプレイリストから曲を足す）

  再生速度・「この曲が終わったら停止」・スリープタイマーは、その場かぎりの指示（`playbackAids`。
  保存しない）。Auto DJは設定（`settings.json`）に保存する。
-->
<script lang="ts">
  import { usePlaylistsQuery } from '#lib/queries/playlists.js';
  import { useSaveSettingsMutation, useSettingsQuery } from '#lib/queries/settings.js';
  import { player } from '#lib/stores/player.svelte.js';
  import { formatDuration } from '#lib/utils/format.js';
  import {
    MAX_PLAYBACK_SPEED,
    MIN_PLAYBACK_SPEED,
    PLAYBACK_SPEED_PRESETS,
    PLAYBACK_SPEED_STEP,
    playbackAids,
    SLEEP_TIMER_MINUTES
  } from '#lib/stores/playbackAids.svelte.js';
  import type { Settings } from '#lib/types/models.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import { Popover } from '#lib/components/ui/index.js';

  /** 曲の元の選択肢で、ライブラリ全体を表す値 */
  const LIBRARY_SOURCE = '';

  const settingsQuery = useSettingsQuery();
  const saveSettingsMutation = useSaveSettingsMutation();
  const playlistsQuery = usePlaylistsQuery();
  const settings = $derived(settingsQuery.data ?? null);
  const playlists = $derived(playlistsQuery.data ?? []);
  // 曲を選ぶプレイリストが削除されている場合は、ライブラリ全体として表示する
  const autoDjSource = $derived(
    playlists.some((playlist) => playlist.id === settings?.autoDjPlaylistId)
      ? (settings?.autoDjPlaylistId ?? LIBRARY_SOURCE)
      : LIBRARY_SOURCE
  );

  // スリープタイマーの残り時間の表示（設定している間、1秒ごとに更新する）
  let now = $state(Date.now());
  $effect(() => {
    if (playbackAids.sleepTimer === null) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const sleepRemaining = $derived(playbackAids.sleepRemainingSeconds(now));
  let waitForTrackEnd = $state(false);

  /** 再生速度を変えているか */
  const isSpeedChanged = $derived(playbackAids.speed !== 1);

  /** ボタンを目立たせるか（その場かぎりの指示が有効な間） */
  const isActive = $derived(
    isSpeedChanged || playbackAids.stopAfterCurrent || playbackAids.sleepTimer !== null
  );

  function saveSettings(patch: Partial<Settings>) {
    if (settings) saveSettingsMutation.mutate({ ...settings, ...patch });
  }

  function toggleSleepWaitForTrackEnd(checked: boolean) {
    waitForTrackEnd = checked;
    playbackAids.setSleepWaitForTrackEnd(checked);
  }
</script>

<div class="aids-menu">
  <!-- プレーヤーは画面の下にあるため、上へ開く -->
  <Popover label={m.playbackAids.title} side="top" sideOffset={12} class="w-72">
    {#snippet trigger(props)}
      <button
        {...props}
        type="button"
        class="aids-button"
        class:active={isActive}
        title={m.playbackAids.title}
        aria-label={m.playbackAids.title}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="18"
          height="18"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
      </button>
    {/snippet}

    <!-- 再生速度 -->
    <section>
      <div class="group-header">
        <h3 class="group-title">{m.playbackAids.speed}</h3>
        <span class="status" aria-live="polite">
          {m.playbackAids.speedValue(playbackAids.speed)}
        </span>
      </div>
      <div class="options" role="group" aria-label={m.playbackAids.speed}>
        {#each PLAYBACK_SPEED_PRESETS as speed (speed)}
          <button
            type="button"
            class="option-button"
            class:selected={playbackAids.speed === speed}
            aria-pressed={playbackAids.speed === speed}
            onclick={() => playbackAids.setSpeed(speed)}
          >
            {m.playbackAids.speedValue(speed)}
          </button>
        {/each}
      </div>
      <input
        type="range"
        class="speed-slider"
        min={MIN_PLAYBACK_SPEED}
        max={MAX_PLAYBACK_SPEED}
        step={PLAYBACK_SPEED_STEP}
        value={playbackAids.speed}
        oninput={(e) => playbackAids.setSpeed(Number(e.currentTarget.value))}
        aria-label={m.playbackAids.speedSlider}
      />
      <p class="hint speed-hint">{m.playbackAids.speedHint}</p>
      {#if isSpeedChanged}
        <button type="button" class="btn-secondary cancel" onclick={() => playbackAids.setSpeed(1)}>
          {m.playbackAids.speedReset}
        </button>
      {/if}
    </section>

    <!-- この曲が終わったら停止 -->
    <section class="group">
      <label class="check">
        <input
          type="checkbox"
          class="w-4 h-4"
          bind:checked={playbackAids.stopAfterCurrent}
          disabled={player.currentTrack === null}
        />
        {m.playbackAids.stopAfterCurrent}
      </label>
    </section>

    <!-- スリープタイマー -->
    <section class="group">
      <div class="group-header">
        <h3 class="group-title">{m.playbackAids.sleepTimer}</h3>
        {#if sleepRemaining !== null}
          <span class="status" aria-live="polite">
            {playbackAids.isFadingOut
              ? m.playbackAids.fadingOut
              : m.playbackAids.remaining(formatDuration(sleepRemaining, '0:00'))}
          </span>
        {/if}
      </div>
      <div class="options" role="group" aria-label={m.playbackAids.sleepTimer}>
        {#each SLEEP_TIMER_MINUTES as minutes (minutes)}
          <button
            type="button"
            class="option-button"
            class:selected={playbackAids.sleepTimer?.minutes === minutes}
            aria-pressed={playbackAids.sleepTimer?.minutes === minutes}
            onclick={() => playbackAids.startSleepTimer(minutes, waitForTrackEnd)}
          >
            {m.playbackAids.minutes(minutes)}
          </button>
        {/each}
      </div>
      <label class="check">
        <input
          type="checkbox"
          class="w-4 h-4"
          checked={playbackAids.sleepTimer?.waitForTrackEnd ?? waitForTrackEnd}
          onchange={(e) => toggleSleepWaitForTrackEnd(e.currentTarget.checked)}
        />
        {m.playbackAids.waitForTrackEnd}
      </label>
      {#if playbackAids.sleepTimer !== null}
        <button
          type="button"
          class="btn-secondary cancel"
          onclick={() => playbackAids.cancelSleepTimer()}
        >
          {m.playbackAids.cancelSleepTimer}
        </button>
      {/if}
    </section>

    <!-- Auto DJ -->
    <section class="group">
      <label class="check">
        <input
          type="checkbox"
          class="w-4 h-4"
          checked={settings?.autoDj ?? false}
          onchange={(e) => saveSettings({ autoDj: e.currentTarget.checked })}
          disabled={settings === null}
        />
        {m.playbackAids.autoDj}
      </label>
      <p class="hint">{m.playbackAids.autoDjHint}</p>
      <label class="source">
        <span class="source-label">{m.playbackAids.autoDjSource}</span>
        <select
          class="form-input"
          value={autoDjSource}
          onchange={(e) => saveSettings({ autoDjPlaylistId: e.currentTarget.value || null })}
          disabled={settings === null}
        >
          <option value={LIBRARY_SOURCE}>{m.playbackAids.autoDjLibrary}</option>
          {#each playlists as playlist (playlist.id)}
            <option value={playlist.id}>{playlist.name}</option>
          {/each}
        </select>
      </label>
    </section>
  </Popover>
  {#if isSpeedChanged}
    <span class="remaining" title={m.playbackAids.speed}>
      {m.playbackAids.speedValue(playbackAids.speed)}
    </span>
  {/if}
  {#if sleepRemaining !== null}
    <span class="remaining" aria-hidden="true">{formatDuration(sleepRemaining, '0:00')}</span>
  {/if}
</div>

<style>
  @reference "../../app.css";

  .aids-menu {
    @apply relative flex items-center gap-1;
  }

  /* プレーヤーのほかのボタン（`Player.svelte`の`.control-button.small`）と同じ見た目にする */
  .aids-button {
    @apply flex items-center justify-center p-1.5 bg-transparent border-none rounded-full text-text-secondary cursor-pointer transition-all duration-200;
  }

  .aids-button:hover {
    @apply text-text-primary bg-surface-active;
  }

  .aids-button:active {
    @apply scale-95;
  }

  .aids-button.active {
    @apply text-secondary;
  }

  /* 変えている再生速度・スリープタイマーの残り時間（ボタンの横に出す） */
  .remaining {
    @apply text-[0.7rem] text-primary tabular-nums;
  }

  .group {
    @apply mt-4 pt-4 border-t border-border;
  }

  .group-header {
    @apply flex items-baseline justify-between gap-2 mb-2;
  }

  .group-title {
    @apply m-0 text-sm font-semibold text-text-secondary;
  }

  .status {
    @apply text-xs text-primary tabular-nums;
  }

  .check {
    @apply flex items-center gap-2 text-sm text-text-secondary cursor-pointer;
  }

  /* 選択肢のボタン（再生速度・スリープタイマーの時間） */
  .options {
    @apply grid grid-cols-3 gap-1.5 mb-3;
  }

  .option-button {
    @apply py-1.5 bg-base-400 border border-border rounded text-xs text-text-secondary cursor-pointer transition-colors;
  }

  .option-button:hover {
    @apply text-text-primary border-border-light;
  }

  .option-button.selected {
    @apply border-primary text-primary;
  }

  /* 再生速度のスライダー（`CardSizeSlider`と同じ見た目） */
  .speed-slider {
    @apply block w-full h-1 rounded-sm cursor-pointer bg-surface-active;
    -webkit-appearance: none;
    appearance: none;
  }

  .speed-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    @apply w-3 h-3 rounded-full bg-primary cursor-pointer transition-transform duration-150;
  }

  .speed-slider::-webkit-slider-thumb:hover {
    @apply scale-125;
  }

  .speed-slider::-moz-range-thumb {
    @apply w-3 h-3 rounded-full bg-primary cursor-pointer border-none transition-transform duration-150;
  }

  .speed-slider::-moz-range-thumb:hover {
    @apply scale-125;
  }

  .speed-hint {
    @apply mt-3 mb-0;
  }

  .cancel {
    @apply mt-3 w-full text-xs;
  }

  .hint {
    @apply mt-1 mb-3 text-xs text-text-muted;
  }

  .source {
    @apply flex flex-col gap-1;
  }

  .source-label {
    @apply text-xs text-text-muted;
  }
</style>
