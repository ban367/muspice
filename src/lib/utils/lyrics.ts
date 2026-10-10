/**
 * 歌詞の解釈（表示用）
 *
 * 歌詞は、バックエンドから文字列のまま受け取る（曲と同じ名前の`.lrc`ファイルか、埋め込みの歌詞）。
 * 時刻付きの歌詞（LRC形式。行の先頭に`[分:秒.百分の1秒]`が付く）は、行ごとの時刻を読み、
 * 再生位置に合わせて行を強調できるようにする。
 */

/** 歌詞の1行 */
export interface LyricLine {
  /** 歌い始める位置（秒）。時刻のない歌詞ではnull */
  time: number | null;
  text: string;
}

/** 解釈した歌詞 */
export interface ParsedLyrics {
  lines: LyricLine[];
  /** 時刻付きの歌詞か（行は、時刻の順に並ぶ） */
  isSynced: boolean;
}

/** 行の先頭の時刻（`[01:23.45]`・`[01:23]`・`[01:23:45]`）。分・秒・秒の小数 */
const TIME_TAG = /^\[(\d+):(\d{1,2})(?:[.:](\d{1,3}))?\]/;
/** 曲の情報の行（`[ar:歌手]`・`[offset:+500]`など）。名前と値 */
const INFO_TAG = /^\[([a-zA-Z#][a-zA-Z0-9_]*):(.*)\]\s*$/;
/** 行の中の、語ごとの時刻（`<01:23.45>`。拡張LRC） */
const WORD_TIME_TAG = /<\d+:\d{1,2}(?:[.:]\d{1,3})?>/g;

/** 時刻の小数の部分を、秒にする（"5"は0.5秒、"45"は0.45秒、"450"は0.45秒） */
const toFraction = (digits: string | undefined) => (digits ? Number(`0.${digits}`) : 0);

/**
 * 歌詞の文字列を解釈する
 *
 * - 時刻の付いた行が1つでもあれば、時刻付きの歌詞として扱う。時刻の付いた行だけを、時刻の順に
 *   並べる（1行に複数の時刻が付いている場合は、時刻ごとの行にする）。`[offset:ミリ秒]`があれば、
 *   その分だけ時刻をずらす（正の値は早める）
 * - 時刻の付いた行がなければ、行ごとにそのまま返す
 */
export function parseLyrics(text: string): ParsedLyrics {
  const rawLines = text.replace(/^\uFEFF/, '').split(/\r\n|\r|\n/);
  const timed: LyricLine[] = [];
  let offsetSeconds = 0;

  for (const rawLine of rawLines) {
    let rest = rawLine.trimStart();
    const times: number[] = [];
    for (let match = TIME_TAG.exec(rest); match !== null; match = TIME_TAG.exec(rest)) {
      times.push(Number(match[1]) * 60 + Number(match[2]) + toFraction(match[3]));
      rest = rest.slice(match[0].length);
    }

    if (times.length === 0) {
      const info = INFO_TAG.exec(rest);
      if (info?.[1].toLowerCase() === 'offset') {
        const milliseconds = Number(info[2].trim());
        if (Number.isFinite(milliseconds)) offsetSeconds = milliseconds / 1000;
      }
      continue;
    }

    const lineText = rest.replace(WORD_TIME_TAG, '').trim();
    for (const time of times) timed.push({ time, text: lineText });
  }

  if (timed.length === 0) {
    // 時刻のない歌詞: 前後の空の行を除いて、そのまま返す
    const lines = text.trim() === '' ? [] : text.trim().split(/\r\n|\r|\n/);
    return { lines: lines.map((line) => ({ time: null, text: line.trimEnd() })), isSynced: false };
  }

  const lines = timed
    .map((line) => ({ ...line, time: Math.max(0, (line.time ?? 0) - offsetSeconds) }))
    // 同じ時刻の行は、書かれている順を保つ
    .sort((a, b) => a.time - b.time);
  return { lines, isSynced: true };
}

/**
 * 再生位置で歌っている行の位置を返す（最初の行より前・時刻のない歌詞では-1）
 * @param lines - `parseLyrics`が返した行
 * @param position - 再生位置（秒）
 */
export function findCurrentLine(lines: readonly LyricLine[], position: number): number {
  let current = -1;
  for (let index = 0; index < lines.length; index++) {
    const time = lines[index].time;
    if (time === null || time > position) break;
    current = index;
  }
  return current;
}
