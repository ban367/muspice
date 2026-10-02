/**
 * アルバムアートのURL
 */
import { convertFileSrc } from '@tauri-apps/api/core';

/** アルバムアートを配信するカスタムプロトコルのスキーム（Rust側の`album_art::SCHEME`） */
const ALBUM_ART_SCHEME = 'albumart';

/**
 * トラックのアルバムアートのURLを返す
 *
 * `<img>`の`src`にそのまま指定する。画像はRust側の`albumart`プロトコルが配信し、
 * アートがないトラックでは読み込みエラーになる（`AlbumArt`コンポーネントがプレースホルダーを表示する）。
 * @param trackId - トラックID（ない場合はnullを返す）
 */
export function albumArtUrl(trackId: string | null | undefined): string | null {
  return trackId ? convertFileSrc(trackId, ALBUM_ART_SCHEME) : null;
}
