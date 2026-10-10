import { describe, expect, it } from 'vitest';
import { findCurrentLine, parseLyrics } from './lyrics.js';

describe('parseLyrics', () => {
  it('時刻のない歌詞は、行ごとにそのまま返す', () => {
    const parsed = parseLyrics('\n1行目\n\n  2行目  \r\n3行目\n\n');

    expect(parsed.isSynced).toBe(false);
    expect(parsed.lines).toEqual([
      { time: null, text: '1行目' },
      { time: null, text: '' },
      { time: null, text: '  2行目' },
      { time: null, text: '3行目' }
    ]);
  });

  it('空の歌詞は、行なし', () => {
    expect(parseLyrics('')).toEqual({ lines: [], isSynced: false });
    expect(parseLyrics('  \n \n')).toEqual({ lines: [], isSynced: false });
  });

  it('時刻付きの歌詞は、行ごとの時刻を読む', () => {
    const parsed = parseLyrics(
      ['[ar:歌手]', '[ti:曲]', '[00:12.34]1行目', '[01:02.5]2行目', '[01:10]3行目', ''].join('\n')
    );

    expect(parsed.isSynced).toBe(true);
    expect(parsed.lines).toEqual([
      { time: 12.34, text: '1行目' },
      { time: 62.5, text: '2行目' },
      { time: 70, text: '3行目' }
    ]);
  });

  it('百分の1秒をコロンで区切った時刻・ミリ秒の時刻・1時間を超える分も読む', () => {
    const parsed = parseLyrics('[00:05:50]a\n[00:06.789]b\n[75:00.00]c');

    expect(parsed.lines.map((line) => line.time)).toEqual([5.5, 6.789, 4500]);
  });

  it('1行に複数の時刻が付いている場合は、時刻ごとの行にして、時刻の順に並べる', () => {
    const parsed = parseLyrics('[00:10.00][00:50.00]サビ\n[00:30.00]Aメロ');

    expect(parsed.lines).toEqual([
      { time: 10, text: 'サビ' },
      { time: 30, text: 'Aメロ' },
      { time: 50, text: 'サビ' }
    ]);
  });

  it('時刻だけの行（間奏）は、空の行として残す', () => {
    const parsed = parseLyrics('[00:10.00]歌\n[00:20.00]\n[00:30.00]歌');

    expect(parsed.lines.map((line) => line.text)).toEqual(['歌', '', '歌']);
  });

  it('語ごとの時刻（拡張LRC）は取り除く', () => {
    const parsed = parseLyrics('[00:10.00]<00:10.00>きょう<00:10.80>は<00:11.20>晴れ');

    expect(parsed.lines).toEqual([{ time: 10, text: 'きょうは晴れ' }]);
  });

  it('offsetの分だけ、時刻をずらす（正の値は早める。0より前にはしない）', () => {
    expect(parseLyrics('[offset:+500]\n[00:10.00]a').lines[0].time).toBe(9.5);
    expect(parseLyrics('[offset: -1500]\n[00:10.00]a').lines[0].time).toBe(11.5);
    expect(parseLyrics('[00:00.20]a\n[offset:1000]').lines[0].time).toBe(0);
    // 数でないoffsetは無視する
    expect(parseLyrics('[offset:abc]\n[00:10.00]a').lines[0].time).toBe(10);
  });

  it('時刻付きの歌詞では、時刻のない行を含めない', () => {
    const parsed = parseLyrics('作詞: だれか\n[00:10.00]歌\n\n[by:つくった人]');

    expect(parsed.lines).toEqual([{ time: 10, text: '歌' }]);
  });

  it('括弧で始まるだけの行は、時刻として扱わない', () => {
    const parsed = parseLyrics('[Chorus]\nラララ\n[Verse 2]');

    expect(parsed.isSynced).toBe(false);
    expect(parsed.lines.map((line) => line.text)).toEqual(['[Chorus]', 'ラララ', '[Verse 2]']);
  });

  it('BOMが付いていても読む', () => {
    expect(parseLyrics('\uFEFF[00:01.00]a').lines).toEqual([{ time: 1, text: 'a' }]);
  });
});

describe('findCurrentLine', () => {
  const { lines } = parseLyrics('[00:10.00]a\n[00:20.00]b\n[00:30.00]c');

  it('再生位置で歌っている行を返す', () => {
    expect(findCurrentLine(lines, 10)).toBe(0);
    expect(findCurrentLine(lines, 19.99)).toBe(0);
    expect(findCurrentLine(lines, 20)).toBe(1);
    expect(findCurrentLine(lines, 300)).toBe(2);
  });

  it('最初の行より前は、行なし', () => {
    expect(findCurrentLine(lines, 0)).toBe(-1);
    expect(findCurrentLine(lines, 9.99)).toBe(-1);
    expect(findCurrentLine([], 10)).toBe(-1);
  });

  it('時刻のない歌詞では、行なし', () => {
    expect(findCurrentLine(parseLyrics('a\nb').lines, 100)).toBe(-1);
  });
});
