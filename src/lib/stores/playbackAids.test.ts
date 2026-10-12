import { beforeEach, describe, expect, it } from 'vitest';
import {
  MAX_PLAYBACK_SPEED,
  MIN_PLAYBACK_SPEED,
  PLAYBACK_SPEED_PRESETS,
  playbackAids
} from './playbackAids.svelte.js';

const NOW = 1_000_000;

beforeEach(() => {
  playbackAids.reset();
});

describe('playbackAids', () => {
  it('最初は、どの指示も無効', () => {
    expect(playbackAids.speed).toBe(1);
    expect(playbackAids.stopAfterCurrent).toBe(false);
    expect(playbackAids.sleepTimer).toBeNull();
    expect(playbackAids.isFadingOut).toBe(false);
    expect(playbackAids.sleepRemainingSeconds(NOW)).toBeNull();
  });

  it('再生速度は、範囲の中に収め、刻みに合わせる', () => {
    playbackAids.setSpeed(1.25);
    expect(playbackAids.speed).toBe(1.25);

    // 刻み（0.05）に丸める（丸めた値に、2進数の誤差を残さない）
    playbackAids.setSpeed(1.234);
    expect(playbackAids.speed).toBe(1.25);
    playbackAids.setSpeed(0.7);
    expect(playbackAids.speed).toBe(0.7);
    playbackAids.setSpeed(1.15);
    expect(playbackAids.speed).toBe(1.15);

    // 範囲の外の値は、範囲の端にする
    playbackAids.setSpeed(9);
    expect(playbackAids.speed).toBe(MAX_PLAYBACK_SPEED);
    playbackAids.setSpeed(0);
    expect(playbackAids.speed).toBe(MIN_PLAYBACK_SPEED);

    // 数値でない値では変えない
    playbackAids.setSpeed(Number.NaN);
    expect(playbackAids.speed).toBe(MIN_PLAYBACK_SPEED);
  });

  it('すぐに選べる速度は、どれも刻みに合っていて、1倍を含む', () => {
    expect(PLAYBACK_SPEED_PRESETS).toContain(1);
    for (const speed of PLAYBACK_SPEED_PRESETS) {
      playbackAids.setSpeed(speed);
      expect(playbackAids.speed).toBe(speed);
    }
  });

  it('リセットすると、再生速度は1倍に戻る', () => {
    playbackAids.setSpeed(1.5);
    playbackAids.reset();
    expect(playbackAids.speed).toBe(1);
  });

  it('スリープタイマーを設定すると、止める時刻と残りの時間が決まる', () => {
    playbackAids.startSleepTimer(30, true, NOW);

    expect(playbackAids.sleepTimer).toEqual({
      endsAt: NOW + 30 * 60_000,
      minutes: 30,
      waitForTrackEnd: true
    });
    expect(playbackAids.sleepRemainingSeconds(NOW)).toBe(1800);
    expect(playbackAids.sleepRemainingSeconds(NOW + 500)).toBe(1800);
    expect(playbackAids.sleepRemainingSeconds(NOW + 1000)).toBe(1799);
    // 時間が過ぎていれば0
    expect(playbackAids.sleepRemainingSeconds(NOW + 31 * 60_000)).toBe(0);
  });

  it('設定し直すと、時間を数え直す', () => {
    playbackAids.startSleepTimer(30, false, NOW);
    playbackAids.startSleepTimer(15, false, NOW + 60_000);

    expect(playbackAids.sleepTimer?.minutes).toBe(15);
    expect(playbackAids.sleepRemainingSeconds(NOW + 60_000)).toBe(900);
  });

  it('時間として正しくない値では、設定しない', () => {
    playbackAids.startSleepTimer(0, false, NOW);
    playbackAids.startSleepTimer(-5, false, NOW);
    playbackAids.startSleepTimer(Number.NaN, false, NOW);

    expect(playbackAids.sleepTimer).toBeNull();
  });

  it('「曲の終わりまで再生する」は、時間を数え直さずに切り替える', () => {
    playbackAids.setSleepWaitForTrackEnd(true);
    expect(playbackAids.sleepTimer).toBeNull();

    playbackAids.startSleepTimer(30, false, NOW);
    playbackAids.setSleepWaitForTrackEnd(true);

    expect(playbackAids.sleepTimer).toEqual({
      endsAt: NOW + 30 * 60_000,
      minutes: 30,
      waitForTrackEnd: true
    });
  });

  it('解除すると、音量を下げている途中の印も消す', () => {
    playbackAids.startSleepTimer(30, false, NOW);
    playbackAids.isFadingOut = true;
    playbackAids.stopAfterCurrent = true;

    playbackAids.cancelSleepTimer();
    expect(playbackAids.sleepTimer).toBeNull();
    expect(playbackAids.isFadingOut).toBe(false);
    // 「この曲が終わったら停止」は、別の指示のため残る
    expect(playbackAids.stopAfterCurrent).toBe(true);

    playbackAids.reset();
    expect(playbackAids.stopAfterCurrent).toBe(false);
  });
});
