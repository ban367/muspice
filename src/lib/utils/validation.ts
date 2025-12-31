/**
 * 入力バリデーションとサニタイゼーションユーティリティ
 */

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
    /\.\./,  // 親ディレクトリへの移動
    /~\//,   // ホームディレクトリ
    /\/\//,  // 二重スラッシュ
  ];
  
  return !dangerousPatterns.some(pattern => pattern.test(path));
}

/**
 * 年をバリデーション
 */
export function validateYear(year: number | null | undefined): { valid: boolean; error?: string } {
  if (year === null || year === undefined) {
    return { valid: true };
  }
  
  if (typeof year !== 'number' || isNaN(year)) {
    return { valid: false, error: '年は数値で入力してください' };
  }
  
  if (year < 1000 || year > 9999) {
    return { valid: false, error: '年は1000から9999の範囲で指定してください' };
  }
  
  return { valid: true };
}

/**
 * トラック番号をバリデーション
 */
export function validateTrackNumber(trackNumber: number | null | undefined): { valid: boolean; error?: string } {
  if (trackNumber === null || trackNumber === undefined) {
    return { valid: true };
  }
  
  if (typeof trackNumber !== 'number' || isNaN(trackNumber)) {
    return { valid: false, error: 'トラック番号は数値で入力してください' };
  }
  
  if (trackNumber < 1 || trackNumber > 999) {
    return { valid: false, error: 'トラック番号は1から999の範囲で指定してください' };
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
      error: `${fieldName}は${maxLength}文字以内で入力してください（現在: ${value.length}文字）` 
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
    .replace(/[;'"\\]/g, '')  // セミコロン、クォート、バックスラッシュを除去
    .replace(/--/g, '')        // SQLコメントを除去
    .replace(/\/\*/g, '')      // マルチラインコメント開始を除去
    .replace(/\*\//g, '')      // マルチラインコメント終了を除去
    .trim();
}

/**
 * プレイリスト名をバリデーション
 */
export function validatePlaylistName(name: string): { valid: boolean; error?: string } {
  if (!name || name.trim().length === 0) {
    return { valid: false, error: 'プレイリスト名を入力してください' };
  }
  
  if (name.length > 100) {
    return { valid: false, error: 'プレイリスト名は100文字以内で入力してください' };
  }
  
  // 危険な文字をチェック
  const dangerousChars = /[<>:"/\\|?*]/;
  if (dangerousChars.test(name)) {
    return { valid: false, error: 'プレイリスト名に使用できない文字が含まれています' };
  }
  
  return { valid: true };
}

/**
 * 複数のバリデーション結果を統合
 */
export function combineValidationResults(
  results: Array<{ valid: boolean; error?: string }>
): { valid: boolean; errors: string[] } {
  const errors = results
    .filter(r => !r.valid && r.error)
    .map(r => r.error!);
  
  return {
    valid: errors.length === 0,
    errors
  };
}

/**
 * 入力値を安全な整数に変換
 */
export function toSafeInteger(value: any, defaultValue: number = 0): number {
  const parsed = parseInt(value, 10);
  
  if (isNaN(parsed) || !isFinite(parsed)) {
    return defaultValue;
  }
  
  return parsed;
}

/**
 * 入力値を安全な文字列に変換
 */
export function toSafeString(value: any, maxLength: number = 255): string {
  if (value === null || value === undefined) {
    return '';
  }
  
  const str = String(value);
  return str.slice(0, maxLength);
}
