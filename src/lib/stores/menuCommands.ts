/**
 * メニューバーの、メインウィンドウで行う操作（Rustの`MenuCommand`。ADR-109）
 *
 * Rust側のメニュー（`menu.rs`）が送る操作を、メインウィンドウの画面で行う。メニューのキー（⌘N・⌘Fなど）は、
 * 入力欄にフォーカスがあってもメニューが受け取るため、ダイアログ（`<dialog>`）を開いている間・
 * 曲の情報の編集画面を開いている間は何もしない。
 */

import { tick } from 'svelte';
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { commands, events, type MenuCommand } from '#lib/bindings.js';
import { withErrorToast } from '#lib/queries/shared.js';
import { openTrackEditor } from '#lib/queries/trackEditor.js';
import { miniPlayer } from '#lib/stores/miniPlayer.svelte.js';
import { smartPlaylistDialog } from '#lib/stores/smartPlaylist.svelte.js';
import { ui } from '#lib/stores/ui.svelte.js';
import { reportError } from '#lib/utils/errorLog.js';
import { artistsOf, goToAlbum, goToArtist } from '#lib/utils/libraryReveal.js';
import { stopListening } from '#lib/utils/listen.js';
import { m } from '#lib/i18n/i18n.svelte.js';

/** 画面の部品が受け持つ操作（部品を表示している間だけ、`provideMenuCommand`で登録する） */
type ProvidedCommand = Extract<MenuCommand['type'], 'newPlaylist' | 'reloadLibrary'>;

const providers = new Map<ProvidedCommand, () => unknown>();

/** 一覧の検索欄に付ける印（`LibraryHeader`の入力欄） */
export const LIBRARY_SEARCH_ATTRIBUTE = 'data-library-search';

/**
 * 操作を受け持つ部品を登録する（新規プレイリストはサイドバーのプレイリストの一覧、ライブラリの再読み込みは
 * メインウィンドウのレイアウト（曲のキャッシュを持つ））
 *
 * @returns 登録を外す関数（部品を閉じる時に呼ぶ）
 */
export function provideMenuCommand(type: ProvidedCommand, run: () => unknown): () => void {
  providers.set(type, run);
  return () => {
    if (providers.get(type) === run) providers.delete(type);
  };
}

/** メニューの操作の受け取りを始める（メインウィンドウのレイアウトで1回呼ぶ。戻り値で終える） */
export function watchMenuCommands(): () => void {
  const listening = events.menuCommand.listen(({ payload }) => {
    runMenuCommand(payload).catch((error) => reportError(error, 'Menu command failed'));
  });
  return () => stopListening(listening);
}

/** ダイアログ・編集画面を開いていて、メニューの操作を受け付けない時 */
function isBlocked(): boolean {
  return ui.isTrackEditorOpen || document.querySelector('dialog[open]') !== null;
}

/** メニューの操作を行う */
export async function runMenuCommand(command: MenuCommand): Promise<void> {
  if (isBlocked()) return;
  switch (command.type) {
    case 'newPlaylist':
      await showNormalView();
      await providers.get('newPlaylist')?.();
      break;
    case 'newSmartPlaylist':
      await showNormalView();
      smartPlaylistDialog.openNew();
      break;
    case 'find':
      await showNormalView();
      await focusLibrarySearch();
      break;
    case 'editTrackInfo':
      await editSelectedTracks();
      break;
    case 'showInFinder':
      await showSelectedTrackInFinder();
      break;
    case 'goToAlbum':
    case 'goToArtist':
      await goToSelectedTrackGroup(command.type);
      break;
    case 'reloadLibrary':
      await providers.get('reloadLibrary')?.();
      break;
  }
}

/** ミニプレーヤーの間は、通常の表示に戻す（入力欄・ダイアログを、小さな表示に出さない） */
async function showNormalView(): Promise<void> {
  if (miniPlayer.isActive) await miniPlayer.exit();
}

/**
 * 一覧で選んでいる曲（`followSelectedTrack`で一覧が出している曲。選んでいなければ空）
 *
 * ミニプレーヤーの間は、選んでいる曲が見えないため、空にする。
 */
function selectedTracks() {
  const selected = ui.selectedTrack;
  if (miniPlayer.isActive || selected === null || selected.count === 0) return [];
  return selected.tracks ?? [selected.track];
}

/** 「曲の情報を編集...」: 選んでいる曲（一覧の順）の編集画面を開く */
async function editSelectedTracks(): Promise<void> {
  const tracks = selectedTracks();
  if (tracks.length === 0) return;
  // 1曲の編集では、一覧の並びも渡す（編集画面から前後の曲へ移れる。ADR-119）
  const sequence = ui.selectedTrack?.listTracks?.map((track) => track.id) ?? [];
  await openTrackEditor(
    tracks.map((track) => track.id),
    sequence
  ).catch(() => {
    // トーストで知らせ済み
  });
}

/** 「ファイルの場所を開く」: 最後に選んだ曲のファイルを、Finderで表示する */
async function showSelectedTrackInFinder(): Promise<void> {
  if (selectedTracks().length === 0) return;
  const track = ui.selectedTrack?.track;
  if (!track) return;
  await withErrorToast(m.operations.showInFolder, () => commands.showInFolder(track.id)).catch(
    () => {
      // トーストで知らせ済み
    }
  );
}

/**
 * 「アルバムへ移動」・「アーティストへ移動」: 最後に選んだ曲のアルバム・アーティストの画面を開く
 * （複数のアーティストの曲は、最初のアーティスト。ほかは、右クリックのメニューから選ぶ。ADR-110）
 */
async function goToSelectedTrackGroup(type: 'goToAlbum' | 'goToArtist'): Promise<void> {
  if (selectedTracks().length === 0) return;
  const track = ui.selectedTrack?.track;
  if (!track) return;
  if (type === 'goToAlbum') {
    await goToAlbum(track);
    return;
  }
  const artist = artistsOf(track)[0];
  if (artist !== undefined) await goToArtist(artist);
}

/**
 * 「検索」: 表示中の画面の検索欄へ移る（検索欄のない画面では、曲の画面を開いてから）
 *
 * 検索欄の文字は選んだ状態にして、そのまま打ち直せるようにする。
 */
export async function focusLibrarySearch(): Promise<void> {
  // Now Playingの画面を開いていたら、検索欄が見えるように閉じる
  ui.isNowPlayingOpen = false;
  let input = findLibrarySearch();
  if (input === null) {
    await goto(resolve('library/songs'));
    await tick();
    input = findLibrarySearch();
  }
  input?.focus();
  input?.select();
}

function findLibrarySearch(): HTMLInputElement | null {
  return document.querySelector<HTMLInputElement>(`input[${LIBRARY_SEARCH_ATTRIBUTE}]`);
}
