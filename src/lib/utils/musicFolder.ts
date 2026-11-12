/**
 * 音楽フォルダ（新しい曲を置く宛先）を選んだ結果の通知文
 *
 * ライブラリが空の画面（メインウィンドウ）と、設定の「ライブラリ」で共通に使う。
 */
import { m } from '#lib/i18n/i18n.svelte.js';
import type { MusicFolderChange } from '#lib/types/models.js';

/** 音楽フォルダにした結果（置き換えの確認を求める結果を除く） */
export type MusicFolderSet = Extract<MusicFolderChange, { kind: 'set' }>;

/** ライブラリの置き換えの確認を求める結果（ADR-086） */
export type MusicFolderReplacement = Extract<MusicFolderChange, { kind: 'confirmReplace' }>;

/**
 * 音楽フォルダにした結果を、知らせる文にする（`showSuccess`で出す）
 *
 * - ライブラリを置き換えた: 外した曲・消したプレイリストの数を入れる（バックアップの場所は、
 *   呼び出し側が別に知らせる）
 * - それ以外: 読み込んだ曲があれば、その数も入れる（読めなかったファイルは、呼び出し側が
 *   インポートの結果として知らせる）
 */
export function musicFolderMessage(change: MusicFolderSet): string {
  if (change.replaced) {
    return m.musicFolder.replaced(
      change.path,
      change.replaced.removedTrackCount,
      change.replaced.removedPlaylistCount
    );
  }
  const imported = change.imported?.importedCount ?? 0;
  return imported > 0
    ? m.musicFolder.setWithTracks(change.path, imported)
    : m.musicFolder.set(change.path);
}

/**
 * 曲が音楽フォルダの外にあるか（音楽フォルダが未設定なら、外の曲とはしない。ADR-084）
 *
 * 印はDBに持たず、曲の場所から求める（ファイルの整理・場所の付け替えでずれない）。
 * パスの要素の単位で比べる（`/Music/A`は`/Music/AB/…`を含まない）。
 */
export function isOutsideMusicFolder(filePath: string, musicFolderPath: string | null): boolean {
  if (musicFolderPath === null) return false;
  const prefix = musicFolderPath.endsWith('/') ? musicFolderPath : `${musicFolderPath}/`;
  return !filePath.startsWith(prefix);
}
