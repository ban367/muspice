/**
 * ブラウザモック用のメディア生成（アルバムアート）
 *
 * 実ファイルを用意せずに済むよう、文字列から決定的に生成する。
 */
/** 文字列から決定的なハッシュ値を得る（FNV-1a） */
export function hashString(value: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < value.length; i++) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/**
 * アルバム名ごとに色の異なるグラデーション画像をdata URLで生成する
 *
 * SVGはASCIIのみで構成し、`btoa`でそのままBase64化できるようにする。
 */
export function createAlbumArt(albumName: string): string {
  const hash = hashString(albumName);
  const startHue = hash % 360;
  const endHue = (startHue + 40 + ((hash >>> 9) % 80)) % 360;
  const svg =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120">' +
    '<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">' +
    `<stop offset="0" stop-color="hsl(${startHue},65%,55%)"/>` +
    `<stop offset="1" stop-color="hsl(${endHue},70%,28%)"/>` +
    '</linearGradient></defs>' +
    '<rect width="120" height="120" fill="url(#g)"/>' +
    '<circle cx="60" cy="60" r="34" fill="none" stroke="rgba(255,255,255,0.35)" stroke-width="2"/>' +
    '<circle cx="60" cy="60" r="6" fill="rgba(255,255,255,0.6)"/>' +
    '</svg>';
  return `data:image/svg+xml;base64,${btoa(svg)}`;
}
