/**
 * アルバムの識別
 *
 * アルバムは「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめるため、
 * 名前だけでは区別できない（別のアーティストの「Greatest Hits」など）。
 */
import type { AlbumSummary } from '#lib/types/models.js';

/**
 * アルバムを識別するキー（一覧の中の選択・描画のキーに使う）
 */
export function albumKey(album: Pick<AlbumSummary, 'name' | 'artist'>): string {
  return JSON.stringify([album.artist, album.name]);
}
