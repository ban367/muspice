/**
 * デバウンスユーティリティ
 * 検索入力などの頻繁なイベントを制御
 */

/**
 * デバウンスされた関数を作成
 * @param callback - 実行する関数
 * @param delay - 遅延時間（ミリ秒）
 * @returns デバウンスされた関数とクリア関数
 */
export function createDebounce<T extends (...args: Parameters<T>) => void>(
  callback: T,
  delay: number = 300
): {
  debounced: (...args: Parameters<T>) => void;
  cancel: () => void;
} {
  let timer: ReturnType<typeof setTimeout> | null = null;

  const debounced = (...args: Parameters<T>) => {
    if (timer) {
      clearTimeout(timer);
    }
    timer = setTimeout(() => {
      callback(...args);
      timer = null;
    }, delay);
  };

  const cancel = () => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  };

  return { debounced, cancel };
}

/**
 * 検索用のデバウンスハンドラーを作成
 * @param onSearch - 検索実行時のコールバック
 * @param delay - デバウンス遅延（デフォルト300ms）
 * @returns 入力ハンドラーとクリア関数
 */
export function createSearchDebounce(
  onSearch: (query: string) => void,
  delay: number = 300
): {
  handleInput: (value: string) => void;
  cancel: () => void;
} {
  const { debounced, cancel } = createDebounce(onSearch, delay);
  return {
    handleInput: debounced,
    cancel
  };
}
