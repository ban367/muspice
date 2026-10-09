/**
 * アルバムアートのURL
 */
import { convertFileSrc } from '@tauri-apps/api/core';
import { albumArtVersion } from '#lib/stores/albumArt.svelte.js';

/** アルバムアートを配信するカスタムプロトコルのスキーム（Rust側の`album_art::SCHEME`） */
const ALBUM_ART_SCHEME = 'albumart';

/**
 * トラックのアルバムアートのURLを返す
 *
 * `<img>`の`src`にそのまま指定する。画像はRust側の`albumart`プロトコルが配信し、
 * アートがないトラックでは読み込みエラーになる（`AlbumArt`コンポーネントがプレースホルダーを表示する）。
 *
 * アプリでアルバムアートを書き換えた曲は、URLに版を付けて読み直させる（版の読み取りは
 * 変更に追随するため、テンプレート・`$derived`の中で呼ぶ）。
 * @param trackId - トラックID（ない場合はnullを返す）
 */
export function albumArtUrl(trackId: string | null | undefined): string | null {
  if (!trackId) return null;

  const url = convertFileSrc(trackId, ALBUM_ART_SCHEME);
  const version = albumArtVersion(trackId);
  // 版はクエリに付ける（Rust側はパスだけを見る）。ブラウザ確認用のモックが返すdata URLは、
  // 画像が変わるとURLそのものが変わるため付けない
  return version > 0 && !url.startsWith('data:') ? `${url}?v=${version}` : url;
}
