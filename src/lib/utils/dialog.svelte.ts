/**
 * ダイアログのユーティリティ
 *
 * 確認ダイアログはOSのネイティブダイアログ（dialogプラグイン）、テキスト入力は
 * アプリ内のダイアログ（`TextPromptDialog`）で表示する。
 */
import { confirm } from '@tauri-apps/plugin-dialog';
import { m } from '#lib/i18n/i18n.svelte.js';

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
    title: m.dialog.confirmTitle,
    kind: 'warning',
    okLabel: m.common.delete,
    cancelLabel: m.common.cancel
  });
}

/** テキスト入力ダイアログの表示内容 */
export interface TextPromptOptions {
  /** ダイアログのタイトル */
  title: string;
  /** 入力欄のラベル */
  label: string;
  /** 入力欄の初期値 */
  defaultValue?: string;
  /** 確定ボタンのラベル */
  confirmLabel?: string;
  /** 入力値（前後の空白を除いたもの）の検証。エラーメッセージを返すと確定できない */
  validate?: (value: string) => string | null;
}

/** 表示中のテキスト入力ダイアログ */
export interface TextPromptRequest extends TextPromptOptions {
  /** 入力値（キャンセル時はnull）でダイアログを閉じる */
  resolve: (value: string | null) => void;
}

/** テキスト入力ダイアログの要求（表示は`TextPromptDialog`、要求は`promptText()`で行う） */
class TextPrompt {
  #request = $state.raw<TextPromptRequest | null>(null);

  /** 表示中のテキスト入力ダイアログ（`TextPromptDialog`が読んで表示する） */
  get request(): TextPromptRequest | null {
    return this.#request;
  }

  /** ダイアログを表示し、確定された文字列（キャンセル時はnull）を返す */
  open(options: TextPromptOptions): Promise<string | null> {
    this.#request?.resolve(null);

    return new Promise((resolve) => {
      const request: TextPromptRequest = {
        ...options,
        resolve: (value) => {
          // 後から来た要求で置き換えられていたら、表示中のダイアログは閉じない
          if (this.#request === request) this.#request = null;
          resolve(value);
        }
      };
      this.#request = request;
    });
  }
}

export const textPrompt = new TextPrompt();

/**
 * テキスト入力ダイアログを表示し、確定された文字列を返す
 *
 * `window.prompt`はmacOSのWebView（wry）が実装しておらず常にnullを返すため使わない。
 * 表示中に別の入力を求めた場合、前のダイアログはキャンセルとして扱う。
 * @returns 前後の空白を除いた入力値。キャンセル時はnull
 */
export function promptText(options: TextPromptOptions): Promise<string | null> {
  return textPrompt.open(options);
}
