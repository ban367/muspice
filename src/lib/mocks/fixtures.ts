/**
 * ブラウザモック用のフィクスチャデータ
 *
 * 一覧・グループ表示・プレースホルダ表示などを一通り確認できるよう、
 * 複数アルバムを持つアーティスト・アルバム未設定・メタデータなし・長いタイトル等を含める。
 * IDと日時は固定値にし、リロードごとに同じ状態から確認できるようにする。
 */
import type { Playlist, Track } from '$lib/types/models';

/** フィクスチャの基準日時（createdAtはここから1時間ずつ進む） */
const BASE_TIME = Date.parse('2026-09-01T09:00:00.000Z');
const HOUR_MS = 60 * 60 * 1000;

/** アルバムアートを持たないアルバム（プレースホルダ表示の確認用） */
export const ALBUMS_WITHOUT_ART: ReadonlySet<string> = new Set(['Quiet Rooms']);

/** モック用のUUID形式ID（バックエンドの`validate_track_id`を通る形式） */
export function mockTrackId(index: number): string {
  return `a1000000-0000-4000-8000-${index.toString(16).padStart(12, '0')}`;
}

export function mockPlaylistId(index: number): string {
  return `b2000000-0000-4000-8000-${index.toString(16).padStart(12, '0')}`;
}

interface TrackSeed {
  title: string | null;
  artist: string | null;
  album: string | null;
  genre: string | null;
  year: number | null;
  trackNumber: number | null;
  /** 秒 */
  duration: number;
  format?: 'mp3' | 'flac' | 'm4a';
  isFavorite?: boolean;
  rating?: number;
  playCount?: number;
}

// 1トラック1行の表形式で見渡せるよう、整形対象から外す
// prettier-ignore
const TRACK_SEEDS: TrackSeed[] = [
  // 複数アルバムを持つアーティスト
  { title: '青い地平線', artist: 'Aoi Sora', album: 'Blue Horizon', genre: 'J-Pop', year: 2021, trackNumber: 1, duration: 214, isFavorite: true, rating: 5, playCount: 42 },
  { title: 'Paper Planes', artist: 'Aoi Sora', album: 'Blue Horizon', genre: 'J-Pop', year: 2021, trackNumber: 2, duration: 198, rating: 4, playCount: 18 },
  { title: '雨上がりのメロディ', artist: 'Aoi Sora', album: 'Blue Horizon', genre: 'J-Pop', year: 2021, trackNumber: 3, duration: 245, playCount: 7 },
  { title: 'Summer Letter', artist: 'Aoi Sora', album: 'Blue Horizon', genre: 'J-Pop', year: 2021, trackNumber: 4, duration: 231 },
  { title: 'Morning Walk', artist: 'Aoi Sora', album: 'Field Notes', genre: 'J-Pop', year: 2024, trackNumber: 1, duration: 187, format: 'flac', rating: 3, playCount: 3 },
  { title: '観察日記', artist: 'Aoi Sora', album: 'Field Notes', genre: 'J-Pop', year: 2024, trackNumber: 2, duration: 263, format: 'flac' },
  // 日本語アーティスト・アルバム
  { title: '夜明けのシグナル', artist: 'ネオン通り', album: '夜明けのシグナル', genre: 'City Pop', year: 2019, trackNumber: 1, duration: 276, isFavorite: true, rating: 5, playCount: 31 },
  { title: 'ミッドナイト・ドライブ', artist: 'ネオン通り', album: '夜明けのシグナル', genre: 'City Pop', year: 2019, trackNumber: 2, duration: 302, isFavorite: true, playCount: 25 },
  { title: '港の灯り', artist: 'ネオン通り', album: '夜明けのシグナル', genre: 'City Pop', year: 2019, trackNumber: 3, duration: 254, playCount: 2 },
  // エレクトロニック
  { title: 'Overclock', artist: 'The Voltage', album: 'Midnight Circuit', genre: 'Electronic', year: 2023, trackNumber: 1, duration: 341, format: 'm4a', rating: 4, playCount: 12 },
  { title: 'Signal Lost', artist: 'The Voltage', album: 'Midnight Circuit', genre: 'Electronic', year: 2023, trackNumber: 2, duration: 289, format: 'm4a' },
  { title: 'Neon Rain', artist: 'The Voltage', album: 'Midnight Circuit', genre: 'Electronic', year: 2023, trackNumber: 3, duration: 412, format: 'm4a', isFavorite: true, playCount: 9 },
  // アルバム未設定（アーティスト表示では「不明なアルバム」に入る）
  { title: 'Untitled Demo', artist: 'The Voltage', album: null, genre: 'Electronic', year: null, trackNumber: null, duration: 95 },
  // アルバムアートなし（プレースホルダ表示の確認用）
  { title: 'Blue in Green Room', artist: 'Mika Hayashi', album: 'Quiet Rooms', genre: 'Jazz', year: 2018, trackNumber: 1, duration: 368, format: 'flac', rating: 4, playCount: 5 },
  { title: 'Late Night Coffee', artist: 'Mika Hayashi', album: 'Quiet Rooms', genre: 'Jazz', year: 2018, trackNumber: 2, duration: 301, format: 'flac' },
  { title: 'Soft Landing', artist: 'Mika Hayashi', album: 'Quiet Rooms', genre: 'Jazz', year: 2018, trackNumber: 3, duration: 279, format: 'flac', playCount: 1 },
  // ジャンル未設定
  { title: 'Field Recording #7', artist: 'Kenji Mori', album: 'Sketches', genre: null, year: 2025, trackNumber: 1, duration: 133 },
  { title: 'Field Recording #8', artist: 'Kenji Mori', album: 'Sketches', genre: null, year: 2025, trackNumber: 2, duration: 156 },
  // 長いタイトル（プレイヤーのマーキー表示の確認用）
  { title: 'A Remarkably Long Song Title That Should Scroll Smoothly Across The Player Bar', artist: 'The Long Names Orchestra', album: 'Extended Play', genre: 'Classical', year: 2020, trackNumber: 1, duration: 524, rating: 2 },
  // メタデータなし（ファイル名で表示される）
  { title: null, artist: null, album: null, genre: null, year: null, trackNumber: null, duration: 61 }
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
    // 再生回数の多いトラックほど最近再生されたことにする
    lastPlayedAt: playCount > 0 ? toIso(BASE_TIME + (24 + playCount) * HOUR_MS) : null,
    createdAt,
    updatedAt: createdAt
  };
}

export function createFixtureTracks(): Track[] {
  return TRACK_SEEDS.map(createTrack);
}

export function createFixturePlaylists(): Playlist[] {
  const playlist = (index: number, name: string, trackIndexes: number[]): Playlist => {
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
      createdAt,
      updatedAt: createdAt
    };
  };

  return [
    playlist(1, 'ドライブ用', [7, 8, 1, 10, 12]),
    playlist(2, '作業用BGM', [14, 15, 16, 17]),
    playlist(3, '空のプレイリスト', [])
  ];
}
