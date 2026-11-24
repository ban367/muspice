import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MenuCommand } from '#lib/bindings.js';
import type { Track } from '#lib/types/models.js';

/** メニューの操作を聞いている関数 */
const listeners = new Set<(event: { payload: MenuCommand }) => void>();

vi.mock('#lib/bindings.js', () => ({
  commands: {
    showInFolder: vi.fn(async () => {}),
    logFrontendMessage: vi.fn(async () => {})
  },
  events: {
    menuCommand: {
      listen: vi.fn(async (listener: (event: { payload: MenuCommand }) => void) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      })
    }
  }
}));

vi.mock('#lib/queries/trackEditor.js', () => ({
  openTrackEditor: vi.fn(async () => {})
}));

vi.mock('#lib/stores/miniPlayer.svelte.js', () => ({
  miniPlayer: {
    isActive: false,
    exit: vi.fn(async function (this: { isActive: boolean }) {
      this.isActive = false;
    })
  }
}));

vi.mock('#lib/utils/libraryReveal.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/utils/libraryReveal.js')>()),
  goToAlbum: vi.fn(async () => {}),
  goToArtist: vi.fn(async () => {})
}));

vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}) }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => `/${path}` }));

import { goto } from '$app/navigation';
import { commands } from '#lib/bindings.js';
import { openTrackEditor } from '#lib/queries/trackEditor.js';
import { goToAlbum, goToArtist } from '#lib/utils/libraryReveal.js';
import { miniPlayer } from '#lib/stores/miniPlayer.svelte.js';
import { smartPlaylistDialog } from '#lib/stores/smartPlaylist.svelte.js';
import { ui } from '#lib/stores/ui.svelte.js';
import { provideMenuCommand, runMenuCommand, watchMenuCommands } from './menuCommands.js';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function track(id: string): Track {
  return { id, title: id } as Track;
}

/** 開いているダイアログ・一覧の検索欄（`document.querySelector`が返すもの） */
let openDialog: object | null;
let searchInput: { focus: () => void; select: () => void } | null;

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  openDialog = null;
  searchInput = null;
  vi.stubGlobal('document', {
    querySelector: (selector: string) => {
      if (selector === 'dialog[open]') return openDialog;
      if (selector === 'input[data-library-search]') return searchInput;
      return null;
    }
  });
  ui.isTrackEditorOpen = false;
  ui.isNowPlayingOpen = false;
  ui.selectedTrack = null;
  smartPlaylistDialog.close();
  (miniPlayer as { isActive: boolean }).isActive = false;
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('runMenuCommand', () => {
  it('新規プレイリストは、登録した部品が受け持ち、登録を外すと何もしない', async () => {
    const create = vi.fn();
    const release = provideMenuCommand('newPlaylist', create);
    await runMenuCommand({ type: 'newPlaylist' });
    expect(create).toHaveBeenCalledTimes(1);

    release();
    await runMenuCommand({ type: 'newPlaylist' });
    expect(create).toHaveBeenCalledTimes(1);
  });

  it('あとから登録した部品の登録は、前の部品の登録を外しても残る', async () => {
    const first = vi.fn();
    const second = vi.fn();
    const releaseFirst = provideMenuCommand('newPlaylist', first);
    const releaseSecond = provideMenuCommand('newPlaylist', second);
    releaseFirst();
    await runMenuCommand({ type: 'newPlaylist' });
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
    releaseSecond();
  });

  it('新規オートプレイリストは、オートプレイリストの編集画面を新しく開く', async () => {
    await runMenuCommand({ type: 'newSmartPlaylist' });
    expect(smartPlaylistDialog.target).toEqual({ smartPlaylist: null });
  });

  it('ダイアログ・曲の情報の編集画面を開いている間は、何もしない', async () => {
    const create = vi.fn();
    const release = provideMenuCommand('newPlaylist', create);

    openDialog = {};
    await runMenuCommand({ type: 'newPlaylist' });
    await runMenuCommand({ type: 'newSmartPlaylist' });
    openDialog = null;
    ui.isTrackEditorOpen = true;
    ui.selectedTrack = { track: track('t1'), count: 1, tracks: [track('t1')] };
    await runMenuCommand({ type: 'editTrackInfo' });

    expect(create).not.toHaveBeenCalled();
    expect(smartPlaylistDialog.target).toBeNull();
    expect(openTrackEditor).not.toHaveBeenCalled();
    release();
  });

  it('ミニプレーヤーの間は、通常の表示に戻してから新規プレイリストの入力を開く', async () => {
    (miniPlayer as { isActive: boolean }).isActive = true;
    const create = vi.fn(() => expect(miniPlayer.isActive).toBe(false));
    const release = provideMenuCommand('newPlaylist', create);
    await runMenuCommand({ type: 'newPlaylist' });
    expect(miniPlayer.exit).toHaveBeenCalledTimes(1);
    expect(create).toHaveBeenCalledTimes(1);
    release();
  });

  it('曲の情報の編集は、一覧で選んでいる曲を一覧の順で開く', async () => {
    const list = [track('t1'), track('t2'), track('t3')];
    ui.selectedTrack = {
      track: track('t3'),
      count: 2,
      tracks: [track('t1'), track('t3')],
      listTracks: list
    };
    await runMenuCommand({ type: 'editTrackInfo' });
    // 一覧の並びも渡す（1曲の編集の時だけ、編集画面で前後の曲へ移るのに使う。ADR-119）
    expect(openTrackEditor).toHaveBeenCalledWith(['t1', 't3'], ['t1', 't2', 't3']);
  });

  it('選んでいる曲がない・ミニプレーヤーの間は、編集画面もFinderも開かない', async () => {
    // 選択を外した後（最後に選んでいた曲は残る）
    ui.selectedTrack = { track: track('t1'), count: 0 };
    await runMenuCommand({ type: 'editTrackInfo' });
    await runMenuCommand({ type: 'showInFinder' });

    ui.selectedTrack = { track: track('t1'), count: 1, tracks: [track('t1')] };
    (miniPlayer as { isActive: boolean }).isActive = true;
    await runMenuCommand({ type: 'editTrackInfo' });
    await runMenuCommand({ type: 'showInFinder' });

    expect(openTrackEditor).not.toHaveBeenCalled();
    expect(commands.showInFolder).not.toHaveBeenCalled();
  });

  it('ファイルの場所を開くは、最後に選んだ曲のファイルを表示する', async () => {
    ui.selectedTrack = { track: track('t2'), count: 2, tracks: [track('t1'), track('t2')] };
    await runMenuCommand({ type: 'showInFinder' });
    expect(commands.showInFolder).toHaveBeenCalledWith('t2');
  });

  it('アルバム・アーティストへの移動は、最後に選んだ曲のアルバム・最初のアーティストへ移る', async () => {
    const last = { ...track('t2'), album: 'Album', albumArtist: null, artist: 'A; B' } as Track;
    ui.selectedTrack = { track: last, count: 2, tracks: [track('t1'), last] };
    await runMenuCommand({ type: 'goToAlbum' });
    await runMenuCommand({ type: 'goToArtist' });
    expect(goToAlbum).toHaveBeenCalledWith(last);
    expect(goToArtist).toHaveBeenCalledWith('A');

    // 選んでいない時は、移らない
    vi.mocked(goToAlbum).mockClear();
    ui.selectedTrack = { track: last, count: 0 };
    await runMenuCommand({ type: 'goToAlbum' });
    expect(goToAlbum).not.toHaveBeenCalled();
  });

  it('検索は、表示中の画面の検索欄へ移り、文字を選ぶ', async () => {
    const input = { focus: vi.fn(), select: vi.fn() };
    searchInput = input;
    ui.isNowPlayingOpen = true;
    await runMenuCommand({ type: 'find' });
    expect(goto).not.toHaveBeenCalled();
    expect(input.focus).toHaveBeenCalled();
    expect(input.select).toHaveBeenCalled();
    // 検索欄が見えるよう、Now Playingの画面を閉じる
    expect(ui.isNowPlayingOpen).toBe(false);
  });

  it('検索欄のない画面では、曲の画面を開いてから検索欄へ移る', async () => {
    const input = { focus: vi.fn(), select: vi.fn() };
    vi.mocked(goto).mockImplementationOnce(async () => {
      searchInput = input;
    });
    await runMenuCommand({ type: 'find' });
    expect(goto).toHaveBeenCalledWith('/library/songs');
    expect(input.focus).toHaveBeenCalled();
  });
});

describe('watchMenuCommands', () => {
  it('メニューから届いた操作を行い、終えた後は受け取らない', async () => {
    const create = vi.fn();
    const release = provideMenuCommand('newPlaylist', create);
    const stop = watchMenuCommands();
    await flush();

    for (const listener of listeners) listener({ payload: { type: 'newPlaylist' } });
    await flush();
    expect(create).toHaveBeenCalledTimes(1);

    stop();
    await flush();
    expect(listeners.size).toBe(0);
    release();
  });
});
