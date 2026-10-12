/**
 * ブラウザモック用のインメモリバックエンド
 *
 * Rust側のコマンド（`src-tauri/src/commands/`）の振る舞いを、フィクスチャを使って
 * メモリ上で再現する。並び順・バリデーション・エラーコード・検索の一致の仕方は実装に
 * 合わせているが、ファイルI/O（タグ書き込み・ファイル削除）は簡略化・省略している。
 *
 * ハンドラ表の型は`bindings.ts`の`commands`から導出しているため、Rust側でコマンドが
 * 追加・変更されてバインディングが再生成されると、ここが型エラーになり追随が必要になる。
 */
import {
  DEFAULT_ACCENT_COLOR,
  LIBRARY_SCAN_INTERVALS,
  MAX_CROSSFADE_SECONDS,
  type commands
} from '#lib/bindings.js';
import type {
  AlbumArtInfo,
  AlbumGroup,
  AlbumSummary,
  AppError,
  ArtistSummary,
  BulkUpdateResult,
  DeviceSyncPlan,
  DeviceSyncResult,
  DuplicateAction,
  GenreSummary,
  ImportResult,
  LibraryFolder,
  Metadata,
  NowPlayingUpdate,
  PlayHistoryEntry,
  Playlist,
  Settings,
  SyncDevice,
  Track
} from '#lib/types/models.js';
import {
  ALBUMS_WITHOUT_ART,
  ALBUMS_WITH_FOLDER_ART,
  createBulkTracks,
  createFixturePlayHistory,
  createFixturePlaylists,
  createFixtureTracks,
  mockPlaylistId,
  mockTrackId
} from './fixtures';
import { matchesSearchTerms, normalizeSearchText, searchTerms } from '#lib/utils/searchText.js';
import { createAlbumArt } from './media';
import { createMockPlaybackEngine, MOCK_OUTPUT_DEVICES } from './playbackEngine';

type Commands = typeof commands;
type CommandName = keyof Commands;
type CommandResult<K extends CommandName> = Awaited<ReturnType<Commands[K]>>;

/** `bindings.ts`の全コマンドを、同じ引数・戻り値で実装するハンドラ表 */
type MockCommandHandlers = {
  [K in CommandName]: (
    ...args: Parameters<Commands[K]>
  ) => CommandResult<K> | Promise<CommandResult<K>>;
};

export interface MockBackendOptions {
  /** バックエンドからのイベント送信（`import-progress`など） */
  emit: (event: string, payload: unknown) => void;
  /** インポート進捗を目視できるようにするための待機。テストでは即時解決に差し替える */
  sleep?: (ms: number) => Promise<void>;
  /** フィクスチャに加えて生成するトラックの数（数万曲のライブラリでの動作確認用） */
  extraTrackCount?: number;
  /**
   * 再生状態（音量・再生キューなど）の保存先。渡すと、再読み込みの後も復元できる
   * （ブラウザでは`sessionStorage`を渡す。省略時は、メモリ上にだけ持つ）
   */
  storage?: Pick<Storage, 'getItem' | 'setItem'>;
}

/** M3Uの読み込みで、選んだことにするファイル */
export type MockM3uImportMode = 'partial' | 'clean' | 'cancel';

/** アルバムアートの埋め込み（`setAlbumArt`）で、選んだことにする画像 */
export type MockAlbumArtPickMode = 'pick' | 'cancel' | 'tooLarge';

export interface MockBackend {
  /** IPCのコマンド名（snake_case）と引数オブジェクトでコマンドを実行する */
  invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
  /**
   * OSのNow Playingへ伝えたことになっている内容（`setNowPlaying`で最後に受け取ったもの）
   *
   * ブラウザにはOSの表示がないため、フロントエンドが伝えた内容をここで確認する。
   */
  nowPlaying(): NowPlayingUpdate | null;
  /**
   * M3Uの読み込み（`importM3uPlaylists`）で、選んだことにするファイルを切り替える
   *
   * - `partial`（既定）: 対応が付かない行・重複のある1つのファイル
   * - `clean`: すべての行に対応する曲がある1つのファイル
   * - `cancel`: ファイルを選ばなかった
   */
  setM3uImportMode(mode: MockM3uImportMode): void;
  /**
   * アルバムアートの埋め込み（`setAlbumArt`）で、選んだことにする画像を切り替える
   *
   * - `pick`（既定）: 埋め込める画像（選ぶたびに、違う色の画像になる）
   * - `cancel`: 画像を選ばなかった
   * - `tooLarge`: 大きすぎる画像
   */
  setAlbumArtPickMode(mode: MockAlbumArtPickMode): void;
  /** `albumart`プロトコルの代わりに、トラックのアルバムアートをdata URLで返す（アートがなければnull） */
  albumArtUrl(trackId: string): string | null;
}

/** インポート時に「見つかった」ことにするファイル数 */
const IMPORT_FILE_COUNT = 6;
/** インポートの1ファイルあたりの処理時間（進捗表示の確認用） */
const IMPORT_STEP_MS = 150;
const DEFAULT_STATS_LIMIT = 50;
/** イコライザのバンドの数（Rustの`EQ_BANDS`と同じ） */
const EQ_BAND_COUNT = 10;
/** デバイスへの同期の1曲あたりの処理時間（進捗表示の確認用） */
const SYNC_STEP_MS = 300;
/** 空き容量の判定で残しておく容量（Rustの`SPACE_MARGIN_BYTES`と同じ） */
const SYNC_SPACE_MARGIN_BYTES = 1024 * 1024;
const GIB = 1024 ** 3;

/** 転送先デバイス（一覧表示用の値のうち、容量は状態から計算する） */
interface MockSyncDevice extends Omit<SyncDevice, 'freeBytes' | 'totalBytes'> {
  /** デバイスの全体の容量と、Muspiceがコピーした曲以外が使っている容量 */
  totalBytes: number;
  otherUsedBytes: number;
  /** デバイスにコピー済みの曲（トラックID → サイズ） */
  copied: Map<string, number>;
}

/** 再生状態を保存するキー（`storage`を渡した場合） */
const PLAYBACK_STATE_KEY = 'muspice-mock:playback-state';

/** 保存する再生状態（Rustの`playback_state.rs`の`StoredState`と同じ内容） */
interface StoredPlaybackState {
  volume: number;
  shuffle: boolean;
  repeat: 'off' | 'all' | 'one';
  currentIndex: number | null;
  trackIds: string[];
  originalTrackIds: string[] | null;
}

const EMPTY_PLAYBACK_STATE: StoredPlaybackState = {
  volume: 1,
  shuffle: false,
  repeat: 'off',
  currentIndex: null,
  trackIds: [],
  originalTrackIds: null
};

/** IPCのコマンド名（`get_all_tracks`）を`commands`のキー（`getAllTracks`）へ変換する */
export function toCommandName(cmd: string): string {
  return cmd.replace(/_([a-z])/g, (_, char: string) => char.toUpperCase());
}

/** Rust側の`AppError`と同じ`{ code, message }`形式でrejectする */
function fail(code: AppError['code'], message: string): never {
  const error: AppError = { code, message };
  throw error;
}

// ---- バリデーション（src-tauri/src/validation.rs・metadata.rsに対応） ----

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function validateTrackId(id: string): void {
  if (!id) fail('VALIDATION', 'トラックIDが空です');
  if (!UUID_PATTERN.test(id)) fail('VALIDATION', '不正なトラックID形式です');
}

function validatePlaylistId(id: string): void {
  if (!UUID_PATTERN.test(id)) fail('VALIDATION', '不正なプレイリストID形式です');
}

/** Rustの`str::len`と同じくUTF-8のバイト数で数える */
function byteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

function validatePlaylistName(name: string): void {
  if (!name.trim()) fail('VALIDATION', 'プレイリスト名を入力してください');
  if (byteLength(name) > 100) fail('VALIDATION', 'プレイリスト名は100文字以内で入力してください');
  if (/[<>:"/\\|?*]/.test(name)) {
    fail('VALIDATION', 'プレイリスト名に使用できない文字が含まれています');
  }
  if (name.includes('\0')) fail('VALIDATION', '不正なプレイリスト名: Null文字が含まれています');
}

function validateFilePath(path: string): void {
  if (!path) fail('VALIDATION', 'ファイルパスが空です');
  if (path.includes('\0')) fail('VALIDATION', '不正なファイルパス: Null文字が含まれています');
  if (byteLength(path) > 4096) fail('VALIDATION', 'ファイルパスが長すぎます');
  if (path.split(/[\\/]/).includes('..')) {
    fail('VALIDATION', '不正なファイルパス: 親ディレクトリへのアクセスは許可されていません');
  }
}

function validateMetadata(metadata: Metadata): void {
  const { year } = metadata;
  if (year != null && (year < 1000 || year > 9999)) {
    fail('VALIDATION', '年は1000から9999の範囲で指定してください');
  }
  const numbers: [number | null | undefined, string][] = [
    [metadata.trackNumber, 'トラック番号'],
    [metadata.trackTotal, 'トラックの総数'],
    [metadata.discNumber, 'ディスク番号'],
    [metadata.discTotal, 'ディスクの総数'],
    [metadata.bpm, 'BPM']
  ];
  for (const [value, name] of numbers) {
    if (value != null && (value < 1 || value > 999)) {
      fail('VALIDATION', `${name}は1から999の範囲で指定してください`);
    }
  }
}

/** 文字列の項目の長さの上限（文字数。Rustの`validate_metadata_input`と同じ） */
const METADATA_TEXT_LIMITS: [keyof Metadata, string, number][] = [
  ['title', 'タイトル', 255],
  ['artist', 'アーティスト', 255],
  ['album', 'アルバム', 255],
  ['genre', 'ジャンル', 100],
  ['albumArtist', 'アルバムアーティスト', 255],
  ['composer', '作曲者', 255],
  ['grouping', 'グループ', 255],
  ['comment', 'コメント', 2000],
  ['lyrics', '歌詞', 50000],
  ['titleSort', 'タイトルの読み', 255],
  ['artistSort', 'アーティストの読み', 255],
  ['albumSort', 'アルバムの読み', 255],
  ['albumArtistSort', 'アルバムアーティストの読み', 255]
];

function validateMetadataInput(metadata: Metadata): void {
  validateMetadata(metadata);
  for (const [field, name, limit] of METADATA_TEXT_LIMITS) {
    const value = metadata[field];
    const length = typeof value === 'string' ? [...value].length : 0;
    if (length > limit) {
      fail('VALIDATION', `${name}は${limit}文字以内で入力してください（現在: ${length}文字）`);
    }
  }
}

function sanitizeSearchQuery(query: string): string {
  return query
    .replace(/[;'"\\]/g, '')
    .replaceAll('--', '')
    .replaceAll('/*', '')
    .replaceAll('*/', '')
    .trim();
}

// ---- 並び替え・グループ化（src-tauri/src/repository.rsに対応） ----

type SortKey = string | number | null;

/** SQLiteの`ORDER BY ... ASC`と同様に、NULLを先頭にして昇順比較する */
function compareAsc(a: SortKey, b: SortKey): number {
  if (a === b) return 0;
  if (a === null) return -1;
  if (b === null) return 1;
  return a < b ? -1 : 1;
}

function orderBy<T>(...keys: ((item: T) => SortKey)[]): (a: T, b: T) => number {
  return (a, b) => {
    for (const key of keys) {
      const result = compareAsc(key(a), key(b));
      if (result !== 0) return result;
    }
    return 0;
  };
}

/** 一覧を並べる時に比べる文字列（並び順に使う値。なければ名前。Rustの`sort_key`と同じ） */
const sortKeyOf = (item: { name: string; sortName?: string | null }) =>
  normalizeSearchText(item.sortName ?? item.name);

const bySortName = (
  a: { name: string; sortName?: string | null },
  b: { name: string; sortName?: string | null }
) => compareAsc(sortKeyOf(a), sortKeyOf(b));

/** 出現順を保ったままグループ化する */
function groupBy<T>(items: T[], key: (item: T) => string): Map<string, T[]> {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const name = key(item);
    const group = groups.get(name);
    if (group) {
      group.push(item);
    } else {
      groups.set(name, [item]);
    }
  }
  return groups;
}

/** アーティストの詳細で、アルバムのない曲をまとめるアルバムの名前（Rustの`UNKNOWN_ALBUM`と同じ） */
const UNKNOWN_ALBUM = '不明なアルバム';

/** アルバム・アーティストの一覧をまとめるアーティスト（Rustの`ALBUM_ARTIST`と同じ） */
const groupArtist = (track: Track) => track.albumArtist ?? track.artist;

/** まとめたアーティストの、並び順に使う値（Rustの`ALBUM_ARTIST_SORT`と同じ） */
const groupArtistSort = (track: Track) =>
  track.albumArtist === null
    ? track.sortTags.artist
    : (track.sortTags.albumArtist ??
      (track.artist === track.albumArtist ? track.sortTags.artist : null));

/** 並び順に使う値のうち、最初に見つかったもの（なければnull） */
const firstSortName = (tracks: Track[], pick: (track: Track) => string | null) =>
  tracks.map(pick).find((value) => value !== null) ?? null;

/** アルバムの中の曲の並び（ディスク番号 → トラック番号 → タイトル。Rustの`ALBUM_TRACK_ORDER`と同じ） */
const albumTrackOrder: ((track: Track) => SortKey)[] = [
  (t) => t.discNumber ?? 1,
  (t) => t.trackNumber,
  (t) => t.title
];

function sumDuration(tracks: Track[]): number {
  return tracks.reduce((total, track) => total + (track.duration ?? 0), 0);
}

function toAlbumSummary(name: string, artist: string | null, tracks: Track[]): AlbumSummary {
  return {
    name,
    sortName: firstSortName(tracks, (track) => track.sortTags.album),
    artist,
    trackCount: tracks.length,
    totalDuration: sumDuration(tracks),
    representativeTrackId: tracks[0]?.id ?? ''
  };
}

export function createMockBackend(options: MockBackendOptions): MockBackend {
  const sleep =
    options.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));

  let tracks: Track[] = [
    ...createFixtureTracks(),
    ...createBulkTracks(options.extraTrackCount ?? 0)
  ];
  let playlists: Playlist[] = createFixturePlaylists();
  // お気に入りにした日時（フィクスチャのお気に入りは、曲を追加した日時にする）
  const favoritedAt = new Map(
    tracks.filter((track) => track.isFavorite).map((track) => [track.id, track.createdAt])
  );
  // 再生履歴（新しい順）
  let playHistory: PlayHistoryEntry[] = createFixturePlayHistory(tracks, Date.now());
  let currentTrackId: string | null = null;
  let nowPlaying: NowPlayingUpdate | null = null;
  // ファイルのタグだけにある項目（作曲者・コメント・歌詞など。トラックID → 値）
  const fileOnlyTags = new Map<string, Metadata>();
  let m3uImportMode: MockM3uImportMode = 'partial';
  // アプリで書き換えたアルバムアート（トラックID → 埋め込んだ画像の番号。nullは取り除いた）
  const embeddedArt = new Map<string, number | null>();
  let albumArtPickMode: MockAlbumArtPickMode = 'pick';
  let pickedArtCount = 0;
  // 前回の再生状態（保存先に壊れた内容があれば使わない）
  let playbackState: StoredPlaybackState = EMPTY_PLAYBACK_STATE;
  try {
    const stored = options.storage?.getItem(PLAYBACK_STATE_KEY);
    if (stored) playbackState = { ...EMPTY_PLAYBACK_STATE, ...JSON.parse(stored) };
  } catch {
    playbackState = EMPTY_PLAYBACK_STATE;
  }
  let settings: Settings = {
    language: 'ja',
    startupPage: 'lastOpened',
    theme: 'dark',
    accentColor: DEFAULT_ACCENT_COLOR,
    volumeNormalization: 'off',
    gaplessPlayback: true,
    crossfadeSeconds: 0,
    outputDeviceId: null,
    watchLibraryFolders: false,
    libraryScanIntervalMinutes: 0
  };
  // ライブラリフォルダ（existsは「フォルダが見つかるか」。外付けドライブが外れた状態を再現する）
  let libraryFolders: Omit<LibraryFolder, 'trackCount'>[] = [
    {
      id: crypto.randomUUID(),
      path: '/Users/demo/Music',
      exists: true,
      addedAt: '2026-01-01T00:00:00.000Z',
      lastScannedAt: '2026-01-01T00:00:00.000Z'
    },
    {
      id: crypto.randomUUID(),
      path: '/Volumes/External/Music',
      exists: false,
      addedAt: '2026-01-02T00:00:00.000Z',
      lastScannedAt: null
    }
  ];

  // 転送先デバイス（connectedは「接続されているか」。外れた状態も再現する）
  const copiedTracks = (indexes: number[]) =>
    new Map(
      indexes.map((index) => {
        const track = tracks.find((t) => t.id === mockTrackId(index));
        return [mockTrackId(index), track?.fileSize ?? 0] as const;
      })
    );
  let syncDevices: MockSyncDevice[] = [
    {
      id: crypto.randomUUID(),
      name: 'SDカード',
      path: '/Volumes/SDCARD/Music',
      syncAll: false,
      playlistIds: [mockPlaylistId(1)],
      removeUnselected: true,
      connected: true,
      createdAt: '2026-01-03T00:00:00.000Z',
      lastSyncedAt: '2026-01-04T00:00:00.000Z',
      totalBytes: 32 * GIB,
      otherUsedBytes: 20 * GIB,
      // プレイリストの5曲のうち2曲と、プレイリストから外した1曲がコピー済み
      copied: copiedTracks([7, 8, 2])
    },
    {
      id: crypto.randomUUID(),
      name: 'Walkman',
      path: '/Volumes/WALKMAN/MUSIC',
      syncAll: true,
      playlistIds: [],
      removeUnselected: true,
      connected: false,
      createdAt: '2026-01-02T00:00:00.000Z',
      lastSyncedAt: null,
      totalBytes: 16 * GIB,
      otherUsedBytes: 0,
      copied: new Map()
    }
  ];
  let isSyncRunning = false;
  let isSyncCancelled = false;

  const now = () => new Date().toISOString();
  const newestFirst = (a: Track, b: Track) => compareAsc(b.createdAt, a.createdAt);
  const tracksWhere = (predicate: (track: Track) => boolean) =>
    tracks.filter(predicate).sort(newestFirst);
  const matches = (value: string | null, expected: string | null | undefined) =>
    expected == null || value === expected;

  function findTrack(id: string): Track {
    const track = tracks.find((t) => t.id === id);
    if (!track) fail('NOT_FOUND', 'トラックが見つかりません');
    return track;
  }

  // 再生エンジン（音は鳴らさず、再生位置と曲の切り替わりを時計に合わせて進める）
  const playbackEngine = createMockPlaybackEngine({
    emit: (event) => options.emit('playback-event', event),
    crossfadeSeconds: () => settings.crossfadeSeconds,
    durationOf: (trackId) => {
      validateTrackId(trackId);
      const track = findTrack(trackId);
      if (track.isMissing) fail('NOT_FOUND', `ファイルが見つかりません: ${track.filePath}`);
      return track.duration;
    }
  });

  /** トラックを、値のある項目ごとにまとめる（値がnullのトラックは含めない） */
  const tracksBy = (value: (track: Track) => string | null) =>
    groupBy(
      tracks.filter((track) => value(track) !== null),
      (track) => value(track) ?? ''
    );

  /** アーティストの曲を、アルバムごとにまとめる（アルバム名の順。アルバムのない曲は`UNKNOWN_ALBUM`にまとめる） */
  function toArtistAlbums(artist: string, items: Track[]): AlbumGroup[] {
    const sorted = [...items].sort(orderBy((t) => t.album ?? UNKNOWN_ALBUM, ...albumTrackOrder));
    return [...groupBy(sorted, (track) => track.album ?? UNKNOWN_ALBUM)]
      .map(([name, albumTracks]) => ({
        ...toAlbumSummary(name, artist, albumTracks),
        tracks: albumTracks
      }))
      .sort(bySortName);
  }

  /** ジャンルの曲を並べる（アルバムをまとめるアーティスト → アルバム → アルバムの中の並び） */
  const sortGenreTracks = (items: Track[]) =>
    [...items].sort(orderBy(groupArtist, (t) => t.album, ...albumTrackOrder));

  function findPlaylist(id: string): Playlist {
    const playlist = playlists.find((p) => p.id === id);
    if (!playlist) fail('NOT_FOUND', 'プレイリストが見つかりません');
    return playlist;
  }

  function uniqueValues(key: 'artist' | 'album' | 'genre'): string[] {
    const values = new Set<string>();
    for (const track of tracks) {
      const value = track[key];
      if (value !== null) values.add(value);
    }
    return [...values].sort(compareAsc);
  }

  function validateTrackIdsForDeletion(trackIds: string[]): void {
    if (trackIds.length === 0) fail('VALIDATION', '削除するトラックが指定されていません');
    trackIds.forEach(validateTrackId);
  }

  function removeTracks(trackIds: string[]): number {
    const targets = new Set(trackIds);
    const before = tracks.length;
    tracks = tracks.filter((track) => !targets.has(track.id));
    // ライブラリから外した曲の再生履歴は消える（DBの外部キーと同じ）
    playHistory = playHistory.filter((entry) => !targets.has(entry.trackId));
    // playlist_tracksのON DELETE CASCADEに相当（実装と同じくpositionは詰めない）
    for (const playlist of playlists) {
      playlist.tracks = playlist.tracks.filter((entry) => !targets.has(entry.trackId));
    }
    return before - tracks.length;
  }

  /**
   * update_track_metadataと同様、title/artist/album/genre/yearを丸ごと置き換える
   * （実装はファイルのタグへ書き込むが、モックはライブラリの値だけを変える）
   */
  function updateTrackMetadata(trackId: string, metadata: Metadata): null {
    validateTrackId(trackId);
    validateMetadataInput(metadata);
    const track = tracks.find((t) => t.id === trackId);
    if (!track) fail('NOT_FOUND', '指定されたトラックが見つかりません');
    // 1曲の編集は、すべての項目を置き換える（値のない項目は空にする）
    track.title = metadata.title ?? null;
    track.artist = metadata.artist ?? null;
    track.album = metadata.album ?? null;
    track.genre = metadata.genre ?? null;
    track.year = metadata.year ?? null;
    track.albumArtist = metadata.albumArtist ?? null;
    track.trackNumber = metadata.trackNumber ?? null;
    track.discNumber = metadata.discNumber ?? null;
    track.sortTags = {
      title: metadata.titleSort ?? null,
      artist: metadata.artistSort ?? null,
      album: metadata.albumSort ?? null,
      albumArtist: metadata.albumArtistSort ?? null
    };
    track.updatedAt = now();
    // データベースに保存しない項目（作曲者・歌詞など）は、ファイルのタグの代わりに持っておく
    fileOnlyTags.set(trackId, pickFileOnlyTags(metadata));
    return null;
  }

  /** ファイルのタグだけにある項目（データベースに保存しない項目）を取り出す */
  function pickFileOnlyTags(metadata: Metadata): Metadata {
    const { composer, trackTotal, discTotal, grouping, bpm, compilation, comment, lyrics } =
      metadata;
    return { composer, trackTotal, discTotal, grouping, bpm, compilation, comment, lyrics };
  }

  /** 曲のタグ（編集画面で扱うすべての項目）。モックでは、曲の情報と、編集で入れた値から作る */
  /**
   * トラックのアルバムアート（画像のdata URLと、その情報。アートがなければ両方null）
   *
   * 埋め込みの画像を優先し、なければフォルダの画像を使う。
   */
  function albumArtOf(track: Track): { url: string | null; info: AlbumArtInfo | null } {
    const none = { url: null, info: null };
    const embedded = (seed: string, mimeType: string, size: number, side: number) => ({
      url: createAlbumArt(seed),
      info: {
        source: 'embedded' as const,
        fileName: null,
        mimeType,
        size,
        width: side,
        height: side
      }
    });

    const picked = embeddedArt.get(track.id);
    if (typeof picked === 'number') {
      return embedded(`picked-${picked}`, 'image/png', 512_000 + picked * 1_000, 1200);
    }
    const album = track.album;
    if (album !== null && ALBUMS_WITH_FOLDER_ART.has(album)) {
      return {
        url: createAlbumArt(`folder-${album}`),
        info: {
          source: 'folder',
          fileName: 'cover.jpg',
          mimeType: 'image/jpeg',
          size: 214_530,
          width: 1000,
          height: 1000
        }
      };
    }
    // 取り除いた曲・アルバムのない曲・アートのないアルバムの曲
    if (picked === null || album === null || ALBUMS_WITHOUT_ART.has(album)) return none;
    return embedded(album, 'image/jpeg', 84_213, 600);
  }

  /** 埋め込みの画像のサイズ（埋め込みの画像がなければ0） */
  function embeddedArtSize(track: Track): number {
    const info = albumArtOf(track).info;
    return info?.source === 'embedded' ? info.size : 0;
  }

  function validateAlbumArtTargets(trackIds: string[]): void {
    if (trackIds.length === 0) fail('VALIDATION', 'トラックIDが指定されていません');
    trackIds.forEach(validateTrackId);
  }

  /**
   * 各トラックのアルバムアートを書き換える（`write`は、書き換えたかを返す）
   *
   * ファイルが見つからない・ライブラリにないトラックは、書き込めなかった曲として数える。
   */
  function writeAlbumArt(trackIds: string[], write: (track: Track) => boolean): BulkUpdateResult {
    const result: BulkUpdateResult = { updatedCount: 0, failedCount: 0, errors: [] };
    for (const trackId of trackIds) {
      const track = tracks.find((t) => t.id === trackId);
      if (!track || track.isMissing) {
        result.failedCount++;
        result.errors.push(
          track
            ? `${track.filePath}: ファイルのオープンに失敗しました: No such file`
            : `トラックが見つかりません: ${trackId}`
        );
        continue;
      }
      const sizeBefore = embeddedArtSize(track);
      if (!write(track)) continue;
      // 画像の分だけ、ファイルのサイズが変わる
      track.fileSize += embeddedArtSize(track) - sizeBefore;
      result.updatedCount++;
    }
    return result;
  }

  function getTrackTags(trackId: string): Metadata {
    validateTrackId(trackId);
    const track = findTrack(trackId);
    if (track.isMissing) fail('METADATA', 'ファイルのオープンに失敗しました: No such file');
    const tags: Metadata = { ...fileOnlyTags.get(trackId) };
    if (track.title !== null) tags.title = track.title;
    if (track.artist !== null) tags.artist = track.artist;
    if (track.album !== null) tags.album = track.album;
    if (track.albumArtist !== null) tags.albumArtist = track.albumArtist;
    if (track.genre !== null) tags.genre = track.genre;
    if (track.year !== null) tags.year = track.year;
    if (track.trackNumber !== null) tags.trackNumber = track.trackNumber;
    if (track.discNumber !== null) tags.discNumber = track.discNumber;
    tags.titleSort = track.sortTags.title;
    tags.artistSort = track.sortTags.artist;
    tags.albumSort = track.sortTags.album;
    tags.albumArtistSort = track.sortTags.albumArtist;
    // 値のない項目は、キーごと除く（Rust側は、タグにない項目を返さない）
    return Object.fromEntries(
      Object.entries(tags).filter(([, value]) => value !== undefined && value !== null)
    );
  }

  /**
   * フォルダのインポート
   *
   * 実際のスキャンの代わりに、フォルダパスから決定的なファイル一覧を生成する。
   * 同じフォルダを再度インポートすると、2回目は重複として扱われる。
   */
  async function importFolder(
    folderPath: string,
    duplicateAction: DuplicateAction
  ): Promise<ImportResult> {
    validateFilePath(folderPath);
    const folder = normalizeFolderPath(folderPath);
    const album = folder.split(/[\\/]/).filter(Boolean).pop() || 'Imported';
    let importedCount = 0;
    let skippedCount = 0;

    for (let index = 0; index < IMPORT_FILE_COUNT; index++) {
      const fileName = `Mock Track ${String(index + 1).padStart(2, '0')}.mp3`;
      const filePath = `${folderPrefix(folder)}${fileName}`;
      options.emit('import-progress', {
        current: index + 1,
        total: IMPORT_FILE_COUNT,
        currentFile: fileName
      });
      await sleep(IMPORT_STEP_MS);

      const existing = tracks.find((track) => track.filePath === filePath);
      if (existing && duplicateAction === 'Skip') {
        skippedCount++;
        continue;
      }

      const timestamp = now();
      if (existing) {
        existing.updatedAt = timestamp;
      } else {
        const duration = 150 + index * 17;
        tracks.push({
          id: crypto.randomUUID(),
          filePath,
          fileName,
          title: `Mock Track ${index + 1}`,
          artist: 'Mock Importer',
          album,
          albumArtist: null,
          genre: 'Demo',
          year: 2026,
          trackNumber: index + 1,
          discNumber: 1,
          duration,
          fileSize: duration * 40_000,
          format: 'mp3',
          bitrate: 320,
          sampleRate: 44100,
          isFavorite: false,
          rating: 0,
          playCount: 0,
          skipCount: 0,
          lastPlayedAt: null,
          createdAt: timestamp,
          updatedAt: timestamp,
          replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
          sortTags: { title: null, artist: null, album: null, albumArtist: null },
          isMissing: false
        });
      }
      importedCount++;
    }

    // Rustと同じく、インポートの後にライブラリが変わったことを知らせる
    registerLibraryFolder(folder);
    options.emit('library-changed', null);
    // 実ファイルは存在しないため、移動・改名されたファイルの対応付けは再現しない
    return { importedCount, skippedCount, relinkedCount: 0, errorCount: 0, errors: [] };
  }

  // ---- ライブラリフォルダ（src-tauri/src/library_folder.rsに対応） ----

  /** フォルダのパスの末尾の区切り文字を除く（ルートは残す。Rustの`normalize_folder_path`と同じ） */
  function normalizeFolderPath(path: string): string {
    const trimmed = path.replace(/[\\/]+$/, '');
    return trimmed === '' || trimmed.endsWith(':') ? path : trimmed;
  }
  /** フォルダ内のファイルのパスの接頭辞（Rustの`track_path_prefix`と同じ） */
  const folderPrefix = (folder: string) => (/[\\/]$/.test(folder) ? folder : `${folder}/`);
  const isSameOrWithin = (path: string, folder: string) =>
    path === folder || path.startsWith(folderPrefix(folder));
  const tracksUnder = (folder: string) =>
    tracks.filter((track) => track.filePath.startsWith(folderPrefix(folder)));

  /** インポートしたフォルダを記録する（フォルダ同士は入れ子にしない） */
  function registerLibraryFolder(path: string): void {
    const same = libraryFolders.find((folder) => folder.path === path);
    if (same) {
      same.lastScannedAt = now();
      return;
    }
    if (libraryFolders.some((folder) => isSameOrWithin(path, folder.path))) return;
    libraryFolders = libraryFolders.filter((folder) => !isSameOrWithin(folder.path, path));
    libraryFolders.push({
      id: crypto.randomUUID(),
      path,
      exists: true,
      addedAt: now(),
      lastScannedAt: now()
    });
  }

  function findLibraryFolder(folderId: string) {
    if (!UUID_PATTERN.test(folderId)) fail('VALIDATION', '不正なライブラリフォルダID形式です');
    const folder = libraryFolders.find((f) => f.id === folderId);
    if (!folder) fail('NOT_FOUND', 'ライブラリフォルダが見つかりません');
    return folder;
  }

  // ---- 転送先デバイス（src-tauri/src/device.rs・commands/devices.rsに対応） ----
  //
  // 実ファイルは存在しないため、デバイス上の曲は「コピー済みのトラックIDの集合」で表す
  // （配置の決定・リネーム・プレイリストのファイルの書き出しは再現しない）。

  function findSyncDevice(deviceId: string): MockSyncDevice {
    if (!UUID_PATTERN.test(deviceId)) fail('VALIDATION', '不正なデバイスID形式です');
    const device = syncDevices.find((d) => d.id === deviceId);
    if (!device) fail('NOT_FOUND', 'デバイスが見つかりません');
    return device;
  }

  /** デバイス名をバリデーションし、前後の空白を除いた名前を返す */
  function validateDeviceName(name: string): string {
    const trimmed = name.trim();
    if (!trimmed) fail('VALIDATION', 'デバイス名を入力してください');
    if (byteLength(trimmed) > 100) fail('VALIDATION', 'デバイス名は100文字以内で入力してください');
    if (/\p{Cc}/u.test(trimmed)) fail('VALIDATION', 'デバイス名に使用できない文字が含まれています');
    return trimmed;
  }

  /** 転送先にするフォルダを検証する（ライブラリフォルダと重なるフォルダは使えない） */
  function validateDeviceFolder(folderPath: string): string {
    validateFilePath(folderPath);
    const path = normalizeFolderPath(folderPath);
    const overlaps = libraryFolders.some(
      (folder) => isSameOrWithin(path, folder.path) || isSameOrWithin(folder.path, path)
    );
    if (overlaps) fail('VALIDATION', 'ライブラリフォルダと重なるフォルダは、転送先にできません');
    return path;
  }

  /** デバイスに同期するプレイリスト（削除済みのプレイリストは含めない） */
  const devicePlaylists = (device: MockSyncDevice) =>
    playlists.filter((playlist) => device.playlistIds.includes(playlist.id));

  const usedBytes = (device: MockSyncDevice) =>
    device.otherUsedBytes + [...device.copied.values()].reduce((sum, size) => sum + size, 0);

  function toSyncDevice(device: MockSyncDevice): SyncDevice {
    return {
      id: device.id,
      name: device.name,
      path: device.path,
      syncAll: device.syncAll,
      playlistIds: devicePlaylists(device).map((playlist) => playlist.id),
      removeUnselected: device.removeUnselected,
      connected: device.connected,
      freeBytes: device.connected ? device.totalBytes - usedBytes(device) : null,
      totalBytes: device.connected ? device.totalBytes : null,
      createdAt: device.createdAt,
      lastSyncedAt: device.lastSyncedAt
    };
  }

  /** デバイスに同期する曲（全曲か、選んだプレイリストの曲） */
  function tracksToSync(device: MockSyncDevice): Track[] {
    if (device.syncAll) return tracks;
    const ids = new Set(
      devicePlaylists(device).flatMap((playlist) => playlist.tracks.map((t) => t.trackId))
    );
    return tracks.filter((track) => ids.has(track.id));
  }

  /** 同期でコピー・削除する曲を決める */
  function planSync(device: MockSyncDevice) {
    if (!device.connected) {
      fail(
        'NOT_FOUND',
        `デバイスが接続されていません: ${device.path}（接続されているか確認してください）`
      );
    }
    const selected = tracksToSync(device);
    const selectedIds = new Set(selected.map((track) => track.id));
    const toCopy = selected.filter((track) => !device.copied.has(track.id));
    const stale = [...device.copied.keys()].filter((id) => !selectedIds.has(id));
    const toDelete = device.removeUnselected ? stale : [];

    const copyBytes = toCopy.reduce((sum, track) => sum + track.fileSize, 0);
    const deleteBytes = toDelete.reduce((sum, id) => sum + (device.copied.get(id) ?? 0), 0);
    const freeBytes = device.totalBytes - usedBytes(device);
    const requiredBytes = Math.max(copyBytes - deleteBytes, 0);
    const summary: DeviceSyncPlan = {
      copyCount: toCopy.length,
      copyBytes,
      deleteCount: toDelete.length,
      deleteBytes,
      renameCount: 0,
      unchangedCount: device.copied.size - toDelete.length,
      playlistCount: devicePlaylists(device).length,
      missingSourceCount: 0,
      freeBytes,
      requiredBytes,
      hasEnoughSpace: requiredBytes === 0 || requiredBytes + SYNC_SPACE_MARGIN_BYTES <= freeBytes
    };
    return { toCopy, toDelete, summary };
  }

  /** デバイスへ同期する（1曲ずつ進捗を送り、中止の要求があれば次の曲の前で止める） */
  async function runDeviceSync(deviceId: string): Promise<DeviceSyncResult> {
    const device = findSyncDevice(deviceId);
    if (isSyncRunning) fail('LOCK', '他のデバイスへの同期が実行中です');
    const { toCopy, toDelete, summary } = planSync(device);
    if (!device.syncAll && devicePlaylists(device).length === 0) {
      fail('VALIDATION', '同期する対象（全曲またはプレイリスト）を選択してください');
    }
    if (!summary.hasEnoughSpace) fail('VALIDATION', 'デバイスの空き容量が足りません');

    isSyncRunning = true;
    isSyncCancelled = false;
    try {
      for (const id of toDelete) device.copied.delete(id);

      let copiedCount = 0;
      let bytesDone = 0;
      const progress = (current: number, currentFile: string) =>
        options.emit('device-sync-progress', {
          deviceId,
          current,
          total: toCopy.length,
          bytesDone,
          bytesTotal: summary.copyBytes,
          currentFile
        });
      for (const [index, track] of toCopy.entries()) {
        if (isSyncCancelled) break;
        progress(index, track.fileName);
        await sleep(SYNC_STEP_MS);
        device.copied.set(track.id, track.fileSize);
        bytesDone += track.fileSize;
        copiedCount++;
      }

      const cancelled = isSyncCancelled;
      if (!cancelled) {
        progress(toCopy.length, '');
        device.lastSyncedAt = now();
      }
      return {
        copiedCount,
        deletedCount: toDelete.length,
        renamedCount: 0,
        playlistCount: cancelled ? 0 : summary.playlistCount,
        errorCount: 0,
        errors: [],
        cancelled
      };
    } finally {
      isSyncRunning = false;
    }
  }

  const handlers: MockCommandHandlers = {
    importFolder,
    getAllTracks: () => tracksWhere(() => true),
    // 実装と同じく、空白で区切った語をすべて含む曲を返す（途中の一致。大文字と小文字・
    // 全角と半角・ひらがなとカタカナを同じとみなす）
    searchTracks: (query) => {
      const terms = searchTerms(sanitizeSearchQuery(query));
      if (terms.length === 0) return [];
      return tracksWhere((track) =>
        matchesSearchTerms(
          [track.title, track.artist, track.album, track.genre, track.albumArtist],
          terms
        )
      );
    },
    filterTracks: (filters) =>
      tracksWhere(
        (track) =>
          matches(track.artist, filters.artist) &&
          matches(track.album, filters.album) &&
          matches(track.genre, filters.genre)
      ),
    getUniqueArtists: () => uniqueValues('artist'),
    getUniqueAlbums: () => uniqueValues('album'),
    getUniqueGenres: () => uniqueValues('genre'),
    // 「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめる
    getAlbums: () => {
      const sorted = tracks
        .filter((track) => track.album !== null)
        .sort(orderBy((t) => t.album, groupArtist, ...albumTrackOrder));
      return [...groupBy(sorted, (track) => JSON.stringify([track.album, groupArtist(track)]))]
        .map(([, items]) => toAlbumSummary(items[0].album ?? '', groupArtist(items[0]), items))
        .sort(bySortName);
    },
    getAlbumTracks: (album, artist) =>
      tracks
        .filter((track) => track.album === album && groupArtist(track) === artist)
        .sort(orderBy(...albumTrackOrder)),
    getArtists: () =>
      [...tracksBy(groupArtist)]
        .map(([name, items]): ArtistSummary => {
          const albums = toArtistAlbums(name, items);
          return {
            name,
            sortName: firstSortName(items, groupArtistSort),
            albumCount: albums.length,
            trackCount: items.length,
            totalDuration: sumDuration(items),
            // 詳細で最初に表示するアルバムの最初の曲
            representativeTrackId: albums[0]?.representativeTrackId ?? ''
          };
        })
        .sort(bySortName),
    getArtistAlbums: (artist) =>
      toArtistAlbums(
        artist,
        tracks.filter((track) => groupArtist(track) === artist)
      ),
    getGenres: () =>
      [...tracksBy((track) => track.genre)]
        .map(([name, items]): GenreSummary => ({
          name,
          trackCount: items.length,
          totalDuration: sumDuration(items),
          representativeTrackId: sortGenreTracks(items)[0]?.id ?? ''
        }))
        .sort(bySortName),
    getGenreTracks: (genre) => sortGenreTracks(tracks.filter((track) => track.genre === genre)),
    getTrackTags,
    updateTrackMetadata,
    updateMultipleTracksMetadata: (trackIds, metadata) => {
      if (trackIds.length === 0) fail('VALIDATION', 'トラックIDが指定されていません');
      trackIds.forEach(validateTrackId);
      validateMetadataInput(metadata);
      // トランザクション相当: 全件の存在を確認してから更新する
      const targets = trackIds.map((id) => {
        const track = tracks.find((t) => t.id === id);
        if (!track) fail('NOT_FOUND', `トラックが見つかりません: ${id}`);
        return track;
      });
      const timestamp = now();
      const { title, artist, album, genre, year, albumArtist, trackNumber, discNumber } = metadata;
      const { titleSort, artistSort, albumSort, albumArtistSort } = metadata;
      const hasChanges = [
        title,
        artist,
        album,
        genre,
        year,
        albumArtist,
        trackNumber,
        discNumber,
        titleSort,
        artistSort,
        albumSort,
        albumArtistSort
      ].some((value) => value != null);
      // 値のある項目だけを変える
      const fileOnly = Object.fromEntries(
        Object.entries(pickFileOnlyTags(metadata)).filter(([, value]) => value != null)
      );
      for (const track of targets) {
        if (title != null) track.title = title;
        if (artist != null) track.artist = artist;
        if (album != null) track.album = album;
        if (genre != null) track.genre = genre;
        if (year != null) track.year = year;
        if (albumArtist != null) track.albumArtist = albumArtist;
        if (trackNumber != null) track.trackNumber = trackNumber;
        if (discNumber != null) track.discNumber = discNumber;
        track.sortTags = {
          title: titleSort ?? track.sortTags.title,
          artist: artistSort ?? track.sortTags.artist,
          album: albumSort ?? track.sortTags.album,
          albumArtist: albumArtistSort ?? track.sortTags.albumArtist
        };
        if (hasChanges) track.updatedAt = timestamp;
        const merged: Metadata = { ...fileOnlyTags.get(track.id), ...fileOnly };
        // コンピレーションの印を外す指定は、項目ごと取り除く
        if (merged.compilation === false) delete merged.compilation;
        fileOnlyTags.set(track.id, merged);
      }
      // モックではファイルへの書き込みに失敗しない
      return { updatedCount: targets.length, failedCount: 0, errors: [] };
    },
    getAlbumArtInfo: (trackId) => {
      validateTrackId(trackId);
      const track = tracks.find((t) => t.id === trackId);
      if (!track) fail('NOT_FOUND', `トラックが見つかりません: ${trackId}`);
      return albumArtOf(track).info;
    },
    setAlbumArt: (trackIds) => {
      // 画像を選ぶダイアログ（Rust側が開く）の代わりに、決まった画像を選んだことにする
      validateAlbumArtTargets(trackIds);
      if (albumArtPickMode === 'cancel') return null;
      if (albumArtPickMode === 'tooLarge') fail('VALIDATION', '画像が大きすぎます（10MBまで）');
      const picture = ++pickedArtCount;
      return writeAlbumArt(trackIds, (track) => {
        embeddedArt.set(track.id, picture);
        return true;
      });
    },
    removeAlbumArt: (trackIds) => {
      validateAlbumArtTargets(trackIds);
      return writeAlbumArt(trackIds, (track) => {
        // 埋め込みの画像がない曲は、書き換えない
        if (albumArtOf(track).info?.source !== 'embedded') return false;
        embeddedArt.set(track.id, null);
        return true;
      });
    },
    createPlaylist: (name) => {
      validatePlaylistName(name);
      const timestamp = now();
      const playlist: Playlist = {
        id: crypto.randomUUID(),
        name,
        description: null,
        tracks: [],
        createdAt: timestamp,
        updatedAt: timestamp
      };
      playlists.push(playlist);
      return playlist;
    },
    getPlaylists: () =>
      [...playlists]
        .sort((a, b) => compareAsc(b.createdAt, a.createdAt))
        .map((playlist) => ({
          ...playlist,
          tracks: [...playlist.tracks].sort((a, b) => a.position - b.position)
        })),
    // 実装と同じく、見つからないプレイリストは空の一覧を返す
    getPlaylistTracks: (playlistId) => {
      validatePlaylistId(playlistId);
      const playlist = playlists.find((p) => p.id === playlistId);
      if (!playlist) return [];
      const byId = new Map(tracks.map((track) => [track.id, track]));
      return [...playlist.tracks]
        .sort((a, b) => a.position - b.position)
        .map((entry) => byId.get(entry.trackId))
        .filter((track): track is Track => track !== undefined);
    },
    deletePlaylist: (playlistId) => {
      validatePlaylistId(playlistId);
      findPlaylist(playlistId);
      playlists = playlists.filter((playlist) => playlist.id !== playlistId);
      return null;
    },
    renamePlaylist: (playlistId, name) => {
      validatePlaylistId(playlistId);
      const trimmed = name.trim();
      validatePlaylistName(trimmed);
      const playlist = findPlaylist(playlistId);
      playlist.name = trimmed;
      playlist.updatedAt = now();
      return null;
    },
    addTracksToPlaylist: (playlistId, trackIds) => {
      validatePlaylistId(playlistId);
      if (trackIds.length === 0) {
        fail('VALIDATION', 'トラックIDが指定されていません');
      }
      trackIds.forEach(validateTrackId);
      const playlist = playlists.find((p) => p.id === playlistId);
      // 実装と同じく、見つからないトラックがあれば1曲も追加しない
      if (!playlist || !trackIds.every((id) => tracks.some((track) => track.id === id))) {
        fail('NOT_FOUND', 'プレイリストまたはトラックが見つかりません');
      }
      const timestamp = now();
      let added = 0;
      for (const trackId of trackIds) {
        // 追加済みの場合は飛ばす
        if (playlist.tracks.some((entry) => entry.trackId === trackId)) continue;
        const lastPosition = Math.max(-1, ...playlist.tracks.map((entry) => entry.position));
        playlist.tracks.push({ trackId, position: lastPosition + 1, addedAt: timestamp });
        added++;
      }
      if (added > 0) playlist.updatedAt = timestamp;
      return added;
    },
    removeTrackFromPlaylist: (playlistId, trackId) => {
      validatePlaylistId(playlistId);
      validateTrackId(trackId);
      const playlist = playlists.find((p) => p.id === playlistId);
      if (!playlist || !playlist.tracks.some((entry) => entry.trackId === trackId)) {
        fail('NOT_FOUND', 'プレイリストまたはトラックが見つかりません');
      }
      // 削除後はpositionを連番に振り直す
      playlist.tracks = playlist.tracks
        .filter((entry) => entry.trackId !== trackId)
        .sort((a, b) => a.position - b.position)
        .map((entry, position) => ({ ...entry, position }));
      playlist.updatedAt = now();
      return null;
    },
    reorderPlaylistTracks: (playlistId, trackIds) => {
      validatePlaylistId(playlistId);
      trackIds.forEach(validateTrackId);
      const playlist = findPlaylist(playlistId);
      trackIds.forEach((trackId, position) => {
        const entry = playlist.tracks.find((e) => e.trackId === trackId);
        if (entry) entry.position = position;
      });
      playlist.updatedAt = now();
      return null;
    },
    importM3uPlaylists: () => {
      // ファイルを選ぶダイアログ（Rust側が開く）の代わりに、選んだことにするファイルの結果を返す
      if (m3uImportMode === 'cancel') return [];
      const clean = m3uImportMode === 'clean';
      const baseName = clean ? 'ドライブ（MusicBee）' : '通勤（MusicBee）';
      let name = baseName;
      for (let number = 2; playlists.some((playlist) => playlist.name === name); number++) {
        name = `${baseName} (${number})`;
      }
      const trackIds = tracks
        .filter((track) => !track.isMissing)
        .slice(0, clean ? 4 : 6)
        .map((track) => track.id);
      const timestamp = now();
      const playlist: Playlist = {
        id: crypto.randomUUID(),
        name,
        description: null,
        tracks: trackIds.map((trackId, position) => ({ trackId, position, addedAt: timestamp })),
        createdAt: timestamp,
        updatedAt: timestamp
      };
      playlists.push(playlist);
      return [
        {
          fileName: `${baseName}.${clean ? 'm3u8' : 'm3u'}`,
          playlistId: playlist.id,
          playlistName: name,
          addedCount: trackIds.length,
          duplicateCount: clean ? 0 : 1,
          unmatchedCount: clean ? 0 : 2,
          unmatched: clean
            ? []
            : ['C:\\Music\\Unknown Artist\\99 Missing.mp3', 'http://example.com/radio.m3u'],
          error: null
        }
      ];
    },
    exportPlaylistM3u: (playlistId) => {
      validatePlaylistId(playlistId);
      const playlist = findPlaylist(playlistId);
      // 保存先を選ぶダイアログ（Rust側が開く）の代わりに、プレイリスト名のファイルへ書いたことにする
      return { fileName: `${playlist.name}.m3u8`, trackCount: playlist.tracks.length };
    },
    setCurrentTrack: (trackId) => {
      currentTrackId = trackId;
      return null;
    },
    getCurrentTrack: () => tracks.find((track) => track.id === currentTrackId) ?? null,
    savePlaybackState: (cursor, queue) => {
      if (queue) {
        [...queue.trackIds, ...(queue.originalTrackIds ?? [])].forEach(validateTrackId);
      }
      const trackIds = queue?.trackIds ?? playbackState.trackIds;
      const volume = cursor.volume;
      playbackState = {
        volume: volume !== null && Number.isFinite(volume) ? Math.max(0, Math.min(volume, 1)) : 1,
        shuffle: cursor.shuffle,
        repeat: cursor.repeat,
        // キューの外を指す位置は「何も再生していない」にする
        currentIndex:
          cursor.currentIndex !== null && cursor.currentIndex < trackIds.length
            ? cursor.currentIndex
            : null,
        trackIds,
        originalTrackIds: queue ? queue.originalTrackIds : playbackState.originalTrackIds
      };
      options.storage?.setItem(PLAYBACK_STATE_KEY, JSON.stringify(playbackState));
      return null;
    },
    getPlaybackState: () => {
      // 再生できる曲（ライブラリにあり、ファイルが見つかる曲）だけを残す
      const playable = new Map(
        tracks.filter((track) => !track.isMissing).map((track) => [track.id, track])
      );
      const queue: Track[] = [];
      let currentIndex: number | null = null;
      playbackState.trackIds.forEach((id, index) => {
        const track = playable.get(id);
        if (!track) return;
        // 再生していた曲（なくなっていれば、その次に残っている曲）の、除いた後の位置
        if (
          currentIndex === null &&
          playbackState.currentIndex !== null &&
          index >= playbackState.currentIndex
        ) {
          currentIndex = queue.length;
        }
        queue.push(track);
      });
      return {
        volume: playbackState.volume,
        shuffle: playbackState.shuffle,
        repeat: playbackState.repeat,
        queue,
        originalTrackIds: playbackState.originalTrackIds?.filter((id) => playable.has(id)) ?? null,
        currentIndex
      };
    },
    playbackPlay: (trackId, token) => playbackEngine.play(trackId, token),
    playbackSetNext: (trackId, token) => {
      playbackEngine.setNext(trackId, token);
      return null;
    },
    playbackPause: () => {
      playbackEngine.pause();
      return null;
    },
    playbackResume: () => {
      playbackEngine.resume();
      return null;
    },
    playbackSeek: (position) => {
      if (position === null || !Number.isFinite(position) || position < 0) {
        fail('VALIDATION', '再生位置が正しくありません');
      }
      playbackEngine.seek(position);
      return null;
    },
    playbackSetVolume: (volume) => {
      if (volume === null || !Number.isFinite(volume)) {
        fail('VALIDATION', '音量が正しくありません');
      }
      return null;
    },
    playbackSetEqualizer: (_enabled, gains) => {
      if (gains.length !== EQ_BAND_COUNT) {
        fail('VALIDATION', `イコライザのゲインは${EQ_BAND_COUNT}個で指定してください`);
      }
      return null;
    },
    playbackStop: () => {
      playbackEngine.stop();
      return null;
    },
    getOutputDevices: () => MOCK_OUTPUT_DEVICES,
    setNowPlaying: (update) => {
      if (update) {
        validateTrackId(update.trackId);
        // ライブラリにない曲は、前の曲の情報を残さない
        if (!tracks.some((track) => track.id === update.trackId)) {
          nowPlaying = null;
          fail('NOT_FOUND', '指定されたトラックが見つかりません');
        }
      }
      nowPlaying = update;
      return null;
    },
    showInFolder: (trackId) => {
      validateTrackId(trackId);
      console.info(`[mock] ファイルマネージャーで表示: ${findTrack(trackId).filePath}`);
      return null;
    },
    getSettings: () => settings,
    saveSettings: (next) => {
      if (!/^#[0-9a-fA-F]{6}$/.test(next.accentColor)) {
        fail('VALIDATION', 'アクセントカラーは#rrggbb形式で指定してください');
      }
      if (
        !Number.isInteger(next.crossfadeSeconds) ||
        next.crossfadeSeconds < 0 ||
        next.crossfadeSeconds > MAX_CROSSFADE_SECONDS
      ) {
        fail('VALIDATION', `クロスフェードは0〜${MAX_CROSSFADE_SECONDS}秒で指定してください`);
      }
      if (next.outputDeviceId !== null && next.outputDeviceId.length === 0) {
        fail('VALIDATION', '出力デバイスの指定が正しくありません');
      }
      if (
        !(LIBRARY_SCAN_INTERVALS as readonly number[]).includes(next.libraryScanIntervalMinutes)
      ) {
        fail('VALIDATION', '再スキャンの間隔が選べる値ではありません');
      }
      settings = { ...next };
      options.emit('settings-changed', settings);
      return null;
    },
    openProjectPage: () => {
      console.info('[mock] プロジェクトのページを開く操作は無視しました');
      return null;
    },
    setFavorite: (trackIds, favorite) => {
      trackIds.forEach(validateTrackId);
      // 見つからないトラックがあれば、1曲も変えない
      const targets = [...new Set(trackIds)].map(findTrack);
      const timestamp = now();
      for (const track of targets) {
        if (favorite) {
          // すでにお気に入りの曲は、お気に入りにした日時を変えない
          if (!track.isFavorite || !favoritedAt.has(track.id)) favoritedAt.set(track.id, timestamp);
        } else {
          favoritedAt.delete(track.id);
        }
        track.isFavorite = favorite;
      }
      return null;
    },
    setRating: (trackId, rating) => {
      validateTrackId(trackId);
      if (rating < 0 || rating > 5) {
        fail('VALIDATION', 'レーティングは0から5の間で指定してください');
      }
      // 実装と同じく、存在しないトラックはエラーにせず無視する
      const track = tracks.find((t) => t.id === trackId);
      if (track) {
        track.rating = rating;
        track.updatedAt = now();
      }
      return null;
    },
    incrementPlayCount: (trackId) => {
      validateTrackId(trackId);
      const track = findTrack(trackId);
      const timestamp = now();
      track.playCount += 1;
      track.lastPlayedAt = timestamp;
      track.updatedAt = timestamp;
      const lastId = playHistory.reduce((max, entry) => Math.max(max, entry.id), 0);
      playHistory = [{ id: lastId + 1, trackId, playedAt: timestamp }, ...playHistory];
      return track.playCount;
    },
    incrementSkipCount: (trackId) => {
      validateTrackId(trackId);
      const track = findTrack(trackId);
      track.skipCount += 1;
      return track.skipCount;
    },
    getFavoriteTracks: () =>
      tracks
        .filter((track) => track.isFavorite)
        // 最近お気に入りにした順（まとめてお気に入りにした曲は、アルバムの中の順）
        .sort(
          (a, b) =>
            compareAsc(favoritedAt.get(b.id) ?? null, favoritedAt.get(a.id) ?? null) ||
            compareAsc(a.album, b.album) ||
            (a.trackNumber ?? 0) - (b.trackNumber ?? 0)
        ),
    getMostPlayedTracks: (limit) =>
      tracks
        .filter((track) => track.playCount > 0)
        // 同じ回数なら、最近再生した曲を先にする
        .sort((a, b) => b.playCount - a.playCount || compareAsc(b.lastPlayedAt, a.lastPlayedAt))
        .slice(0, limit ?? DEFAULT_STATS_LIMIT),
    getPlayHistory: () => playHistory,
    deleteTracksCommand: (trackIds) => {
      validateTrackIdsForDeletion(trackIds);
      return removeTracks(trackIds);
    },
    // 実ファイルは存在しないため、削除は常に成功したものとして扱う
    deleteTracksWithFilesCommand: (trackIds) => {
      validateTrackIdsForDeletion(trackIds);
      return { successCount: removeTracks(trackIds), failedCount: 0, failedTracks: [] };
    },
    getLibraryFolders: () => {
      const folders = [...libraryFolders]
        .sort((a, b) => compareAsc(a.path, b.path))
        .map((folder) => ({ ...folder, trackCount: tracksUnder(folder.path).length }));
      const registered = folders.reduce((sum, folder) => sum + folder.trackCount, 0);
      return {
        folders,
        unregisteredTrackCount: tracks.length - registered,
        missingTrackCount: tracks.filter((track) => track.isMissing).length
      };
    },
    // 見つからない曲を、すべてライブラリから外す
    removeMissingTracks: () => {
      const removed = removeTracks(
        tracks.filter((track) => track.isMissing).map((track) => track.id)
      );
      if (removed > 0) options.emit('library-changed', null);
      return removed;
    },
    removeLibraryFolder: (folderId, removeTracksToo) => {
      const folder = findLibraryFolder(folderId);
      const removed = removeTracksToo
        ? removeTracks(tracksUnder(folder.path).map((track) => track.id))
        : 0;
      libraryFolders = libraryFolders.filter((f) => f.id !== folderId);
      if (removed > 0) options.emit('library-changed', null);
      return removed;
    },
    // 実ファイルは存在しないため、見つかるフォルダは「変更なし」として扱う
    rescanLibraryFolder: (folderId) => {
      const folder = findLibraryFolder(folderId);
      if (!folder.exists) {
        fail(
          'NOT_FOUND',
          `フォルダが見つかりません: ${folder.path}（外付けドライブなどが接続されているか確認してください）`
        );
      }
      folder.lastScannedAt = now();
      return {
        addedCount: 0,
        updatedCount: 0,
        relinkedCount: 0,
        missingCount: 0,
        restoredCount: 0,
        errorCount: 0,
        errors: [],
        missingSkipped: false
      };
    },
    // 実ファイルを読み直す代わりに、全トラックを更新済みとして扱う
    refreshLibraryMetadata: () => ({
      updatedCount: tracks.length,
      skippedCount: 0,
      errorCount: 0,
      errors: []
    }),
    // モックの値はすべて「ファイルと同じ」として扱う（書き込みは行わない）
    importLibraryXml: (includePlaylists) => {
      // ファイルを選ぶダイアログ（Rust側が開く）の代わりに、決まった内容のXMLを選んだことにする:
      // 先頭の8曲に、今より多い再生回数・古い追加日が書かれている（2回目からは変わらない）
      const targets = tracks.filter((track) => !track.isMissing).slice(0, 8);
      let updatedCount = 0;
      targets.forEach((track, index) => {
        const playCount = 20 + index * 3;
        const createdAt = `2019-0${(index % 9) + 1}-10T12:00:00.000Z`;
        if (track.playCount < playCount || track.createdAt > createdAt) updatedCount++;
        track.playCount = Math.max(track.playCount, playCount);
        if (track.createdAt > createdAt) track.createdAt = createdAt;
        track.lastPlayedAt ??= '2024-09-25T18:31:11.000Z';
      });
      let playlistCount = 0;
      if (includePlaylists) {
        const baseName = 'お気に入り（MusicBee）';
        let name = baseName;
        for (let number = 2; playlists.some((playlist) => playlist.name === name); number++) {
          name = `${baseName} (${number})`;
        }
        const timestamp = now();
        playlists.push({
          id: crypto.randomUUID(),
          name,
          description: null,
          tracks: targets
            .slice(0, 5)
            .map((track, position) => ({ trackId: track.id, position, addedAt: timestamp })),
          createdAt: timestamp,
          updatedAt: timestamp
        });
        playlistCount = 1;
      }
      options.emit('library-changed', null);
      return {
        fileName: 'iTunes Music Library.xml',
        trackCount: targets.length + 2,
        matchedCount: targets.length,
        updatedCount,
        unmatchedCount: 2,
        unmatched: ['D:/Music/Unknown Artist/99 Missing.mp3', 'D:/Music/Old/Deleted Song.flac'],
        playlistCount,
        skippedPlaylistCount: includePlaylists ? 1 : 0
      };
    },
    writeLibraryMetadataToFiles: () => {
      options.emit('library-changed', null);
      return {
        writtenCount: 0,
        unchangedCount: tracks.length,
        skippedCount: 0,
        errorCount: 0,
        errors: []
      };
    },
    getSyncDevices: () =>
      [...syncDevices]
        .sort(
          orderBy(
            (d) => d.name,
            (d) => d.createdAt
          )
        )
        .map(toSyncDevice),
    // 管理ファイルは存在しないため、選んだフォルダは常に新しいデバイスとして登録する
    registerSyncDevice: (folderPath, name) => {
      const trimmedName = validateDeviceName(name);
      const device: MockSyncDevice = {
        id: crypto.randomUUID(),
        name: trimmedName,
        path: validateDeviceFolder(folderPath),
        syncAll: false,
        playlistIds: [],
        removeUnselected: true,
        connected: true,
        createdAt: now(),
        lastSyncedAt: null,
        totalBytes: 64 * GIB,
        otherUsedBytes: 0,
        copied: new Map()
      };
      syncDevices.push(device);
      return toSyncDevice(device);
    },
    updateSyncDevice: (deviceId, config) => {
      const device = findSyncDevice(deviceId);
      config.playlistIds.forEach(validatePlaylistId);
      device.name = validateDeviceName(config.name);
      device.syncAll = config.syncAll;
      // 実装と同じく、見つからないプレイリストは無視する
      device.playlistIds = config.playlistIds.filter((id) => playlists.some((p) => p.id === id));
      device.removeUnselected = config.removeUnselected;
      return toSyncDevice(device);
    },
    // 選んだフォルダにデバイスがあることにして、接続された状態にする
    relinkSyncDevice: (deviceId, folderPath) => {
      const device = findSyncDevice(deviceId);
      device.path = validateDeviceFolder(folderPath);
      device.connected = true;
      return toSyncDevice(device);
    },
    removeSyncDevice: (deviceId) => {
      const device = findSyncDevice(deviceId);
      syncDevices = syncDevices.filter((d) => d !== device);
      return null;
    },
    planDeviceSync: (deviceId) => planSync(findSyncDevice(deviceId)).summary,
    runDeviceSync,
    cancelDeviceSync: () => {
      if (isSyncRunning) isSyncCancelled = true;
      return null;
    }
  };

  return {
    async invoke(cmd, args = {}) {
      const name = toCommandName(cmd);
      if (!Object.hasOwn(handlers, name)) {
        throw new Error(`[mock] 未実装のコマンドです: ${cmd}`);
      }
      const handler = handlers[name as CommandName] as (...params: unknown[]) => unknown;
      // bindings.tsは引数を宣言順のオブジェクト（`{ trackId, metadata }`）で渡すため、
      // 値の並びがそのまま`commands.xxx()`の位置引数に対応する
      const result = await handler(...Object.values(args));
      // IPCのシリアライズと同様に複製を返し、呼び出し側から状態を書き換えられないようにする
      return structuredClone(result ?? null);
    },
    albumArtUrl(trackId) {
      const track = tracks.find((t) => t.id === trackId);
      return track ? albumArtOf(track).url : null;
    },
    nowPlaying: () => structuredClone(nowPlaying),
    setM3uImportMode: (mode) => {
      m3uImportMode = mode;
    },
    setAlbumArtPickMode: (mode) => {
      albumArtPickMode = mode;
    }
  };
}
