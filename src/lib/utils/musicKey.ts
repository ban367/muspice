/**
 * 曲の解析で求めたキー（主音の0〜11。短調は12を足す。Rustの`TrackDetails.analyzed_key`）の表記（ADR-115）
 *
 * 一般的なDJ・音楽管理ソフトと同じく、長調は主音の名前（`C`・`F♯`）、短調は`m`を付ける（`Am`）。
 * 黒鍵は、よく使う方の名前（`E♭`・`F♯`など）にする。
 */

const PITCH_NAMES = ['C', 'D♭', 'D', 'E♭', 'E', 'F', 'F♯', 'G', 'A♭', 'A', 'B♭', 'B'] as const;

/** キーの表記（範囲の外の値・値のない曲は空） */
export function formatMusicKey(key: number | null): string {
  if (key === null || !Number.isInteger(key) || key < 0 || key >= 24) return '';
  const name = PITCH_NAMES[key % 12];
  return key >= 12 ? `${name}m` : name;
}
