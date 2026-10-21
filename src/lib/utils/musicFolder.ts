/**
 * 音楽フォルダ（新しい曲を置く宛先）を選んだ結果の通知文
 *
 * ライブラリが空の画面（メインウィンドウ）と、設定の「ライブラリ」で共通に使う。
 */
import { m } from '#lib/i18n/i18n.svelte.js';
import type { MusicFolderChange } from '#lib/types/models.js';

/** 知らせる内容（`success`は`showSuccess`、`warning`は`showWarning`で出す） */
export interface MusicFolderNotice {
  kind: 'success' | 'warning';
  message: string;
}

/**
 * 音楽フォルダを選んだ結果を、知らせる文にする
 *
 * - 音楽フォルダにした: 読み込んだ曲があれば、その数も入れる（読めなかったファイルは、呼び出し側が
 *   インポートの結果として知らせる）
 * - ライブラリフォルダの中のフォルダを選んだ: 外側のフォルダを案内する
 */
export function musicFolderNotice(change: MusicFolderChange): MusicFolderNotice {
  if (change.kind === 'insideLibraryFolder') {
    return {
      kind: 'warning',
      message: m.musicFolder.insideLibraryFolder(change.path, change.libraryFolderPath)
    };
  }
  const imported = change.imported?.importedCount ?? 0;
  return {
    kind: 'success',
    message:
      imported > 0
        ? m.musicFolder.setWithTracks(change.path, imported)
        : m.musicFolder.set(change.path)
  };
}
