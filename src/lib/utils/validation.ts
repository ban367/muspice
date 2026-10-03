/**
 * 入力バリデーションとサニタイゼーションユーティリティ
 *
 * エラーの文言は表示の言語に合わせる。
 */
import { m } from '#lib/i18n/i18n.svelte.js';

/**
 * 文字列をサニタイズ（XSS対策）
 */
export function sanitizeString(input: string): string {
  if (!input) return '';

  // HTMLエンティティをエスケープ
  return input
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#x27;')
    .replace(/\//g, '&#x2F;');
}

/**
 * ファイルパスをバリデーション（パストラバーサル攻撃対策）
 */
export function validateFilePath(path: string): boolean {
  if (!path) return false;

  // パストラバーサルパターンを検出
  const dangerousPatterns = [
    /\.\./, // 親ディレクトリへの移動
    /~\//, // ホームディレクトリ
    /\/\// // 二重スラッシュ
  ];

  return !dangerousPatterns.some((pattern) => pattern.test(path));
}

/**
 * 年をバリデーション
 */
export function validateYear(year: number | null | undefined): { valid: boolean; error?: string } {
  if (year === null || year === undefined) {
    return { valid: true };
  }

  if (typeof year !== 'number' || isNaN(year)) {
    return { valid: false, error: m.validation.yearNotNumber };
  }

  if (year < 1000 || year > 9999) {
    return { valid: false, error: m.validation.yearOutOfRange };
  }

  return { valid: true };
}

/**
 * トラック番号をバリデーション
 */
export function validateTrackNumber(trackNumber: number | null | undefined): {
  valid: boolean;
  error?: string;
} {
  if (trackNumber === null || trackNumber === undefined) {
    return { valid: true };
  }

  if (typeof trackNumber !== 'number' || isNaN(trackNumber)) {
    return { valid: false, error: m.validation.trackNumberNotNumber };
  }

  if (trackNumber < 1 || trackNumber > 999) {
    return { valid: false, error: m.validation.trackNumberOutOfRange };
  }

  return { valid: true };
}

/**
 * メタデータフィールドの長さをバリデーション
 */
export function validateFieldLength(
  value: string | null | undefined,
  fieldName: string,
  maxLength: number = 255
): { valid: boolean; error?: string } {
  if (!value) {
    return { valid: true };
  }

  if (value.length > maxLength) {
    return {
      valid: false,
      error: m.validation.tooLong(fieldName, maxLength, value.length)
    };
  }

  return { valid: true };
}

/**
 * 検索クエリをサニタイズ（SQLインジェクション対策）
 */
export function sanitizeSearchQuery(query: string): string {
  if (!query) return '';

  // 危険な文字を除去
  return query
    .replace(/[;'"\\]/g, '') // セミコロン、クォート、バックスラッシュを除去
    .replace(/--/g, '') // SQLコメントを除去
    .replace(/\/\*/g, '') // マルチラインコメント開始を除去
    .replace(/\*\//g, '') // マルチラインコメント終了を除去
    .trim();
}

/**
 * プレイリスト名をバリデーション
 */
export function validatePlaylistName(name: string): { valid: boolean; error?: string } {
  if (!name || name.trim().length === 0) {
    return { valid: false, error: m.validation.playlistNameRequired };
  }

  if (name.length > 100) {
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
 * 複数のバリデーション結果を統合
 */
export function combineValidationResults(results: Array<{ valid: boolean; error?: string }>): {
  valid: boolean;
  errors: string[];
} {
  const errors = results.filter((r) => !r.valid && r.error).map((r) => r.error!);

  return {
    valid: errors.length === 0,
    errors
  };
}

/**
 * 入力値を安全な整数に変換
 */
export function toSafeInteger(value: unknown, defaultValue: number = 0): number {
  const parsed = parseInt(String(value), 10);

  if (isNaN(parsed) || !isFinite(parsed)) {
    return defaultValue;
  }

  return parsed;
}

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
