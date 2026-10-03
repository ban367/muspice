/**
 * ブラウザモック用のインメモリバックエンド
 *
 * Rust側のコマンド（`src-tauri/src/commands/`）の振る舞いを、フィクスチャを使って
 * メモリ上で再現する。並び順・バリデーション・エラーコードは実装に合わせているが、
 * FTS5検索やファイルI/O（タグ書き込み・ファイル削除）は簡略化・省略している。
 *
 * ハンドラ表の型は`bindings.ts`の`commands`から導出しているため、Rust側でコマンドが
 * 追加・変更されてバインディングが再生成されると、ここが型エラーになり追随が必要になる。
 */
import type { commands } from '#lib/bindings.js';
import type {
  AlbumGroup,
  AppError,
  ArtistGroup,
  DuplicateAction,
  GenreGroup,
  ImportResult,
  LibraryFolder,
  Metadata,
  Playlist,
  Settings,
  Track
} from '#lib/types/models.js';
import { ALBUMS_WITHOUT_ART, createFixturePlaylists, createFixtureTracks } from './fixtures';
import { createAlbumArt } from './media';

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
}

export interface MockBackend {
  /** IPCのコマンド名（snake_case）と引数オブジェクトでコマンドを実行する */
  invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
  /** `albumart`プロトコルの代わりに、トラックのアルバムアートをdata URLで返す（アートがなければnull） */
  albumArtUrl(trackId: string): string | null;
}

/** インポート時に「見つかった」ことにするファイル数 */
const IMPORT_FILE_COUNT = 6;
/** インポートの1ファイルあたりの処理時間（進捗表示の確認用） */
const IMPORT_STEP_MS = 150;
const DEFAULT_STATS_LIMIT = 50;

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
  const { year, trackNumber } = metadata;
  if (year != null && (year < 1000 || year > 9999)) {
    fail('VALIDATION', '年は1000から9999の範囲で指定してください');
  }
  if (trackNumber != null && (trackNumber < 1 || trackNumber > 999)) {
    fail('VALIDATION', 'トラック番号は1から999の範囲で指定してください');
  }
}

function validateStringLength(value: string | null | undefined, field: string, max: number) {
  if (value == null) return;
  const length = byteLength(value);
  if (length > max) {
    fail('VALIDATION', `${field}は${max}文字以内で入力してください（現在: ${length}文字）`);
  }
}

function validateMetadataInput(metadata: Metadata): void {
  validateMetadata(metadata);
  validateStringLength(metadata.title, 'タイトル', 255);
  validateStringLength(metadata.artist, 'アーティスト', 255);
  validateStringLength(metadata.album, 'アルバム', 255);
  validateStringLength(metadata.genre, 'ジャンル', 100);
  validateStringLength(metadata.albumArtist, 'アルバムアーティスト', 255);
  validateStringLength(metadata.composer, '作曲者', 255);
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

const byNameIgnoreCase = (a: { name: string }, b: { name: string }) =>
  compareAsc(a.name.toLowerCase(), b.name.toLowerCase());

/** 出現順を保ったままグループ化する */
function groupBy<T>(items: T[], key: (item: T) => string): Map<string, T[]> {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const name = key(item);
    groups.set(name, [...(groups.get(name) ?? []), item]);
  }
  return groups;
}

function sumDuration(tracks: Track[]): number {
  return tracks.reduce((total, track) => total + (track.duration ?? 0), 0);
}

function toAlbumGroup(name: string, artist: string | null, tracks: Track[]): AlbumGroup {
  return {
    name,
    artist,
    trackCount: tracks.length,
    totalDuration: sumDuration(tracks),
    representativeTrackId: tracks[0]?.id ?? '',
    tracks
  };
}

export function createMockBackend(options: MockBackendOptions): MockBackend {
  const sleep =
    options.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));

  let tracks: Track[] = createFixtureTracks();
  let playlists: Playlist[] = createFixturePlaylists();
  let currentTrackId: string | null = null;
  let settings: Settings = {
    startupPage: 'lastOpened',
    accentColor: '#3b82f6',
    volumeNormalization: 'off',
    gaplessPlayback: true,
    crossfadeSeconds: 0,
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
    // playlist_tracksのON DELETE CASCADEに相当（実装と同じくpositionは詰めない）
    for (const playlist of playlists) {
      playlist.tracks = playlist.tracks.filter((entry) => !targets.has(entry.trackId));
    }
    return before - tracks.length;
  }

  /** update_track_metadataと同様、title/artist/album/genre/yearを丸ごと置き換える */
  function updateTrackMetadata(trackId: string, metadata: Metadata): null {
    validateTrackId(trackId);
    validateMetadataInput(metadata);
    const track = tracks.find((t) => t.id === trackId);
    if (!track) fail('NOT_FOUND', '指定されたトラックが見つかりません');
    track.title = metadata.title ?? null;
    track.artist = metadata.artist ?? null;
    track.album = metadata.album ?? null;
    track.genre = metadata.genre ?? null;
    track.year = metadata.year ?? null;
    track.updatedAt = now();
    return null;
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
    const folder = folderPath.replace(/[\\/]+$/, '');
    const album = folder.split(/[\\/]/).pop() || 'Imported';
    let importedCount = 0;
    let skippedCount = 0;

    for (let index = 0; index < IMPORT_FILE_COUNT; index++) {
      const fileName = `Mock Track ${String(index + 1).padStart(2, '0')}.mp3`;
      const filePath = `${folder}/${fileName}`;
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
          lastPlayedAt: null,
          createdAt: timestamp,
          updatedAt: timestamp,
          replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null }
        });
      }
      importedCount++;
    }

    registerLibraryFolder(folder);
    return { importedCount, skippedCount, errorCount: 0, errors: [] };
  }

  // ---- ライブラリフォルダ（src-tauri/src/library_folder.rsに対応） ----

  const isSameOrWithin = (path: string, folder: string) =>
    path === folder || path.startsWith(`${folder}/`);
  const tracksUnder = (folder: string) =>
    tracks.filter((track) => track.filePath.startsWith(`${folder}/`));

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

  const handlers: MockCommandHandlers = {
    importFolder,
    getAllTracks: () => tracksWhere(() => true),
    searchTracks: (query) => {
      const keyword = sanitizeSearchQuery(query).toLowerCase();
      if (!keyword) return [];
      // FTS5の代わりに、LIKEフォールバックと同じ列を部分一致で検索する
      return tracksWhere((track) =>
        [track.title, track.artist, track.album, track.genre].some((value) =>
          value?.toLowerCase().includes(keyword)
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
    getAlbumsGrouped: () => {
      const sorted = tracks
        .filter((track) => track.album !== null)
        .sort(
          orderBy(
            (t) => t.album,
            (t) => t.trackNumber,
            (t) => t.title
          )
        );
      return [...groupBy(sorted, (track) => track.album ?? '')]
        .map(([name, items]) => toAlbumGroup(name, items[0].artist, items))
        .sort(byNameIgnoreCase);
    },
    getArtistsGrouped: () => {
      const sorted = tracks
        .filter((track) => track.artist !== null)
        .sort(
          orderBy(
            (t) => t.artist,
            (t) => t.album,
            (t) => t.trackNumber,
            (t) => t.title
          )
        );
      return [...groupBy(sorted, (track) => track.artist ?? '')]
        .map(([name, items]): ArtistGroup => {
          const albums = [...groupBy(items, (track) => track.album ?? '不明なアルバム')]
            .map(([album, albumTracks]) => toAlbumGroup(album, name, albumTracks))
            .sort(byNameIgnoreCase);
          return {
            name,
            albumCount: albums.length,
            trackCount: items.length,
            totalDuration: sumDuration(items),
            representativeTrackId: albums[0]?.representativeTrackId ?? '',
            albums
          };
        })
        .sort(byNameIgnoreCase);
    },
    getGenresGrouped: () => {
      const sorted = tracks
        .filter((track) => track.genre !== null)
        .sort(
          orderBy(
            (t) => t.genre,
            (t) => t.artist,
            (t) => t.album,
            (t) => t.trackNumber,
            (t) => t.title
          )
        );
      return [...groupBy(sorted, (track) => track.genre ?? '')]
        .map(([name, items]): GenreGroup => ({
          name,
          trackCount: items.length,
          totalDuration: sumDuration(items),
          representativeTrackId: items[0].id,
          tracks: items
        }))
        .sort(byNameIgnoreCase);
    },
    updateTrackMetadata,
    // ファイルへのタグ書き込みは行わず、DB相当の更新のみ行う
    updateTrackMetadataWithFile: updateTrackMetadata,
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
      const { title, artist, album, genre, year } = metadata;
      const hasChanges = [title, artist, album, genre, year].some((value) => value != null);
      for (const track of targets) {
        if (title != null) track.title = title;
        if (artist != null) track.artist = artist;
        if (album != null) track.album = album;
        if (genre != null) track.genre = genre;
        if (year != null) track.year = year;
        if (hasChanges) track.updatedAt = timestamp;
      }
      return null;
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
    addTrackToPlaylist: (playlistId, trackId) => {
      validatePlaylistId(playlistId);
      validateTrackId(trackId);
      const playlist = playlists.find((p) => p.id === playlistId);
      if (!playlist || !tracks.some((track) => track.id === trackId)) {
        fail('NOT_FOUND', 'プレイリストまたはトラックが見つかりません');
      }
      // 追加済みの場合は何もしない
      if (playlist.tracks.some((entry) => entry.trackId === trackId)) return null;
      const timestamp = now();
      const lastPosition = Math.max(-1, ...playlist.tracks.map((entry) => entry.position));
      playlist.tracks.push({ trackId, position: lastPosition + 1, addedAt: timestamp });
      playlist.updatedAt = timestamp;
      return null;
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
    getTrackFilePath: (trackId) => {
      validateTrackId(trackId);
      return findTrack(trackId).filePath;
    },
    setCurrentTrack: (trackId) => {
      currentTrackId = trackId;
      return null;
    },
    getCurrentTrack: () => tracks.find((track) => track.id === currentTrackId) ?? null,
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
        next.crossfadeSeconds > 12
      ) {
        fail('VALIDATION', 'クロスフェードは0〜12秒で指定してください');
      }
      if (![0, 15, 30, 60, 360].includes(next.libraryScanIntervalMinutes)) {
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
    toggleFavorite: (trackId) => {
      validateTrackId(trackId);
      const track = findTrack(trackId);
      track.isFavorite = !track.isFavorite;
      track.updatedAt = now();
      return track.isFavorite;
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
      return track.playCount;
    },
    getFavoriteTracks: () =>
      tracks
        .filter((track) => track.isFavorite)
        .sort((a, b) => compareAsc(b.updatedAt, a.updatedAt)),
    getMostPlayedTracks: (limit) =>
      tracks
        .filter((track) => track.playCount > 0)
        .sort((a, b) => b.playCount - a.playCount)
        .slice(0, limit ?? DEFAULT_STATS_LIMIT),
    getRecentlyPlayedTracks: (limit) =>
      tracks
        .filter((track) => track.lastPlayedAt !== null)
        .sort((a, b) => compareAsc(b.lastPlayedAt, a.lastPlayedAt))
        .slice(0, limit ?? DEFAULT_STATS_LIMIT),
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
      return { folders, unregisteredTrackCount: tracks.length - registered };
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
        removedCount: 0,
        errorCount: 0,
        errors: [],
        removalSkipped: false
      };
    },
    // 実ファイルを読み直す代わりに、全トラックを更新済みとして扱う
    refreshLibraryMetadata: () => ({
      updatedCount: tracks.length,
      skippedCount: 0,
      errorCount: 0,
      errors: []
    })
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
      const album = tracks.find((t) => t.id === trackId)?.album ?? null;
      if (album === null || ALBUMS_WITHOUT_ART.has(album)) return null;
      return createAlbumArt(album);
    }
  };
}
