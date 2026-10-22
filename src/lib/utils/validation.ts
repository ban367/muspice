/**
 * 入力バリデーションユーティリティ
 *
 * エラーの文言は表示の言語に合わせる。
 */
import { m } from '#lib/i18n/i18n.svelte.js';

/**
 * プレイリスト名をバリデーション
 */
export function validatePlaylistName(name: string): { valid: boolean; error?: string } {
  if (!name || name.trim().length === 0) {
    return { valid: false, error: m.validation.playlistNameRequired };
  }

  if ([...name].length > 100) {
    return { valid: false, error: m.validation.playlistNameTooLong };
  }

  // 危険な文字をチェック
  const dangerousChars = /[<>:"/\\|?*]/;
  if (dangerousChars.test(name)) {
    return { valid: false, error: m.validation.playlistNameInvalid };
  }

  return { valid: true };
}

/**
 * プレイリストのフォルダの名前を検証する
 * @returns 誤りの説明（誤りがなければnull）
 */
export function validatePlaylistFolderName(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0) return m.validation.folderNameRequired;
  if ([...trimmed].length > 100) return m.validation.folderNameTooLong;
  return null;
}

/** プレイリストの説明の長さの上限（文字数。Rustの`MAX_PLAYLIST_DESCRIPTION_LENGTH`と同じ） */
export const MAX_PLAYLIST_DESCRIPTION_LENGTH = 1000;

/**
 * 入力値を安全な文字列に変換
 */
export function toSafeString(value: unknown, maxLength: number = 255): string {
  if (value === null || value === undefined) {
    return '';
  }

  const str = String(value);
  return str.slice(0, maxLength);
}
