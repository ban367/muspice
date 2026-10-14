import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { matchQueueEntries, toQueueEntries } from './playQueue.js';

function makeTrack(id: string): Track {
  return { id, title: id } as Track;
}

const [a, b, c] = ['a', 'b', 'c'].map(makeTrack);

describe('toQueueEntries', () => {
  it('曲ごとに要素を作り、同じ曲でも要素ごとに別のIDを振る', () => {
    const entries = [...toQueueEntries([a, a]), ...toQueueEntries([a, b])];

    expect(entries.map((entry) => entry.track)).toEqual([a, a, a, b]);
    expect(new Set(entries.map((entry) => entry.entryId)).size).toBe(4);
  });
});

describe('matchQueueEntries', () => {
  it('曲を、同じ曲の要素へ前から順に対応付ける', () => {
    const entries = toQueueEntries([a, b, a]);

    expect(matchQueueEntries([b, a, a], entries)).toEqual([entries[1], entries[0], entries[2]]);
  });

  it('対応する要素が残っていない曲は、新しい要素にする', () => {
    const entries = toQueueEntries([a]);

    const matched = matchQueueEntries([a, a, c], entries);

    expect(matched[0]).toBe(entries[0]);
    expect(matched.map((entry) => entry.track)).toEqual([a, a, c]);
    expect(new Set(matched.map((entry) => entry.entryId)).size).toBe(3);
  });
});
