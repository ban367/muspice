import { afterEach, describe, expect, it, vi } from 'vitest';
import type { OrganizeEntry, OrganizeStatus } from '#lib/types/models.js';
import {
  DEFAULT_FILE_ORGANIZE_TEMPLATE,
  FILE_ORGANIZE_PRESETS,
  loadOrganizeTemplate,
  saveOrganizeTemplate,
  summarizeOrganizePlan
} from './fileOrganize.js';
import { parseOrganizeTemplate } from '#lib/mocks/fileOrganize.js';

const entry = (trackId: string, status: OrganizeStatus): OrganizeEntry => ({
  trackId,
  currentPath: `in/${trackId}.mp3`,
  newPath: status === 'outsideLibrary' || status === 'missing' ? null : `out/${trackId}.mp3`,
  status
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('summarizeOrganizePlan', () => {
  it('動かす曲・動かさない曲・そのままの曲に分ける（順は変えない）', () => {
    const summary = summarizeOrganizePlan([
      entry('a', 'move'),
      entry('b', 'unchanged'),
      entry('c', 'targetExists'),
      entry('d', 'move'),
      entry('e', 'outsideLibrary'),
      entry('f', 'missing'),
      entry('g', 'sameTarget'),
      entry('h', 'unchanged')
    ]);

    expect(summary.moves.map((e) => e.trackId)).toEqual(['a', 'd']);
    expect(summary.blocked.map((e) => e.trackId)).toEqual(['c', 'e', 'f', 'g']);
    expect(summary.unchangedCount).toBe(2);
  });

  it('空の計画は、すべて空にする', () => {
    expect(summarizeOrganizePlan([])).toEqual({ moves: [], blocked: [], unchangedCount: 0 });
  });
});

describe('書式の候補', () => {
  it('どの候補も、書式として解釈できる', () => {
    for (const preset of FILE_ORGANIZE_PRESETS) {
      expect(() => parseOrganizeTemplate(preset), preset).not.toThrow();
    }
    expect(FILE_ORGANIZE_PRESETS).toContain(DEFAULT_FILE_ORGANIZE_TEMPLATE);
  });
});

describe('前回の書式', () => {
  function stubStorage(initial: Record<string, string> = {}) {
    const items = { ...initial };
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => items[key] ?? null,
      setItem: (key: string, value: string) => {
        items[key] = value;
      }
    });
    return items;
  }

  it('保存した書式を読み、なければ既定の書式にする', () => {
    stubStorage();
    expect(loadOrganizeTemplate()).toBe(DEFAULT_FILE_ORGANIZE_TEMPLATE);

    saveOrganizeTemplate('%artist% - %title%');
    expect(loadOrganizeTemplate()).toBe('%artist% - %title%');

    saveOrganizeTemplate('   ');
    expect(loadOrganizeTemplate()).toBe(DEFAULT_FILE_ORGANIZE_TEMPLATE);
  });

  it('保存先を使えなくても、既定の書式で動く', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('使えません');
      },
      setItem: () => {
        throw new Error('使えません');
      }
    });

    expect(() => saveOrganizeTemplate('%title%')).not.toThrow();
    expect(loadOrganizeTemplate()).toBe(DEFAULT_FILE_ORGANIZE_TEMPLATE);
  });
});
