import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import {
  UNKNOWN_YEAR_KEY,
  buildFolderTree,
  buildYearTree,
  flattenTree,
  folderOf,
  isInFolder,
  recentlyAdded,
  sortTracksForBrowsing,
  tracksInFolder,
  tracksInYear,
  type TreeNode
} from './libraryViews.js';

const noSortTags = { title: null, artist: null, album: null, albumArtist: null };

function track(id: string, overrides: Partial<Track> = {}): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title: id,
    artist: null,
    album: null,
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
    discNumber: null,
    duration: 60,
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
    sortTags: noSortTags,
    isMissing: false,
    ...overrides
  };
}

const ids = (tracks: readonly Track[]) => tracks.map((t) => t.id);
/** 木を「名前(曲数)」の入れ子の形にする（比べやすくする） */
const outline = (nodes: TreeNode[]): unknown[] =>
  nodes.map((node) =>
    node.children.length > 0
      ? [`${node.label}(${node.count})`, outline(node.children)]
      : `${node.label}(${node.count})`
  );

describe('recentlyAdded', () => {
  it('アルバムごとに1項目にまとめ、追加した日時の新しい順に並べる', () => {
    const tracks = [
      track('a1', { album: 'A', artist: 'X', trackNumber: 2, createdAt: '2026-03-01T00:00:00Z' }),
      track('a2', { album: 'A', artist: 'X', trackNumber: 1, createdAt: '2026-03-01T00:00:01Z' }),
      track('b1', { album: 'B', artist: 'Y', createdAt: '2026-04-01T00:00:00Z' }),
      track('c1', { album: 'C', artist: 'Z', createdAt: '2026-02-01T00:00:00Z' })
    ];

    const items = recentlyAdded(tracks);

    expect(items.map((item) => item.name)).toEqual(['B', 'A', 'C']);
    expect(items[1]).toMatchObject({
      kind: 'album',
      artist: 'X',
      trackCount: 2,
      totalDuration: 120,
      // アルバムアートは、アルバムの最初の曲（トラック番号の順）
      representativeTrackId: 'a2',
      // 追加した日時は、アルバムの曲のうち最も新しいもの
      addedAt: '2026-03-01T00:00:01Z'
    });
  });

  it('アルバムに後から曲を足すと、足した日時の位置に来る', () => {
    const tracks = [
      track('old1', { album: 'Old', artist: 'X', createdAt: '2025-01-01T00:00:00Z' }),
      track('new1', { album: 'New', artist: 'Y', createdAt: '2026-01-01T00:00:00Z' }),
      track('old2', { album: 'Old', artist: 'X', createdAt: '2026-06-01T00:00:00Z' })
    ];

    expect(recentlyAdded(tracks).map((item) => item.name)).toEqual(['Old', 'New']);
  });

  it('アルバムのない曲は、1曲で1項目にする', () => {
    const tracks = [
      track('s1', { title: 'Single', artist: 'X', createdAt: '2026-05-01T00:00:00Z' }),
      track('s2', { title: null, fileName: 'demo.mp3', createdAt: '2026-04-01T00:00:00Z' }),
      track('a1', { album: 'A', artist: 'X', createdAt: '2026-04-15T00:00:00Z' })
    ];

    const items = recentlyAdded(tracks);

    expect(items.map((item) => `${item.kind}:${item.name}`)).toEqual([
      'track:Single',
      'album:A',
      'track:demo.mp3'
    ]);
    expect(items[0].track?.id).toBe('s1');
    expect(items[1].track).toBeUndefined();
  });

  it('同じ名前のアルバムでも、アーティストが違えば別の項目（アルバムアーティストでまとめる）', () => {
    const tracks = [
      track('a', { album: 'Greatest Hits', artist: 'A' }),
      track('b', { album: 'Greatest Hits', artist: 'B' }),
      track('c1', { album: 'Compilation', artist: 'A', albumArtist: 'Various Artists' }),
      track('c2', { album: 'Compilation', artist: 'B', albumArtist: 'Various Artists' })
    ];

    const items = recentlyAdded(tracks);

    expect(items).toHaveLength(3);
    expect(items.find((item) => item.name === 'Compilation')).toMatchObject({
      artist: 'Various Artists',
      trackCount: 2
    });
    expect(new Set(items.map((item) => item.key)).size).toBe(3);
  });
});

describe('フォルダ別', () => {
  it('folderOf は、ファイル名を除いたパスを返す', () => {
    expect(folderOf('/music/rock/a.mp3')).toBe('/music/rock');
    expect(folderOf('/a.mp3')).toBe('/');
    expect(folderOf('C:\\Music\\Rock\\a.mp3')).toBe('C:\\Music\\Rock');
    expect(folderOf('a.mp3')).toBe('');
  });

  it('isInFolder は、名前が前方一致するだけの別のフォルダを含めない', () => {
    expect(isInFolder('/music/rock', '/music')).toBe(true);
    expect(isInFolder('/music', '/music')).toBe(true);
    expect(isInFolder('/music2/rock', '/music')).toBe(false);
    expect(isInFolder('/other', '/music')).toBe(false);
    expect(isInFolder('C:\\Music\\Rock', 'C:\\Music')).toBe(true);
    expect(isInFolder('/music', '/')).toBe(true);
  });

  const tracks = [
    track('a1', { filePath: '/music/Aoi Sora/Blue Horizon/01.mp3' }),
    track('a2', { filePath: '/music/Aoi Sora/Blue Horizon/02.mp3' }),
    track('a3', { filePath: '/music/Aoi Sora/Field Notes/01.flac' }),
    track('l1', { filePath: '/music/loose.mp3' }),
    track('z1', { filePath: '/music/ZARD/揺れる想い/01.mp3' }),
    track('o1', { filePath: '/downloads/single.mp3' })
  ];

  it('ライブラリフォルダを根にした木を作る（途中のフォルダも出す。曲数は下のフォルダを含む）', () => {
    const tree = buildFolderTree(tracks, ['/music/', '/empty']);

    expect(outline(tree)).toEqual([
      // ライブラリフォルダの外にある曲は、その曲のフォルダを根にする
      'downloads(1)',
      [
        'music(5)',
        [
          ['Aoi Sora(3)', ['Blue Horizon(2)', 'Field Notes(1)']],
          ['ZARD(1)', ['揺れる想い(1)']]
        ]
      ]
    ]);
    // キーは、フォルダのパス
    expect(tree[1].key).toBe('/music');
    expect(tree[1].children[0].children[0].key).toBe('/music/Aoi Sora/Blue Horizon');
  });

  it('入れ子のライブラリフォルダは、深い方を根にする', () => {
    const tree = buildFolderTree(tracks, ['/music', '/music/ZARD']);

    expect(tree.map((node) => `${node.key}(${node.count})`)).toEqual([
      '/downloads(1)',
      '/music(4)',
      '/music/ZARD(1)'
    ]);
  });

  it('Windowsのパスも、同じように木にする', () => {
    const tree = buildFolderTree(
      [track('w1', { filePath: 'C:\\Music\\Rock\\Album\\01.mp3' })],
      ['C:\\Music']
    );

    expect(outline(tree)).toEqual([['Music(1)', [['Rock(1)', ['Album(1)']]]]]);
    expect(tree[0].children[0].children[0].key).toBe('C:\\Music\\Rock\\Album');
  });

  it('フォルダとその下の曲を、場所の順に返す', () => {
    expect(ids(tracksInFolder(tracks, '/music/Aoi Sora'))).toEqual(['a1', 'a2', 'a3']);
    expect(ids(tracksInFolder(tracks, '/music/Aoi Sora/Field Notes'))).toEqual(['a3']);
    expect(tracksInFolder(tracks, '/music')).toHaveLength(5);
    expect(tracksInFolder(tracks, '/mus')).toHaveLength(0);
  });
});

describe('flattenTree', () => {
  const tree: TreeNode[] = [
    {
      key: 'a',
      label: 'A',
      count: 3,
      children: [
        {
          key: 'a/x',
          label: 'X',
          count: 2,
          children: [{ key: 'a/x/1', label: '1', count: 2, children: [] }]
        },
        { key: 'a/y', label: 'Y', count: 1, children: [] }
      ]
    },
    { key: 'b', label: 'B', count: 1, children: [] }
  ];

  it('開いている項目の子だけを並べる', () => {
    const keys = (expanded: string[]) =>
      flattenTree(tree, new Set(expanded)).map((row) => `${'  '.repeat(row.depth)}${row.node.key}`);

    expect(keys([])).toEqual(['a', 'b']);
    expect(keys(['a'])).toEqual(['a', '  a/x', '  a/y', 'b']);
    expect(keys(['a', 'a/x'])).toEqual(['a', '  a/x', '    a/x/1', '  a/y', 'b']);
    // 親を閉じていれば、子が開いていても出さない
    expect(keys(['a/x'])).toEqual(['a', 'b']);
  });

  it('子があるか・開いているかを返す', () => {
    const [a, , y] = flattenTree(tree, new Set(['a']));

    expect(a).toMatchObject({ hasChildren: true, isExpanded: true, depth: 0 });
    expect(y).toMatchObject({ hasChildren: false, isExpanded: false, depth: 1 });
  });
});

describe('年代別', () => {
  const tracks = [
    track('a', { year: 1994, artist: 'B', album: 'Y' }),
    track('b', { year: 1999, artist: 'A', album: 'X', trackNumber: 2 }),
    track('c', { year: 1999, artist: 'A', album: 'X', trackNumber: 1 }),
    track('d', { year: 2021, artist: 'C' }),
    track('e', { year: null })
  ];
  const labels = { decade: (decade: number) => `${decade}年代`, unknown: '年不明' };

  it('年代 → 年の木を、新しい方から作る（年のない曲は最後）', () => {
    const tree = buildYearTree(tracks, labels);

    expect(outline(tree)).toEqual([
      ['2020年代(1)', ['2021(1)']],
      ['1990年代(3)', ['1999(2)', '1994(1)']],
      '年不明(1)'
    ]);
    expect(tree[1].key).toBe('decade:1990');
    expect(tree[1].children[0].key).toBe('year:1999');
    expect(tree[2].key).toBe(UNKNOWN_YEAR_KEY);
  });

  it('年のない曲がなければ、「年不明」を出さない', () => {
    const tree = buildYearTree(tracks.slice(0, 4), labels);
    expect(tree.map((node) => node.key)).toEqual(['decade:2020', 'decade:1990']);
  });

  it('年代・年・年不明の曲を、アーティスト → アルバム → トラック番号の順に返す', () => {
    expect(ids(tracksInYear(tracks, 'decade:1990'))).toEqual(['c', 'b', 'a']);
    expect(ids(tracksInYear(tracks, 'year:1999'))).toEqual(['c', 'b']);
    expect(ids(tracksInYear(tracks, UNKNOWN_YEAR_KEY))).toEqual(['e']);
    expect(tracksInYear(tracks, 'year:1980')).toEqual([]);
  });
});

describe('sortTracksForBrowsing', () => {
  it('アーティスト（読みの順） → アルバム → ディスク・トラック番号の順に並べる', () => {
    const tracks = [
      track('n1', {
        artist: '中島みゆき',
        album: '歌集',
        sortTags: { ...noSortTags, artist: 'なかじまみゆき' }
      }),
      track('s2', {
        artist: '椎名林檎',
        album: '無罪モラトリアム',
        trackNumber: 2,
        sortTags: { ...noSortTags, artist: 'しいなりんご' }
      }),
      track('s1', {
        artist: '椎名林檎',
        album: '無罪モラトリアム',
        trackNumber: 1,
        sortTags: { ...noSortTags, artist: 'しいなりんご' }
      }),
      track('s3', {
        artist: '椎名林檎',
        album: '無罪モラトリアム',
        discNumber: 2,
        trackNumber: 1,
        sortTags: { ...noSortTags, artist: 'しいなりんご' }
      })
    ];

    expect(ids(sortTracksForBrowsing(tracks))).toEqual(['s1', 's2', 's3', 'n1']);
    // 元の配列は変えない
    expect(ids(tracks)).toEqual(['n1', 's2', 's1', 's3']);
  });
});
