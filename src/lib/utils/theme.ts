/**
 * 外観の設定をCSSに反映するユーティリティ
 */

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
