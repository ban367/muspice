/**
 * ブラウザモック用のフィクスチャデータ
 *
 * 一覧・グループ表示・プレースホルダ表示などを一通り確認できるよう、
 * 複数アルバムを持つアーティスト・アルバム未設定・メタデータなし・長いタイトル・
 * コンピレーション（アルバムアーティスト）・同じ名前のアルバム・ファイルが見つからない曲等を含める。
 * IDと日時は固定値にし、リロードごとに同じ状態から確認できるようにする。
 */
import type {
  PlayHistoryEntry,
  Playlist,
  PlaylistFolder,
  SmartRules,
  Track
} from '#lib/types/models.js';

/** フィクスチャの基準日時（createdAtはここから1時間ずつ進む） */
const BASE_TIME = Date.parse('2026-09-01T09:00:00.000Z');
const HOUR_MS = 60 * 60 * 1000;

/** アルバムアートを持たないアルバム（プレースホルダ表示の確認用） */
export const ALBUMS_WITHOUT_ART: ReadonlySet<string> = new Set(['Quiet Rooms']);

/**
 * 埋め込みの画像がなく、フォルダの画像（`cover.jpg`）を表示するアルバム
 * （アルバムアートの画面の確認用）
 */
export const ALBUMS_WITH_FOLDER_ART: ReadonlySet<string> = new Set(['Midnight Circuit']);

/** モック用のUUID形式ID（バックエンドの`validate_track_id`を通る形式） */
export function mockTrackId(index: number): string {
  return `a1000000-0000-4000-8000-${index.toString(16).padStart(12, '0')}`;
}

export function mockPlaylistId(index: number): string {
  return `b2000000-0000-4000-8000-${index.toString(16).padStart(12, '0')}`;
}

export function mockPlaylistFolderId(index: number): string {
  return `b3000000-0000-4000-8000-${index.toString(16).padStart(12, '0')}`;
}

interface TrackSeed {
  title: string | null;
  artist: string | null;
  album: string | null;
  /** アルバムアーティストのタグ（ない曲は、曲のアーティストでまとめられる） */
  albumArtist?: string;
  /** ファイルが見つからない曲（薄い表示・再生できない曲の確認用） */
  isMissing?: boolean;
  /** 並び順に使う値（読み仮名。ソート用のタグ） */
  titleSort?: string;
  artistSort?: string;
  albumSort?: string;
  genre: string | null;
  year: number | null;
  trackNumber: number | null;
  /** 秒 */
  duration: number;
  format?: 'mp3' | 'flac' | 'm4a';
  isFavorite?: boolean;
  rating?: number;
  playCount?: number;
  skipCount?: number;
}

// 1トラック1行の表形式で見渡せるよう、整形対象から外す
// prettier-ignore
const TRACK_SEEDS: TrackSeed[] = [
  // 複数アルバムを持つアーティスト
  { title: '青い地平線', titleSort: 'あおいちへいせん', artist: 'Aoi Sora', album: 'Blue Horizon', genre: 'J-Pop', year: 2021, trackNumber: 1, duration: 214, isFavorite: true, rating: 5, playCount: 42 },
  { title: 'Paper Planes', artist: 'Aoi Sora', album: 'Blue Horizon', genre: 'J-Pop', year: 2021, trackNumber: 2, duration: 198, rating: 4, playCount: 18 },
  { title: '雨上がりのメロディ', titleSort: 'あめあがりのめろでぃ', artist: 'Aoi Sora', album: 'Blue Horizon', genre: 'J-Pop', year: 2021, trackNumber: 3, duration: 245, playCount: 7 },
  { title: 'Summer Letter', artist: 'Aoi Sora', album: 'Blue Horizon', genre: 'J-Pop', year: 2021, trackNumber: 4, duration: 231 },
  { title: 'Morning Walk', artist: 'Aoi Sora', album: 'Field Notes', genre: 'J-Pop', year: 2024, trackNumber: 1, duration: 187, format: 'flac', rating: 3, playCount: 3 },
  { title: '観察日記', titleSort: 'かんさつにっき', artist: 'Aoi Sora', album: 'Field Notes', genre: 'J-Pop', year: 2024, trackNumber: 2, duration: 263, format: 'flac' },
  // 日本語アーティスト・アルバム
  { title: '夜明けのシグナル', titleSort: 'よあけのしぐなる', artist: 'ネオン通り', artistSort: 'ねおんどおり', album: '夜明けのシグナル', albumSort: 'よあけのしぐなる', genre: 'City Pop', year: 2019, trackNumber: 1, duration: 276, isFavorite: true, rating: 5, playCount: 31 },
  { title: 'ミッドナイト・ドライブ', titleSort: 'みっどないとどらいぶ', artist: 'ネオン通り', artistSort: 'ねおんどおり', album: '夜明けのシグナル', albumSort: 'よあけのしぐなる', genre: 'City Pop', year: 2019, trackNumber: 2, duration: 302, isFavorite: true, playCount: 25 },
  { title: '港の灯り', titleSort: 'みなとのあかり', artist: 'ネオン通り', artistSort: 'ねおんどおり', album: '夜明けのシグナル', albumSort: 'よあけのしぐなる', genre: 'City Pop', year: 2019, trackNumber: 3, duration: 254, playCount: 2 },
  // エレクトロニック
  { title: 'Overclock', artist: 'The Voltage', album: 'Midnight Circuit', genre: 'Electronic', year: 2023, trackNumber: 1, duration: 341, format: 'm4a', rating: 4, playCount: 12 },
  { title: 'Signal Lost', artist: 'The Voltage', album: 'Midnight Circuit', genre: 'Electronic', year: 2023, trackNumber: 2, duration: 289, format: 'm4a' },
  { title: 'Neon Rain', artist: 'The Voltage', album: 'Midnight Circuit', genre: 'Electronic', year: 2023, trackNumber: 3, duration: 412, format: 'm4a', isFavorite: true, playCount: 9 },
  // アルバム未設定（アーティスト表示では「不明なアルバム」に入る）
  { title: 'Untitled Demo', artist: 'The Voltage', album: null, genre: 'Electronic', year: null, trackNumber: null, duration: 95 },
  // アルバムアートなし（プレースホルダ表示の確認用）
  { title: 'Blue in Green Room', artist: 'Mika Hayashi', album: 'Quiet Rooms', genre: 'Jazz', year: 2018, trackNumber: 1, duration: 368, format: 'flac', rating: 4, playCount: 5 },
  { title: 'Late Night Coffee', artist: 'Mika Hayashi', album: 'Quiet Rooms', genre: 'Jazz', year: 2018, trackNumber: 2, duration: 301, format: 'flac' },
  { title: 'Soft Landing', artist: 'Mika Hayashi', album: 'Quiet Rooms', genre: 'Jazz', year: 2018, trackNumber: 3, duration: 279, format: 'flac', playCount: 1, skipCount: 4 },
  // ジャンル未設定
  { title: 'Field Recording #7', artist: 'Kenji Mori', album: 'Sketches', genre: null, year: 2025, trackNumber: 1, duration: 133 },
  { title: 'Field Recording #8', artist: 'Kenji Mori', album: 'Sketches', genre: null, year: 2025, trackNumber: 2, duration: 156 },
  // 長いタイトル（プレイヤーのマーキー表示の確認用）
  { title: 'A Remarkably Long Song Title That Should Scroll Smoothly Across The Player Bar', artist: 'The Long Names Orchestra', album: 'Extended Play', genre: 'Classical', year: 2020, trackNumber: 1, duration: 524, rating: 2 },
  // メタデータなし（ファイル名で表示される）
  { title: null, artist: null, album: null, genre: null, year: null, trackNumber: null, duration: 61 },
  // コンピレーション（曲ごとのアーティストが違い、アルバムアーティストでまとめる）
  { title: 'Harbor Lights', artist: 'ネオン通り', album: 'City Nights Collection', albumArtist: 'Various Artists', genre: 'City Pop', year: 2022, trackNumber: 1, duration: 248 },
  { title: 'Afterglow', artist: 'Aoi Sora', album: 'City Nights Collection', albumArtist: 'Various Artists', genre: 'City Pop', year: 2022, trackNumber: 2, duration: 221 },
  { title: 'Last Train', artist: 'Mika Hayashi', album: 'City Nights Collection', albumArtist: 'Various Artists', genre: 'City Pop', year: 2022, trackNumber: 3, duration: 305 },
  // フィーチャリング（アルバムアーティストは主のアーティスト）
  { title: 'Two Voices', artist: 'Aoi Sora feat. Mika Hayashi', album: 'Field Notes', albumArtist: 'Aoi Sora', genre: 'J-Pop', year: 2024, trackNumber: 3, duration: 232, format: 'flac' },
  // 別のアーティストの、同じ名前のアルバム
  { title: 'Rough Draft', artist: 'The Voltage', album: 'Sketches', genre: 'Electronic', year: 2022, trackNumber: 1, duration: 174 },
  // ファイルが見つからない曲（再スキャンで見つからなくなり、ライブラリに残している）
  { title: 'Lost Tape', artist: 'Kenji Mori', album: 'Sketches', genre: null, year: 2025, trackNumber: 3, duration: 201, isMissing: true }
];

const BITRATE_BY_FORMAT = { mp3: 320, flac: 1411, m4a: 256 } as const;

function toIso(time: number): string {
  return new Date(time).toISOString();
}

/** ファイルパスに使えない文字を置き換える */
function toPathSegment(value: string | null, fallback: string): string {
  return (value ?? fallback).replace(/[/\\:*?"<>|]/g, '_');
}

function createTrack(seed: TrackSeed, index: number): Track {
  const format = seed.format ?? 'mp3';
  const bitrate = BITRATE_BY_FORMAT[format];
  const createdAt = toIso(BASE_TIME + index * HOUR_MS);
  const playCount = seed.playCount ?? 0;
  const number = seed.trackNumber ? `${String(seed.trackNumber).padStart(2, '0')} ` : '';
  const fileName = `${number}${toPathSegment(seed.title, `audio_${index + 1}`)}.${format}`;
  const directory = `/Users/demo/Music/${toPathSegment(seed.artist, 'Unknown Artist')}/${toPathSegment(seed.album, 'Unknown Album')}`;

  return {
    id: mockTrackId(index + 1),
    filePath: `${directory}/${fileName}`,
    fileName,
    title: seed.title,
    artist: seed.artist,
    album: seed.album,
    albumArtist: seed.albumArtist ?? null,
    genre: seed.genre,
    year: seed.year,
    trackNumber: seed.trackNumber,
    discNumber: seed.trackNumber === null ? null : 1,
    duration: seed.duration,
    fileSize: Math.round((seed.duration * bitrate * 1000) / 8),
    format,
    bitrate,
    sampleRate: 44100,
    isFavorite: seed.isFavorite ?? false,
    rating: seed.rating ?? 0,
    playCount,
    skipCount: seed.skipCount ?? 0,
    // 再生回数の多いトラックほど最近再生されたことにする
    lastPlayedAt: playCount > 0 ? toIso(BASE_TIME + (24 + playCount) * HOUR_MS) : null,
    createdAt,
    updatedAt: createdAt,
    // 音量の正規化の確認用に、一部の曲にだけReplayGainのタグがあることにする
    replayGain:
      index % 3 === 2
        ? { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null }
        : {
            trackGain: -6 - (index % 5),
            trackPeak: 0.95,
            albumGain: -7,
            albumPeak: 0.99
          },
    sortTags: {
      title: seed.titleSort ?? null,
      artist: seed.artistSort ?? null,
      album: seed.albumSort ?? null,
      albumArtist: null
    },
    isMissing: seed.isMissing ?? false
  };
}

export function createFixtureTracks(): Track[] {
  return TRACK_SEEDS.map(createTrack);
}

/** 生成するトラックのIDの開始位置（フィクスチャのIDと重ならないようにする） */
const BULK_TRACK_ID_OFFSET = 100_000;
const MINUTE_MS = 60 * 1000;

/**
 * 数万曲のライブラリでの動作確認用に、トラックをまとめて生成する
 *
 * アーティスト・アルバム・ジャンルは決まった数を繰り返し使う（1アルバム12曲）。
 * 追加日時はフィクスチャより古くし、一覧の先頭にはフィクスチャが並ぶようにする。
 */
export function createBulkTracks(count: number): Track[] {
  const genres = ['Rock', 'Pop', 'Jazz', 'Classical', 'Electronic', 'Hip Hop', 'Folk', 'Ambient'];

  return Array.from({ length: Math.max(0, count) }, (_, index): Track => {
    const albumIndex = Math.floor(index / 12);
    const artist = `Bulk Artist ${String(albumIndex % 1500).padStart(4, '0')}`;
    const album = `Bulk Album ${String(albumIndex).padStart(5, '0')}`;
    const trackNumber = (index % 12) + 1;
    const title = `Bulk Track ${String(index + 1).padStart(6, '0')}`;
    const fileName = `${String(trackNumber).padStart(2, '0')} ${title}.mp3`;
    const duration = 120 + (index % 240);
    const createdAt = toIso(BASE_TIME - (index + 1) * MINUTE_MS);

    return {
      id: mockTrackId(BULK_TRACK_ID_OFFSET + index),
      filePath: `/Users/demo/Music/${artist}/${album}/${fileName}`,
      fileName,
      title,
      artist,
      album,
      albumArtist: null,
      genre: genres[albumIndex % genres.length],
      year: 1980 + (albumIndex % 45),
      trackNumber,
      discNumber: 1,
      duration,
      fileSize: Math.round((duration * BITRATE_BY_FORMAT.mp3 * 1000) / 8),
      format: 'mp3',
      bitrate: BITRATE_BY_FORMAT.mp3,
      sampleRate: 44100,
      isFavorite: false,
      rating: index % 6,
      playCount: 0,
      skipCount: 0,
      lastPlayedAt: null,
      createdAt,
      updatedAt: createdAt,
      replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
      sortTags: { title: null, artist: null, album: null, albumArtist: null },
      isMissing: false
    };
  });
}

/** 再生履歴のフィクスチャで、再生と再生の間を空ける時間（1日に数件ずつ並ぶ） */
const HISTORY_STEP_MS = 5 * HOUR_MS;

/**
 * 再生履歴のフィクスチャを作る（新しい順）
 *
 * 再生回数のある曲を、再生回数の多い順に繰り返し並べる（多い曲ほど何度も出る）。日時は`now`から
 * さかのぼって付け、画面を開いた日の「今日」「昨日」の見出しも確認できるようにする。
 * @param limit 作る件数の上限（1曲あたりは、多くても再生回数まで）
 */
export function createFixturePlayHistory(
  tracks: readonly Track[],
  now: number,
  limit = 40
): PlayHistoryEntry[] {
  const remaining = tracks
    .filter((track) => track.playCount > 0)
    .sort((a, b) => b.playCount - a.playCount)
    .map((track) => ({ trackId: track.id, count: track.playCount }));

  const trackIds: string[] = [];
  while (trackIds.length < limit && remaining.some((entry) => entry.count > 0)) {
    for (const entry of remaining) {
      if (entry.count === 0 || trackIds.length >= limit) continue;
      entry.count--;
      trackIds.push(entry.trackId);
    }
  }

  return trackIds.map((trackId, index) => ({
    id: trackIds.length - index,
    trackId,
    // 最新の再生は30分前
    playedAt: toIso(now - 30 * MINUTE_MS - index * HISTORY_STEP_MS)
  }));
}

/**
 * 曲と同じ名前の`.lrc`ファイル（時刻付きの歌詞）があることにする曲と、その中身
 *
 * 実装では、曲のファイルと同じフォルダから探す。モックでは、決まった曲にだけ持たせる。
 */
export const FIXTURE_LRC_FILES: ReadonlyMap<string, string> = new Map([
  [
    mockTrackId(1),
    [
      '[ar:Aoi Sora]',
      '[ti:青い地平線]',
      '[00:05.00]夜明け前の道を',
      '[00:12.50]ひとり歩いていた',
      '[00:20.00]遠くに見える',
      '[00:27.50]青い地平線',
      '[00:35.00]',
      '[00:45.00]風が変わるたびに',
      '[00:52.50]思い出すあの日のこと',
      '[01:00.00]まだ届かない',
      '[01:07.50]青い地平線',
      '[01:15.00][02:30.00]どこまでも続いていく',
      '[01:22.50][02:37.50]空と海のあいだ'
    ].join('\n')
  ]
]);

/** 埋め込みの歌詞（時刻なし）を持つ曲と、その歌詞 */
export const FIXTURE_EMBEDDED_LYRICS: ReadonlyMap<string, string> = new Map([
  [
    mockTrackId(2),
    ['紙ひこうきを飛ばした', '屋上の午後', '', '風にのって', 'どこまで行けるだろう'].join('\n')
  ]
]);

/** プレイリストのフォルダ（最初は空。プレイリストは、どれもフォルダの外にある） */
export function createFixturePlaylistFolders(): PlaylistFolder[] {
  return [
    {
      id: mockPlaylistFolderId(1),
      name: '気分',
      position: 0,
      createdAt: toIso(BASE_TIME + 47 * HOUR_MS)
    }
  ];
}

export function createFixturePlaylists(): Playlist[] {
  const playlist = (
    index: number,
    name: string,
    trackIndexes: number[],
    rules: SmartRules | null = null
  ): Playlist => {
    const createdAt = toIso(BASE_TIME + (48 + index) * HOUR_MS);
    return {
      id: mockPlaylistId(index),
      name,
      description: null,
      tracks: trackIndexes.map((trackIndex, position) => ({
        trackId: mockTrackId(trackIndex),
        position,
        addedAt: createdAt
      })),
      rules,
      folderId: null,
      position: index - 1,
      createdAt,
      updatedAt: createdAt
    };
  };

  return [
    playlist(1, 'ドライブ用', [7, 8, 1, 10, 12]),
    playlist(2, '作業用BGM', [14, 15, 16, 17]),
    playlist(3, '空のプレイリスト', []),
    // 自動プレイリスト（条件で曲を集める。曲は持たない）
    playlist(4, '高評価の曲', [], {
      matchMode: 'all',
      rules: [{ kind: 'number', field: 'rating', op: 'atLeast', value: 4, valueTo: null }],
      order: { field: 'rating', descending: true },
      limit: null
    })
  ];
}
