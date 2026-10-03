/**
 * 外観の設定をCSSに反映するユーティリティ
 */
import type { Theme } from '#lib/types/models.js';

/** 画面に反映する配色（「OSの設定に従う」を解決したもの） */
export type ResolvedTheme = 'dark' | 'light';

/** OSの配色がダークかを判定するメディアクエリ */
const DARK_SCHEME_QUERY = '(prefers-color-scheme: dark)';

/** 最後に反映したテーマを保存するキー（次の起動で、設定を読み込む前に使う） */
const THEME_STORAGE_KEY = 'muspice:theme';

/**
 * アクセントカラーをCSS変数（`app.css`の`--color-primary`）に反映する
 *
 * ホバー時の色（`--color-primary-focus`）はアクセントカラーを少し暗くした色にする。
 * @param color - `#rrggbb`形式の色
 */
export function applyAccentColor(color: string): void {
  const style = document.documentElement.style;
  style.setProperty('--color-primary', color);
  style.setProperty('--color-primary-focus', `color-mix(in oklab, ${color} 82%, black)`);
}

/**
 * テーマの設定から、画面に反映する配色を決める
 * @param prefersDark OSの配色がダークか
 */
export function resolveTheme(theme: Theme, prefersDark: boolean): ResolvedTheme {
  if (theme === 'system') return prefersDark ? 'dark' : 'light';
  return theme;
}

/** 配色を`<html data-theme>`に反映する（`app.css`とDaisyUIのテーマが切り替わる） */
function setDocumentTheme(theme: ResolvedTheme): void {
  document.documentElement.dataset.theme = theme;
}

/**
 * テーマを画面に反映する
 *
 * 「OSの設定に従う」の間は、OSの配色の変化に追従する。次の起動で設定を読み込む前に
 * 使えるよう、設定を保存しておく（`restoreTheme`）。
 * @returns 追従を止める関数（テーマが変わったとき・破棄するときに呼ぶ）
 */
export function applyTheme(theme: Theme): () => void {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // 保存できなくても、次の起動で設定を読み込むまでダークになるだけ
  }

  const media = window.matchMedia(DARK_SCHEME_QUERY);
  const update = () => setDocumentTheme(resolveTheme(theme, media.matches));
  update();
  if (theme !== 'system') return () => {};

  media.addEventListener('change', update);
  return () => media.removeEventListener('change', update);
}

/**
 * 前回反映したテーマを反映する（起動直後、設定を読み込むまでの間の配色）
 *
 * OSの配色の変化には追従しない（設定を読み込んだ後の`applyTheme`が追従する）。
 */
export function restoreTheme(): void {
  let theme: string | null;
  try {
    theme = localStorage.getItem(THEME_STORAGE_KEY);
  } catch {
    return;
  }
  if (theme === 'dark' || theme === 'light' || theme === 'system') {
    setDocumentTheme(resolveTheme(theme, window.matchMedia(DARK_SCHEME_QUERY).matches));
  }
}
