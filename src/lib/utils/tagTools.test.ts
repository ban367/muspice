import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import {
  countFieldChanges,
  guessFromFileName,
  parseFileNamePattern,
  renumberTracks,
  searchAndReplace,
  toMetadataChanges,
  type ReplaceOptions,
  type ToolResult
} from './tagTools.js';

function track(id: string, filePath: string, overrides: Partial<Track> = {}): Track {
  return {
    id,
    filePath,
    fileName: filePath.split('/').pop() ?? '',
    title: null,
    artist: null,
    album: null,
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
    discNumber: null,
    duration: null,
    fileSize: 1,
    format: 'mp3',
    bitrate: null,
    sampleRate: null,
    isFavorite: false,
    rating: 0,
    playCount: 0,
    skipCount: 0,
    lastPlayedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
    sortTags: { title: null, artist: null, album: null, albumArtist: null },
    isMissing: false,
    ...overrides
  };
}

/** 結果を「トラックID: 項目=後の値」の形にする（比べやすくする） */
function summarize(result: ToolResult): Record<string, Record<string, string>> {
  return Object.fromEntries(
    result.changes.map((change) => [
      change.track.id,
      Object.fromEntries(change.fields.map((field) => [field.field, field.after]))
    ])
  );
}

describe('parseFileNamePattern', () => {
  it('書式の誤りを返す', () => {
    const error = (pattern: string) => {
      const parsed = parseFileNamePattern(pattern);
      return parsed.ok ? null : parsed.error;
    };

    expect(error('%track% - %title%')).toBeNull();
    expect(error('  ')).toBe('emptyPattern');
    expect(error('track - title')).toBe('noPlaceholder');
    expect(error('%dummy% - %dummy%')).toBe('noPlaceholder');
    expect(error('%track% - %name%')).toBe('unknownPlaceholder');
    expect(error('%title% - %title%')).toBe('duplicatePlaceholder');
    // 文字列の項目が続くと、分け目が決まらない
    expect(error('%artist%%title%')).toBe('adjacentPlaceholders');
    // 番号の後に文字列が続くのは、分けられる
    expect(error('%track%%title%')).toBeNull();
  });

  it('知らない項目の名前を返す', () => {
    const parsed = parseFileNamePattern('%track% - %name%');
    expect(parsed).toMatchObject({ ok: false, detail: '%name%' });
  });
});

describe('guessFromFileName', () => {
  it('ファイル名から、番号とタイトルを読み取る', () => {
    const tracks = [
      track('a', '/music/01 - 青い地平線.mp3'),
      track('b', '/music/02 - Paper Planes.flac'),
      track('c', '/music/12 - A - B.mp3')
    ];

    const result = guessFromFileName(tracks, '%track% - %title%');

    expect(result.error).toBeNull();
    expect(summarize(result)).toEqual({
      a: { trackNumber: '1', title: '青い地平線' },
      b: { trackNumber: '2', title: 'Paper Planes' },
      // 最後の項目は、残りすべてに一致する
      c: { trackNumber: '12', title: 'A - B' }
    });
    expect(result.changes[0].fields[0]).toEqual({ field: 'trackNumber', before: null, after: '1' });
  });

  it('途中の項目は、次の区切りまでに一致する', () => {
    const tracks = [track('a', '/music/Aoi Sora - Blue Horizon - 03 - Summer Letter.mp3')];

    const result = guessFromFileName(tracks, '%artist% - %album% - %track% - %title%');

    expect(summarize(result)).toEqual({
      a: { artist: 'Aoi Sora', album: 'Blue Horizon', trackNumber: '3', title: 'Summer Letter' }
    });
  });

  it('フォルダの名前も使える', () => {
    const tracks = [
      track('a', '/Users/me/Music/ネオン通り/夜明けのシグナル/2-03 港の灯り.m4a'),
      track('b', 'C:\\Music\\The Voltage\\Midnight Circuit\\1-01 Overclock.mp3')
    ];

    const result = guessFromFileName(tracks, '%artist%/%album%/%disc%-%track% %title%');

    expect(summarize(result)).toEqual({
      a: {
        artist: 'ネオン通り',
        album: '夜明けのシグナル',
        discNumber: '2',
        trackNumber: '3',
        title: '港の灯り'
      },
      b: {
        artist: 'The Voltage',
        album: 'Midnight Circuit',
        discNumber: '1',
        trackNumber: '1',
        title: 'Overclock'
      }
    });
  });

  it('読み飛ばす部分・年・番号の直後の文字列を扱える', () => {
    const tracks = [
      track('a', '/music/[2021] Aoi Sora - 青い地平線 (Remaster).mp3'),
      track('b', '/music/07Paper Planes.mp3')
    ];

    expect(
      summarize(guessFromFileName([tracks[0]], '[%year%] %artist% - %title% (%dummy%)'))
    ).toEqual({
      a: { year: '2021', artist: 'Aoi Sora', title: '青い地平線' }
    });
    expect(summarize(guessFromFileName([tracks[1]], '%track%%title%'))).toEqual({
      b: { trackNumber: '7', title: 'Paper Planes' }
    });
  });

  it('書式に合わない曲は変えず、数える', () => {
    const tracks = [
      track('a', '/music/01 - Song.mp3'),
      // 区切りがない・番号が数字でない・番号が範囲の外・フォルダが足りない
      track('b', '/music/Song.mp3'),
      track('c', '/music/A1 - Song.mp3'),
      track('d', '/music/1000 - Song.mp3')
    ];

    const result = guessFromFileName(tracks, '%track% - %title%');

    expect(Object.keys(summarize(result))).toEqual(['a']);
    expect(result.unmatchedCount).toBe(3);
    expect(guessFromFileName([track('e', 'Song.mp3')], '%album%/%title%').unmatchedCount).toBe(1);
  });

  it('今の値と同じ項目は、変更に含めない', () => {
    const tracks = [
      track('a', '/music/01 - Song.mp3', { trackNumber: 1, title: 'Song' }),
      track('b', '/music/02 - Song B.mp3', { trackNumber: 2, title: '古いタイトル' })
    ];

    const result = guessFromFileName(tracks, '%track% - %title%');

    expect(summarize(result)).toEqual({ b: { title: 'Song B' } });
    expect(result.changes[0].fields[0].before).toBe('古いタイトル');
    expect(result.unmatchedCount).toBe(0);
  });

  it('濁点が分かれた形（NFD）のファイル名は、合成した形で読み取る', () => {
    const nfd = '01 - がくと'.normalize('NFD');
    expect(nfd).not.toBe('01 - がくと');

    const result = guessFromFileName([track('a', `/music/${nfd}.mp3`)], '%track% - %title%');

    expect(summarize(result).a.title).toBe('がくと');
  });

  it('書式の誤りは、変更なしで返す', () => {
    const result = guessFromFileName([track('a', '/music/01 - Song.mp3')], '%track% - %nope%');

    expect(result).toMatchObject({
      changes: [],
      error: 'unknownPlaceholder',
      errorDetail: '%nope%'
    });
  });
});

describe('renumberTracks', () => {
  const tracks = [
    track('a', '/music/a.mp3', { trackNumber: 5 }),
    track('b', '/music/b.mp3', { trackNumber: 2 }),
    track('c', '/music/c.mp3')
  ];

  it('渡した順に番号を振り、変わらない曲は含めない', () => {
    const result = renumberTracks(tracks, { start: 1, setTotal: false });

    expect(summarize(result)).toEqual({ a: { trackNumber: '1' }, c: { trackNumber: '3' } });
    expect(result.changes[0].fields[0].before).toBe('5');
  });

  it('総数を書き込む場合は、すべての曲に書き込む', () => {
    const result = renumberTracks(tracks, { start: 1, setTotal: true });

    expect(summarize(result)).toEqual({
      a: { trackNumber: '1', trackTotal: '3' },
      b: { trackTotal: '3' },
      c: { trackNumber: '3', trackTotal: '3' }
    });
    // 今の総数は分からない
    expect(result.changes[1].fields[0].before).toBeNull();
  });

  it('最初の番号を変えられる（総数は、最後の番号）', () => {
    const result = renumberTracks(tracks, { start: 11, setTotal: true });

    expect(summarize(result).c).toEqual({ trackNumber: '13', trackTotal: '13' });
  });

  it('番号が範囲の外になる場合は、誤りにする', () => {
    expect(renumberTracks(tracks, { start: 0, setTotal: false }).error).toBe('numberOutOfRange');
    expect(renumberTracks(tracks, { start: 998, setTotal: false }).error).toBe('numberOutOfRange');
    expect(renumberTracks(tracks, { start: 1.5, setTotal: false }).error).toBe('numberOutOfRange');
    expect(renumberTracks(tracks, { start: 997, setTotal: false }).error).toBeNull();
  });
});

describe('searchAndReplace', () => {
  const options = (overrides: Partial<ReplaceOptions>): ReplaceOptions => ({
    fields: ['title'],
    search: '',
    replace: '',
    matchCase: false,
    useRegex: false,
    caseConversion: 'none',
    trim: false,
    ...overrides
  });
  const tracks = [
    track('a', '/music/a.mp3', { title: 'Song (Remastered)', artist: 'aoi sora', genre: 'J-Pop' }),
    track('b', '/music/b.mp3', { title: ' song b ', artist: 'AOI SORA', genre: 'j-pop' }),
    track('c', '/music/c.mp3', { title: null, artist: 'Mika', genre: null })
  ];

  it('選んだ項目の文字列を置き換える（大文字と小文字は区別しない）', () => {
    const result = searchAndReplace(tracks, options({ search: 'song', replace: 'Track' }));

    expect(summarize(result)).toEqual({
      a: { title: 'Track (Remastered)' },
      b: { title: ' Track b ' }
    });
    expect(result.changes[0].fields[0].before).toBe('Song (Remastered)');
  });

  it('大文字と小文字を区別できる', () => {
    const result = searchAndReplace(
      tracks,
      options({ search: 'song', replace: 'Track', matchCase: true })
    );

    expect(Object.keys(summarize(result))).toEqual(['b']);
  });

  it('正規表現でなければ、記号をそのままの文字として扱う', () => {
    const result = searchAndReplace(tracks, options({ search: ' (Remastered)', replace: '$1' }));

    expect(summarize(result)).toEqual({ a: { title: 'Song$1' } });
  });

  it('正規表現では、グループを置き換える文字列に使える', () => {
    const result = searchAndReplace(
      tracks,
      options({ search: '^(\\w+) \\((.+)\\)$', replace: '$2: $1', useRegex: true })
    );

    expect(summarize(result)).toEqual({ a: { title: 'Remastered: Song' } });
    expect(
      searchAndReplace(tracks, options({ search: '(', replace: '', useRegex: true })).error
    ).toBe('invalidRegex');
  });

  it('複数の項目をまとめて置き換える', () => {
    const result = searchAndReplace(
      tracks,
      options({ fields: ['artist', 'genre'], search: 'j-pop', replace: 'J-POP', matchCase: true })
    );

    expect(summarize(result)).toEqual({ b: { genre: 'J-POP' } });
  });

  it('大文字・小文字を変換する', () => {
    const convert = (caseConversion: ReplaceOptions['caseConversion']) =>
      summarize(searchAndReplace(tracks, options({ fields: ['artist'], caseConversion })));

    expect(convert('upper')).toEqual({ a: { artist: 'AOI SORA' }, c: { artist: 'MIKA' } });
    expect(convert('lower')).toEqual({ b: { artist: 'aoi sora' }, c: { artist: 'mika' } });
    expect(convert('title')).toEqual({ a: { artist: 'Aoi Sora' }, b: { artist: 'Aoi Sora' } });
  });

  it('単語の先頭を大文字にする変換は、アポストロフィの後を単語の途中として扱う', () => {
    const list = [track('a', '/music/a.mp3', { title: "DON'T STOP (live at tokyo-dome)" })];

    const result = searchAndReplace(list, options({ caseConversion: 'title' }));

    expect(summarize(result).a.title).toBe("Don't Stop (Live At Tokyo-Dome)");
  });

  it('前後の空白を除く（置換・変換の後に行う）', () => {
    const result = searchAndReplace(
      tracks,
      options({ search: '(Remastered)', replace: '', trim: true })
    );

    expect(summarize(result)).toEqual({ a: { title: 'Song' }, b: { title: 'song b' } });
  });

  it('結果が空になった項目は、空の値にする（タグから取り除く）', () => {
    const result = searchAndReplace(
      tracks,
      options({ fields: ['genre'], search: 'j-pop', replace: '' })
    );

    expect(summarize(result)).toEqual({ a: { genre: '' }, b: { genre: '' } });
  });

  it('対象の項目がない・長すぎる値になる場合は、誤りにする', () => {
    expect(searchAndReplace(tracks, options({ fields: [] })).error).toBe('noFields');
    const tooLong = searchAndReplace(
      tracks,
      options({ fields: ['genre'], search: 'j-pop', replace: 'あ'.repeat(101) })
    );
    expect(tooLong).toMatchObject({ changes: [], error: 'tooLong' });
    // 文字数で数える（絵文字を2文字と数えない）
    const emoji = searchAndReplace(
      tracks,
      options({ fields: ['genre'], search: 'j-pop', replace: '🎵'.repeat(100) })
    );
    expect(emoji.error).toBeNull();
  });
});

describe('toMetadataChanges', () => {
  it('変更を、曲ごとのメタデータにする（数値は数値に、空の文字列はそのまま）', () => {
    const tracks = [
      track('a', '/music/01 - Song.mp3', { genre: 'Other' }),
      track('b', '/music/02 - Song B.mp3')
    ];
    const guessed = guessFromFileName(tracks, '%track% - %title%');
    const cleared = searchAndReplace(tracks, {
      fields: ['genre'],
      search: 'Other',
      replace: '',
      matchCase: false,
      useRegex: false,
      caseConversion: 'none',
      trim: false
    });

    expect(toMetadataChanges(guessed.changes)).toEqual([
      { trackId: 'a', metadata: { trackNumber: 1, title: 'Song' } },
      { trackId: 'b', metadata: { trackNumber: 2, title: 'Song B' } }
    ]);
    expect(toMetadataChanges(cleared.changes)).toEqual([{ trackId: 'a', metadata: { genre: '' } }]);
    expect(countFieldChanges(guessed.changes)).toBe(4);
    expect(
      toMetadataChanges(renumberTracks(tracks, { start: 1, setTotal: true }).changes)[0].metadata
    ).toEqual({ trackNumber: 1, trackTotal: 2 });
  });
});
