import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { followSelectedTrack, ui, type SelectedTrack } from './ui.svelte.js';

/** テスト用の曲（`title`を変えると、タグを編集して取り直した曲） */
function track(id: string, title = `Song ${id}`): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title,
    artist: 'Artist',
    album: 'Album',
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: 1,
    discNumber: null,
    duration: 200,
    fileSize: 1,
    format: 'mp3',
    bitrate: null,
    sampleRate: null,
    isFavorite: false,
    rating: 0,
    playCount: 0,
    skipCount: 0,
    lastPlayedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
    sortTags: { title: null, artist: null, album: null, albumArtist: null },
    isMissing: false
  };
}

/** 曲を選べる一覧（`followSelectedTrack`を呼んだコンポーネントの代わり） */
interface List {
  /** 選択を変える（曲を取り直した時も、新しい曲で呼ぶ） */
  select(value: SelectedTrack | null): void;
  /** 一覧をなくす（別の画面へ移った時） */
  destroy(): void;
}

const lists: List[] = [];

function createList(): List {
  let selected = $state.raw<SelectedTrack | null>(null);
  const destroy = $effect.root(() => {
    followSelectedTrack(() => selected);
  });
  flushSync();
  const list = {
    select(value: SelectedTrack | null) {
      selected = value;
      flushSync();
    },
    destroy
  };
  lists.push(list);
  return list;
}

describe('followSelectedTrack', () => {
  beforeEach(() => {
    ui.selectedTrack = null;
  });

  afterEach(() => {
    for (const list of lists.splice(0)) list.destroy();
  });

  it('選んでいる曲と数を出し、選択が空になったら消す', () => {
    const list = createList();
    expect(ui.selectedTrack).toBeNull();

    list.select({ track: track('t1'), count: 1 });
    expect(ui.selectedTrack).toEqual({ track: track('t1'), count: 1 });

    list.select({ track: track('t2'), count: 3 });
    expect(ui.selectedTrack).toEqual({ track: track('t2'), count: 3 });

    list.select(null);
    expect(ui.selectedTrack).toBeNull();
  });

  it('最後に選択を変えた一覧の曲を出す（ほかの一覧は、曲を取り直しただけでは上書きしない）', () => {
    const first = createList();
    const second = createList();

    first.select({ track: track('t1'), count: 1 });
    second.select({ track: track('t2'), count: 1 });
    expect(ui.selectedTrack?.track.id).toBe('t2');

    // 前の一覧の曲を取り直しただけ（選択は同じ）では、入れ替えない
    first.select({ track: track('t1', 'Edited'), count: 1 });
    expect(ui.selectedTrack?.track.id).toBe('t2');

    // 前の一覧の選択が空になっても、ほかの一覧が出している曲は消さない
    first.select(null);
    expect(ui.selectedTrack?.track.id).toBe('t2');

    // 前の一覧で選び直すと、その曲に入れ替える
    first.select({ track: track('t3'), count: 1 });
    expect(ui.selectedTrack?.track.id).toBe('t3');
  });

  it('出している一覧の曲を取り直した時は、新しい内容に入れ替える', () => {
    const list = createList();
    list.select({ track: track('t1'), count: 1 });

    list.select({ track: track('t1', 'Edited'), count: 1 });
    expect(ui.selectedTrack?.track.title).toBe('Edited');
  });

  it('出している一覧がなくなったら消す（ほかの一覧が出している時は消さない）', () => {
    const first = createList();
    const second = createList();

    first.select({ track: track('t1'), count: 1 });
    second.destroy();
    expect(ui.selectedTrack?.track.id).toBe('t1');

    first.destroy();
    expect(ui.selectedTrack).toBeNull();
  });
});
