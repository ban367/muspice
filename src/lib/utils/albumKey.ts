/**
 * アルバムの識別
 *
 * アルバムは「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめるため、
 * 名前だけでは区別できない（別のアーティストの「Greatest Hits」など）。
 */
import type { AlbumSummary, Track } from '#lib/types/models.js';

/**
 * コンピレーションの印があり、アルバムアーティストのない曲をまとめるアルバムアーティスト
 * （Rustの`models::VARIOUS_ARTISTS`と同じ。アルバムの識別に使うため、表示の言語によらず固定）
 */
export const VARIOUS_ARTISTS = 'Various Artists';

/**
 * 曲をまとめるアーティスト（Rustの`Track::group_artist`・`ALBUM_ARTIST`と同じ。ADR-093）
 *
 * アルバムアーティスト。なければ、コンピレーションの曲は`VARIOUS_ARTISTS`、ほかの曲は曲のアーティスト。
 */
export function groupArtist(
  track: Pick<Track, 'albumArtist' | 'artist' | 'compilation'>
): string | null {
  return track.albumArtist ?? (track.compilation ? VARIOUS_ARTISTS : track.artist);
}

/**
 * アルバムを識別するキー（一覧の中の選択・描画のキーに使う）
 */
export function albumKey(album: Pick<AlbumSummary, 'name' | 'artist'>): string {
  return JSON.stringify([album.artist, album.name]);
}
