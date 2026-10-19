/**
 * アルバムの識別
 *
 * アルバムは「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめるため、
 * 名前だけでは区別できない（別のアーティストの「Greatest Hits」など）。
 */
import type { AlbumSummary, Track } from '#lib/types/models.js';

/**
 * 曲をまとめるアーティスト（アルバムアーティスト。なければ曲のアーティスト。Rustの`ALBUM_ARTIST`と同じ）
 */
export function groupArtist(track: Pick<Track, 'albumArtist' | 'artist'>): string | null {
  return track.albumArtist ?? track.artist;
}

/**
 * アルバムを識別するキー（一覧の中の選択・描画のキーに使う）
 */
export function albumKey(album: Pick<AlbumSummary, 'name' | 'artist'>): string {
  return JSON.stringify([album.artist, album.name]);
}
