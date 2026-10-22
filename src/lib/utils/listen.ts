/**
 * イベントの購読の解除
 *
 * `events.xxx.listen()`・`onFocusChanged()`は、購読を解除する関数をPromiseで返す。購読に失敗した
 * （Promiseが拒否された）場合も、解除の時に未処理の拒否にならないようにする。
 */

/**
 * 購読を解除する（購読に失敗していた場合は、何もしない）
 * @param listenings - `listen()`が返したPromise
 */
export function stopListening(...listenings: Promise<() => void>[]): void {
  for (const listening of listenings) {
    listening.then(
      (stop) => stop(),
      () => {}
    );
  }
}
