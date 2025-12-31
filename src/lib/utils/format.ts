/**
 * フォーマットユーティリティ関数
 * 時間、再生時間などの表示形式を統一
 */

/**
 * 秒数を mm:ss 形式にフォーマット
 * @param seconds - 秒数（null/undefined/0の場合は '--:--' を返す）
 * @returns フォーマットされた文字列
 */
export function formatDuration(seconds: number | null): string {
  if (!seconds) return '--:--';
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, '0')}`;
}

/**
 * 秒数を「X時間Y分」または「Y分」形式にフォーマット
 * @param seconds - 秒数
 * @returns フォーマットされた文字列
 */
export function formatTotalDuration(seconds: number): string {
  if (!seconds) return '0分';
  const hours = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  if (hours > 0) {
    return `${hours}時間${mins}分`;
  }
  return `${mins}分`;
}

/**
 * ファイルサイズをフォーマット
 * @param bytes - バイト数（null/undefined の場合は '--'、0 の場合は '0 B'）
 * @returns フォーマットされた文字列 (例: "1.5 MB")
 */
export function formatFileSize(bytes: number | null): string {
  if (bytes == null) return '--';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/**
 * 日付をローカル形式でフォーマット
 * @param dateString - ISO形式の日付文字列
 * @returns フォーマットされた日付文字列
 */
export function formatDate(dateString: string | null): string {
  if (!dateString) return '--';
  const date = new Date(dateString);
  if (isNaN(date.getTime())) return '--';
  return date.toLocaleDateString('ja-JP', {
    year: 'numeric',
    month: 'short',
    day: 'numeric'
  });
}

/**
 * 曲数をフォーマット
 * @param count - 曲数
 * @returns フォーマットされた文字列 (例: "12曲")
 */
export function formatTrackCount(count: number): string {
  return `${count}曲`;
}
