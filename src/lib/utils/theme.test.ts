import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { applyTheme, resolveTheme, restoreTheme } from './theme';

describe('resolveTheme', () => {
  it('「OSの設定に従う」はOSの配色で決め、それ以外はそのまま使う', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
    expect(resolveTheme('light', true)).toBe('light');
  });
});

describe('applyTheme', () => {
  /** テスト用のメディアクエリ（OSの配色の変化を再現する） */
  class FakeMediaQuery extends EventTarget {
    matches = true;

    setDark(dark: boolean) {
      this.matches = dark;
      this.dispatchEvent(new Event('change'));
    }
  }

  let media: FakeMediaQuery;
  let root: { dataset: Record<string, string> };
  let storage: Map<string, string>;

  beforeEach(() => {
    media = new FakeMediaQuery();
    root = { dataset: {} };
    storage = new Map();
    vi.stubGlobal('window', { matchMedia: () => media });
    vi.stubGlobal('document', { documentElement: root });
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value)
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('テーマを<html data-theme>に反映する', () => {
    applyTheme('light');
    expect(root.dataset.theme).toBe('light');

    // ダーク・ライトを選んだ場合は、OSの配色が変わっても変えない
    media.setDark(false);
    applyTheme('dark');
    media.setDark(true);
    media.setDark(false);
    expect(root.dataset.theme).toBe('dark');
  });

  it('「OSの設定に従う」の間はOSの配色の変化に追従し、止めた後は追従しない', () => {
    const stop = applyTheme('system');
    expect(root.dataset.theme).toBe('dark');

    media.setDark(false);
    expect(root.dataset.theme).toBe('light');

    stop();
    media.setDark(true);
    expect(root.dataset.theme).toBe('light');
  });

  it('次の起動で、設定を読み込む前に前回のテーマを反映できる', () => {
    applyTheme('system');
    root.dataset = {};

    media.matches = false;
    restoreTheme();
    expect(root.dataset.theme).toBe('light');
  });

  it('保存されたテーマが不正なら何もしない', () => {
    storage.set('muspice:theme', 'sepia');
    restoreTheme();
    expect(root.dataset.theme).toBeUndefined();
  });
});
