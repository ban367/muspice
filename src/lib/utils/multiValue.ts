/**
 * 複数の値を持つ項目（アーティスト・ジャンル）の扱い
 *
 * 1曲に複数のアーティスト・ジャンルがある場合、曲の`artist`・`genre`は、値を`; `でつないだ
 * 文字列になっている（`Rock; Pop`）。一覧の集計・絞り込みでは、ここで値に分ける
 * （Rust側の`src-tauri/src/multi_value.rs`と同じ決まり。ADR-051）。
 *
 * `/`・`,`・`&`では分けない（`AC/DC`・`Earth, Wind & Fire`は1つの値）。
 */

/** 値の区切り */
export const VALUE_SEPARATOR = ';';

/** 文字列を、値の一覧に分ける（前後の空白を除く。空の値と、同じ値の2つ目以降は除く） */
export function splitValues(text: string | null | undefined): string[] {
  if (!text) return [];
  const values: string[] = [];
  for (const part of text.split(VALUE_SEPARATOR)) {
    const value = part.trim();
    if (value && !values.includes(value)) values.push(value);
  }
  return values;
}

/** 値の一覧に、その値があるか */
export function hasValue(text: string | null | undefined, value: string): boolean {
  return splitValues(text).includes(value);
}

/**
 * 曲を一覧でまとめるアーティスト（複数ある曲は、それぞれ）
 *
 * アルバムアーティストがあれば、それだけ（コンピレーションの曲は、曲ごとのアーティストでは
 * まとめない）。なければ、曲のアーティストのそれぞれの値。
 */
export function listedArtists(track: {
  artist: string | null;
  albumArtist: string | null;
}): string[] {
  return track.albumArtist !== null ? [track.albumArtist] : splitValues(track.artist);
}
