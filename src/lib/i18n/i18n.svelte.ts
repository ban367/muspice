/**
 * 表示の言語（多言語化）
 *
 * 画面の文言は`m`から読む（例: `m.common.cancel`、`m.common.trackCount(3)`）。
 * `m`は読むたびに今の言語のメッセージを返し、言語は`$state`のため、テンプレートや
 * `$derived`の中で読めば、言語を切り替えたときに表示も変わる（コンポーネントを作り直さない
 * ため、再生も止まらない）。スクリプトの初期化時に文字列として取り出した値は変わらないので、
 * 選択肢のラベルなどは`$derived`で作るか、テンプレートで読む。
 */
import type { Language } from '#lib/types/models.js';
import { en } from './messages/en.js';
import { ja, type Messages } from './messages/ja.js';

export type { Messages };

const MESSAGES: Record<Language, Messages> = { ja, en };

/** 日付・数値の書式に使うロケール */
const LOCALES: Record<Language, string> = { ja: 'ja-JP', en: 'en-US' };

/** 最後に反映した言語を保存するキー（次の起動で、設定を読み込む前に使う） */
const LANGUAGE_STORAGE_KEY = 'muspice:language';

class I18n {
  /** 表示の言語 */
  language = $state<Language>('ja');

  /** 日付・数値の書式に使うロケール（例: `ja-JP`） */
  get locale(): string {
    return LOCALES[this.language];
  }
}

export const i18n = new I18n();

/** 今の言語のメッセージ */
export const m: Messages = new Proxy({} as Messages, {
  get(_target, key) {
    return MESSAGES[i18n.language][key as keyof Messages];
  }
});

function isLanguage(value: unknown): value is Language {
  return value === 'ja' || value === 'en';
}

/** 表示の言語を変える（`<html lang>`も合わせる） */
function setLanguage(language: Language): void {
  i18n.language = language;
  document.documentElement.lang = language;
}

/**
 * 言語を切り替える
 *
 * 次の起動で設定を読み込む前に使えるよう、言語を保存しておく（`restoreLanguage`）。
 */
export function applyLanguage(language: Language): void {
  setLanguage(language);
  try {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
  } catch {
    // 保存できなくても、次の起動で設定を読み込むまで日本語になるだけ
  }
}

/** 前回反映した言語を反映する（起動直後、設定を読み込むまでの間の言語） */
export function restoreLanguage(): void {
  let language: string | null;
  try {
    language = localStorage.getItem(LANGUAGE_STORAGE_KEY);
  } catch {
    return;
  }
  if (isLanguage(language)) {
    setLanguage(language);
  }
}
