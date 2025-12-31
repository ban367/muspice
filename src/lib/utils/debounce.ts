/**
 * デバウンスユーティリティ
 * 検索入力などの頻繁なイベントを遅延させる
 */

/**
 * 汎用デバウンス関数を作成
 * @param callback - 実行するコールバック関数
 * @param delay - 遅延時間（ミリ秒）
 * @returns デバウンスされた関数とキャンセル関数
 */
export function createDebounce<T extends (...args: Parameters<T>) => void>(
  callback: T,
  delay: number = 300
): { debounced: (...args: Parameters<T>) => void; cancel: () => void } {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  const debounced = (...args: Parameters<T>) => {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(() => {
      callback(...args);
      timeoutId = null;
    }, delay);
  };

  const cancel = () => {
    if (timeoutId) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
  };

  return { debounced, cancel };
}

/**
 * 検索用のデバウンスフックを作成
 * @param onSearch - 検索実行時のコールバック
 * @param delay - 遅延時間（ミリ秒、デフォルト300ms）
 * @returns 入力ハンドラとキャンセル関数
 */
export function createSearchDebounce(
  onSearch: (query: string) => void,
  delay: number = 300
): { handleInput: (value: string) => void; cancel: () => void } {
  const { debounced, cancel } = createDebounce(onSearch, delay);

  return {
    handleInput: debounced,
    cancel
  };
}
