import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { applyLanguage, i18n, m, restoreLanguage } from './i18n.svelte';
import { en } from './messages/en';
import { ja } from './messages/ja';
import { formatDateTime, formatTotalDuration } from '#lib/utils/format.js';

/** メッセージのキーの一覧（入れ子は`.`でつなぐ） */
function keysOf(value: object, prefix = ''): string[] {
  return Object.entries(value).flatMap(([key, child]) =>
    typeof child === 'object' && child !== null
      ? keysOf(child, `${prefix}${key}.`)
      : [`${prefix}${key}`]
  );
}

describe('メッセージ', () => {
  it('日本語と英語で同じキーを持つ', () => {
    // エラーコードごとの汎用メッセージは、言語によって含めるコードが違う（下のテスト）
    const keys = (messages: object) =>
      keysOf(messages)
        .filter((key) => !key.startsWith('errors.byCode.'))
        .sort();
    expect(keys(en)).toEqual(keys(ja));
  });

  it('英語では数に合わせて単数形・複数形を選ぶ', () => {
    expect(en.common.trackCount(1)).toBe('1 track');
    expect(en.common.trackCount(3)).toBe('3 tracks');
    expect(ja.common.trackCount(3)).toBe('3曲');
  });

  it('英語では、日本語でバックエンドの文言を使うエラーも汎用メッセージにする', () => {
    expect(ja.errors.byCode.NOT_FOUND).toBeUndefined();
    expect(en.errors.byCode.NOT_FOUND).toBeDefined();
    expect(en.errors.byCode.VALIDATION).toBeDefined();
  });
});

describe('言語の切り替え', () => {
  let root: { lang: string };
  let storage: Map<string, string>;

  beforeEach(() => {
    root = { lang: 'ja' };
    storage = new Map();
    vi.stubGlobal('document', { documentElement: root });
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value)
    });
  });

  afterEach(() => {
    i18n.language = 'ja';
    vi.unstubAllGlobals();
  });

  it('切り替えると`m`が今の言語のメッセージを返し、<html lang>も合わせる', () => {
    expect(m.common.cancel).toBe('キャンセル');

    applyLanguage('en');
    expect(m.common.cancel).toBe('Cancel');
    expect(m.common.trackCount(2)).toBe('2 tracks');
    expect(root.lang).toBe('en');
  });

  it('`m`を読む$derivedは、言語の切り替えに追従する', () => {
    const cleanup = $effect.root(() => {
      const label = $derived(m.player.noTrack);
      const values: string[] = [];
      $effect(() => {
        values.push(label);
      });
      flushSync();

      applyLanguage('en');
      flushSync();
      expect(values).toEqual(['トラックを選択して再生', 'Select a track to play']);
    });
    cleanup();
  });

  it('日付と時間の書式も言語に合わせる', () => {
    applyLanguage('en');
    expect(formatTotalDuration(3900)).toBe('1 hr 5 min');
    expect(formatDateTime('2026-10-03T05:07:00Z')).toMatch(/10\/03\/2026/);

    applyLanguage('ja');
    expect(formatTotalDuration(3900)).toBe('1時間5分');
    expect(formatDateTime('2026-10-03T05:07:00Z')).toMatch(/2026\/10\/03/);
  });

  it('次の起動で、設定を読み込む前に前回の言語を反映できる', () => {
    applyLanguage('en');
    i18n.language = 'ja';

    restoreLanguage();
    expect(i18n.language).toBe('en');
  });

  it('保存された言語が不正なら何もしない', () => {
    storage.set('muspice:language', 'fr');
    restoreLanguage();
    expect(i18n.language).toBe('ja');
  });
});
