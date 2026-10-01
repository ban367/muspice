/**
 * ネイティブダイアログのユーティリティ
 */
import { confirm } from '@tauri-apps/plugin-dialog';

/**
 * 削除などの取り消せない操作の確認ダイアログを表示
 *
 * `window.confirm`はdialogプラグインにより非同期関数へ置き換えられており、
 * 同期的に呼ぶと戻り値のPromiseが常にtrue扱いになる（回答を待たずに処理が進む）。
 * 確認が必要な箇所では必ずこの関数をawaitして使う。
 * @param message - ダイアログに表示するメッセージ
 * @returns 「削除」が押された場合はtrue
 */
export function confirmDestructive(message: string): Promise<boolean> {
  return confirm(message, {
    title: '確認',
    kind: 'warning',
    okLabel: '削除',
    cancelLabel: 'キャンセル'
  });
}
