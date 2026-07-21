/**
 * 共通のSvelteアクション
 */

export interface IntersectionObserverOptions {
  /** 要素が表示領域に入ったときに実行するコールバック */
  callback: () => void;
  /** 事前読み込みのマージン（デフォルト100px） */
  rootMargin?: string;
}

/**
 * 要素が表示領域に入ったら一度だけコールバックを実行するアクション
 *
 * アルバムアートの遅延読み込みに使用する。発火後は監視を解除するため、
 * スクロールで再表示されても重複して実行されない。
 *
 * @example
 * ```svelte
 * <div use:intersectionObserver={{ callback: () => loadAlbumArt(id) }}>
 * ```
 */
export function intersectionObserver(node: HTMLElement, options: IntersectionObserverOptions) {
  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          options.callback();
          observer.unobserve(node);
        }
      });
    },
    { rootMargin: options.rootMargin ?? '100px' }
  );

  observer.observe(node);

  return {
    destroy() {
      observer.disconnect();
    }
  };
}
