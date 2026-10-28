/**
 * ブラウザモック用のインメモリバックエンド
 *
 * Rust側のコマンド（`src-tauri/src/commands/`）の振る舞いを、フィクスチャを使って
 * メモリ上で再現する。並び順・バリデーション・エラーコード・検索の一致の仕方は実装に
 * 合わせているが、ファイルI/O（タグ書き込み・ファイル削除）は簡略化・省略している。
 *
 * ハンドラ表の型は`bindings.ts`の`commands`から導出しているため、Rust側でコマンドが
 * 追加・変更されてバインディングが再生成されると、ここが型エラーになり追随が必要になる。
 * 送るイベントも、名前（`events`のキー）と内容の型を`bindings.ts`の`events`から導出している。
 */
import {
  DEFAULT_ACCENT_COLOR,
  DEFAULT_MUSIC_FOLDER_TEMPLATE,
  AAC_BITRATES_KBPS,
  LIBRARY_SCAN_INTERVALS,
  MAX_CROSSFADE_SECONDS,
  MAX_NORMALIZATION_PREAMP_DB,
  type commands,
  type events
} from '#lib/bindings.js';
import type {
  AlbumArtInfo,
  AlbumGroup,
  AlbumSummary,
  AppError,
  ArtistSummary,
  BulkUpdateResult,
  ConvertEntry,
  ConvertFormat,
  ConvertOptions,
  ConvertResult,
  DeviceSyncPlan,
  DeviceSyncResult,
  DuplicateAction,
  FolderImportMode,
  FolderRelocation,
  GenreSummary,
  ImportOutcome,
  KeepOrganizedResult,
  ImportResult,
  LibraryFolder,
  LoudnessScanResult,
  Metadata,
  MusicFolderChange,
  NotCopied,
  NowPlayingUpdate,
  OrganizeDestination,
  PendingFolderImport,
  PlayHistoryEntry,
  Playlist,
  PlaylistFolder,
  RescanResult,
  Settings,
  SmartRules,
  SyncDevice,
  Track
} from '#lib/types/models.js';
import {
  ALBUMS_WITHOUT_ART,
  ALBUMS_WITH_FOLDER_ART,
  createBulkTracks,
  createFixturePlayHistory,
  FIXTURE_EMBEDDED_LYRICS,
  FIXTURE_LRC_FILES,
  createFixturePlaylistFolders,
  createFixturePlaylists,
  createFixtureTracks,
  fileFormatName,
  mockPlaylistId,
  mockTrackId
} from './fixtures';
import { matchesSearchTerms, normalizeSearchText, searchTerms } from '#lib/utils/searchText.js';
import {
  countSmartPlaylistTracks,
  findSmartPlaylistTracks,
  smartRulesError,
  type SmartPlaylistContext
} from './smartPlaylist.js';
import { createAlbumArt } from './media';
import { organizedRelativePath, parseOrganizeTemplate, planOrganize } from './fileOrganize';
import { createMockPlaybackEngine, MOCK_OUTPUT_DEVICES } from './playbackEngine';
import { MAX_PLAYBACK_SPEED, MIN_PLAYBACK_SPEED } from '#lib/stores/playbackAids.svelte.js';
import { hasValue, listedArtists, splitValues, VALUE_SEPARATOR } from '#lib/utils/multiValue.js';

type Commands = typeof commands;
type CommandName = keyof Commands;

type Events = typeof events;
/** バックエンドが送るイベント（`bindings.ts`の`events`のキー） */
export type MockEventName = keyof Events;
/** イベントの内容（`events.xxx.listen()`のコールバックが受け取る`payload`の型） */
export type MockEventPayload<K extends MockEventName> = Parameters<
  Parameters<Events[K]['listen']>[0]
>[0]['payload'];

/**
 * イベントの、IPCで送る名前（`bindings.ts`の`makeEvent`に渡している名前と同じ）
 *
 * `satisfies`で、`events`のすべてのキーがあることを確かめる（名前が合っているかは、テストで確かめる）。
 */
export const MOCK_EVENT_NAMES = {
  convertProgress: 'convert-progress',
  deviceSyncProgress: 'device-sync-progress',
  importFilesRequested: 'import-files-requested',
  importProgress: 'import-progress',
  libraryChanged: 'library-changed',
  libraryScanProgress: 'library-scan-progress',
  loudnessScanFinished: 'loudness-scan-finished',
  loudnessScanProgress: 'loudness-scan-progress',
  openImportDialog: 'open-import-dialog',
  organizeProgress: 'organize-progress',
  playbackControl: 'playback-control',
  playbackEvent: 'playback-event',
  settingsChanged: 'settings-changed',
  showAboutDialog: 'show-about-dialog',
  toggleMiniPlayer: 'toggle-mini-player',
  toggleSidebar: 'toggle-sidebar',
  tracksOpened: 'tracks-opened'
} as const satisfies Record<MockEventName, string>;
type CommandResult<K extends CommandName> = Awaited<ReturnType<Commands[K]>>;

/** `bindings.ts`の全コマンドを、同じ引数・戻り値で実装するハンドラ表 */
type MockCommandHandlers = {
  [K in CommandName]: (
    ...args: Parameters<Commands[K]>
  ) => CommandResult<K> | Promise<CommandResult<K>>;
};

export interface MockBackendOptions {
  /**
   * バックエンドからのイベント送信（`importProgress`など。IPCで送る名前は`MOCK_EVENT_NAMES`）
   *
   * 名前と内容は`bindings.ts`の`events`の型で決まるため、Rust側でイベントの名前・項目を変えると、
   * ここが型エラーになる。
   */
  emit: <K extends MockEventName>(event: K, payload: MockEventPayload<K>) => void;
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
  /**
   * ミニプレーヤー（メインウィンドウの小さな表示）にしていることになっているか
   *
   * ブラウザではウィンドウの大きさを変えられないため、`setMiniPlayer`で受け取った内容をここで確認する。
   */
  miniPlayer(): { enabled: boolean; alwaysOnTop: boolean };
  /**
   * ファイルを選ぶインポート（`importFiles`）で、ファイルを選んだことにするかを切り替える
   *
   * - `pick`（既定）: 2つの音楽ファイルを選んだ
   * - `cancel`: 選ばずに閉じた
   */
  setFileImportMode(mode: MockFileImportMode): void;
  /**
   * Finderの「このアプリで開く」で、音楽ファイルが開かれたことにする
   *
   * 実装と同じく、ライブラリへインポートしてから`tracks-opened`を送る（曲は`takeOpenedFiles`で渡す）。
   * @param fileNames - 開かれたファイルの名前（音楽ファイルでないものは、エラーとして渡す）
   */
  openFiles(fileNames: string[]): void;
  /**
   * 重複した曲を、ライブラリに足す（重複した曲の画面と、ファイルの整理の確認用）
   *
   * ライブラリフォルダの中の`Downloads`に、今ある曲と同じタイトル・アーティストの曲を足す
   * （整理すると、移動先に同じ名前のファイルがある曲・同じ移動先になる曲が出る）。
   */
  addDuplicateTracks(): void;
  /**
   * 形式の変換で、書き出すフォルダを選ぶダイアログの結果を切り替える（nullは、選ばなかった）
   *
   * 既定は`/Users/mock/Export`。
   */
  setConvertExportFolder(path: string | null): void;
  /**
   * 読み込まないフォルダの追加（`addLibraryFolderExclusion`）で、ダイアログで選んだことにするフォルダを
   * 切り替える（絶対パス。nullは、選ばなかった）
   *
   * 切り替えるまでは、ライブラリフォルダの中で曲のある、まだ読み込まないフォルダにしていない
   * 最初のフォルダ（アーティストのフォルダ）を選んだことにする（中の曲を外すかの確認が出る）。
   */
  setExclusionFolder(path: string | null): void;
  /**
   * ライブラリフォルダの場所の変更（`planLibraryFolderRelocation`）で、ダイアログで選んだことにする
   * フォルダを切り替える（絶対パス。nullは、選ばなかった）
   *
   * 既定は`/Volumes/Backup/Music`（どのライブラリフォルダとも重ならない）。実ファイルは存在しないため、
   * 見つからない曲のほかは、新しい場所でファイルが見つかったことにする。
   */
  setRelocationFolder(path: string | null): void;
  /**
   * バックアップの保存（`exportLibraryBackup`）の保存先と、復元（`planLibraryRestore`）で選ぶファイルを
   * 切り替える（絶対パス。nullは、選ばなかった）
   *
   * 既定は`/Users/demo/Documents/Muspice Backup.muspicebackup`。実ファイルは存在しないため、復元では、
   * 今のライブラリと同じ内容のバックアップを選んだことにする（拡張子が`.muspicebackup`・`.db`でなければ
   * Muspiceのバックアップではない、名前に`damaged`・`newer`を含めば壊れている・新しい版のもの）。
   */
  setBackupFile(path: string | null): void;
  /**
   * フォルダのインポート・転送先デバイスの登録・転送先の変更で、フォルダを選ぶダイアログ
   * （Rust側が開く）で選んだことにするフォルダを切り替える（nullは、選ばずに閉じた）
   *
   * 既定は`/Users/demo/Music/Mock Import`（ライブラリフォルダの中のため、転送先デバイスの
   * 追加を確かめる時は、ライブラリの外のフォルダ（例: `/Volumes/NEW_SD`）にする）。
   */
  setFolderResult(path: string | null): void;
  /**
   * 音楽フォルダに、新しく登録するフォルダを選んだ時（`setMusicFolder`）に、見つかったことにする
   * 曲の数を切り替える（0は、空のフォルダ）
   *
   * 既定はフォルダのインポートと同じ数。
   */
  setMusicFolderFileCount(count: number): void;
  /**
   * 音楽フォルダのドライブの空き容量（バイト）を切り替える（取り込む曲のコピーで、空き容量が足りない
   * 場合の確認用）
   *
   * 既定は`MUSIC_FOLDER_FREE_BYTES`。
   */
  setMusicFolderFreeBytes(bytes: number): void;
  /** `albumart`プロトコルの代わりに、トラックのアルバムアートをdata URLで返す（アートがなければnull） */
  albumArtUrl(trackId: string): string | null;
}

/** ファイルを選ぶインポートで、選んだことにする内容 */
export type MockFileImportMode = 'pick' | 'cancel';

/** 取り込む対象（音楽ファイルは、あることにするフォルダと名前。フォルダはパス） */
interface MockImportTargets {
  files: { folder: string; fileName: string }[];
  folders: string[];
}

/** モックのライブラリフォルダ（曲数は、曲の一覧から数える。読み込まないフォルダは相対パス） */
type MockLibraryFolder = Omit<LibraryFolder, 'trackCount' | 'exclusions'> & {
  exclusions: { id: string; path: string }[];
};

/** インポートできる音楽ファイルの拡張子（Rustの`SUPPORTED_EXTENSIONS`と同じ） */
const AUDIO_EXTENSIONS = [
  'mp3',
  'flac',
  'wav',
  'm4a',
  'aac',
  'mp2',
  'aiff',
  'aif',
  'ogg',
  'opus',
  'wv',
  'ape'
];
/** フォルダを選ぶダイアログで、既定で選んだことにするフォルダ（`setFolderResult`で切り替える） */
const DEFAULT_FOLDER_RESULT = '/Users/demo/Music/Mock Import';
/** ライブラリフォルダの場所の変更で、既定で選んだことにするフォルダ（`setRelocationFolder`で切り替える） */
const DEFAULT_RELOCATION_FOLDER = '/Volumes/Backup/Music';
/** バックアップの保存・復元で、既定で選んだことにするファイル（`setBackupFile`で切り替える） */
const DEFAULT_BACKUP_FILE = '/Users/demo/Documents/Muspice Backup.muspicebackup';
/** ファイルを選ぶインポートで、選んだことにするファイル（フォルダと名前） */
const PICKED_FILES_FOLDER = '/Users/mock/Downloads';
const PICKED_FILE_NAMES = ['Picked Song A.flac', 'Picked Song B.mp3'];
/** ウィンドウへのドロップ・「このアプリで開く」で渡されたファイルが、あることにするフォルダ */
const DROPPED_FILES_FOLDER = '/Users/mock/Desktop';

/** インポート時に「見つかった」ことにするファイル数 */
const IMPORT_FILE_COUNT = 6;
/** 音楽フォルダのドライブの、既定の空き容量（バイト） */
const MUSIC_FOLDER_FREE_BYTES = 200 * 1024 ** 3;
/** コピーの後に残す、音楽フォルダのドライブの空き容量（Rustの`SPACE_MARGIN_BYTES`と同じ） */
const COPY_SPACE_MARGIN_BYTES = 64 * 1024 ** 2;
/** OSの音楽フォルダ（Rustの`audio_dir`の代わり。ライブラリフォルダとして登録してある） */
const SYSTEM_MUSIC_FOLDER = '/Users/demo/Music';
/** 音楽フォルダの書式の例に出す曲の数（Rustの`TEMPLATE_EXAMPLE_COUNT`と同じ） */
const TEMPLATE_EXAMPLE_COUNT = 5;
/** インポートの1ファイルあたりの処理時間（進捗表示の確認用） */
const IMPORT_STEP_MS = 150;
/** 再スキャン・場所の付け替えで、読み込んだことにするファイルの数（進捗表示の確認用） */
const RESCAN_PROGRESS_STEPS = 5;
/** ファイルの整理の1曲あたりの処理時間（進捗表示・中止の確認用） */
const ORGANIZE_STEP_MS = 200;
const DEFAULT_STATS_LIMIT = 50;
/** イコライザのバンドの数（Rustの`EQ_BANDS`と同じ） */
const EQ_BAND_COUNT = 10;
/** デバイスへの同期の1曲あたりの処理時間（進捗表示の確認用） */
const SYNC_STEP_MS = 300;

/** 形式の変換で、1曲にかかることにする時間（進捗と中断を目視できるようにする） */
const CONVERT_STEP_MS = 300;

/** 変換後のファイルの拡張子と、ビットレート（kbps。AACは、指定した値） */
const CONVERT_OUTPUTS: Record<ConvertFormat, { extension: string; bitrate: number | null }> = {
  aac: { extension: 'm4a', bitrate: null },
  alac: { extension: 'm4a', bitrate: 900 },
  flac: { extension: 'flac', bitrate: 900 },
  wav: { extension: 'wav', bitrate: 1411 },
  aiff: { extension: 'aiff', bitrate: 1411 }
};

/** 可逆の形式の拡張子（`m4a`は、ビットレートから決める） */
const LOSSLESS_EXTENSIONS = ['flac', 'wav', 'aiff', 'aif', 'wv', 'ape'];

/** 音量の解析で、1曲を読むのにかかることにする時間（進捗と中断を目視できるようにする） */
const LOUDNESS_SCAN_STEP_MS = 250;
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

/**
 * 転送先のフォルダから、デバイス名を決める（決められなければnull。Rustの`suggest_device_name`と同じ）
 *
 * 取り外せるドライブ（`/Volumes/名前`・`/media/ユーザー/名前`・`/run/media/ユーザー/名前`）の
 * 中ならドライブの名前、それ以外はフォルダの名前にする。
 */
function suggestDeviceName(folderPath: string): string | null {
  // Windowsのドライブ（`E:`）は、名前にしない
  const segments = folderPath.split(/[\\/]/).filter((s) => s && !/^[a-zA-Z]:$/.test(s));
  let name = segments.at(-1);
  if (segments[0] === 'Volumes' && segments.length >= 2) name = segments[1];
  else if (segments[0] === 'media' && segments.length >= 3) name = segments[2];
  else if (segments[0] === 'run' && segments[1] === 'media' && segments.length >= 4) {
    name = segments[3];
  }
  const cleaned = [...(name ?? '').replace(/\p{Cc}/gu, '')].slice(0, 100).join('').trim();
  return cleaned || null;
}

function validatePlaylistName(name: string): void {
  if (!name.trim()) fail('VALIDATION', 'プレイリスト名を入力してください');
  if ([...name].length > 100) fail('VALIDATION', 'プレイリスト名は100文字以内で入力してください');
  if (/[<>:"/\\|?*]/.test(name)) {
    fail('VALIDATION', 'プレイリスト名に使用できない文字が含まれています');
  }
  if (name.includes('\0')) fail('VALIDATION', '不正なプレイリスト名: Null文字が含まれています');
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

/**
 * 一覧でまとめるアーティスト（`name`）の、並び順に使う値
 *
 * 曲のアーティストが複数ある曲は、読みも同じ数だけ区切って書かれている場合に、順に対応させる
 * （Rustの`find_artist_summaries`と同じ）。
 */
function listedArtistSort(track: Track, name: string): string | null {
  if (track.albumArtist !== null) return groupArtistSort(track);
  const names = splitValues(track.artist);
  const sortNames = splitValues(track.sortTags.artist);
  return sortNames.length === names.length ? (sortNames[names.indexOf(name)] ?? null) : null;
}

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
    artistSortName: firstSortName(tracks, groupArtistSort),
    trackCount: tracks.length,
    totalDuration: sumDuration(tracks),
    representativeTrackId: tracks[0]?.id ?? ''
  };
}

/**
 * 曲を、アルバムごとにまとめる（アルバム名の順。Rustの`group_album_tracks`と同じ）
 *
 * アルバムのない曲は、`UNKNOWN_ALBUM`という名前のアルバムにまとめる。
 */
function toAlbumGroups(items: Track[]): AlbumGroup[] {
  const sorted = [...items].sort(
    orderBy((t) => t.album ?? UNKNOWN_ALBUM, groupArtist, ...albumTrackOrder)
  );
  const keyOf = (track: Track) =>
    JSON.stringify([track.album ?? UNKNOWN_ALBUM, groupArtist(track)]);
  return [...groupBy(sorted, keyOf)]
    .map(([, albumTracks]): AlbumGroup => {
      // アルバムの曲の一覧（`AlbumGroup`）には、まとめたアーティストの読みを含めない
      const { artistSortName: _, ...summary } = toAlbumSummary(
        albumTracks[0].album ?? UNKNOWN_ALBUM,
        groupArtist(albumTracks[0]),
        albumTracks
      );
      return { ...summary, tracks: albumTracks };
    })
    .sort(bySortName);
}

export function createMockBackend(options: MockBackendOptions): MockBackend {
  const sleep =
    options.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));

  let tracks: Track[] = [
    ...createFixtureTracks(),
    ...createBulkTracks(options.extraTrackCount ?? 0)
  ];
  let playlists: Playlist[] = createFixturePlaylists();
  let playlistFolders: PlaylistFolder[] = createFixturePlaylistFolders();
  // ミニプレーヤー（メインウィンドウの小さな表示）にしているか・常に手前に表示しているか
  let miniPlayerWindow = { enabled: false, alwaysOnTop: false };
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
  let fileImportMode: MockFileImportMode = 'pick';
  /** 「このアプリで開く」で開かれ、フロントエンドがまだ受け取っていないもの */
  let openedFiles: { tracks: Track[]; errors: string[] } = { tracks: [], errors: [] };
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
    normalizationPreampDb: 0,
    gaplessPlayback: true,
    crossfadeSeconds: 0,
    outputDeviceId: null,
    watchLibraryFolders: false,
    libraryScanIntervalMinutes: 0,
    hiddenSidebarItems: [],
    autoDj: false,
    autoDjPlaylistId: null,
    stayInMenuBar: false,
    globalShortcuts: false,
    notifyTrackChange: false,
    musicFolderTemplate: DEFAULT_MUSIC_FOLDER_TEMPLATE,
    copyToMusicFolder: true,
    keepMusicFolderOrganized: false,
    autoBackup: true
  };
  // ライブラリフォルダ（existsは「フォルダが見つかるか」。外付けドライブが外れた状態を再現する）
  // 読み込まないフォルダ（exclusions）は、相対パスで持つ。OSの音楽フォルダには、登録した時に
  // 入る既知のアプリのフォルダを入れておく。音楽フォルダ（isMusicFolder）は、設定していない
  let libraryFolders: MockLibraryFolder[] = [
    {
      id: crypto.randomUUID(),
      path: SYSTEM_MUSIC_FOLDER,
      exists: true,
      addedAt: '2026-01-01T00:00:00.000Z',
      lastScannedAt: '2026-01-01T00:00:00.000Z',
      exclusions: [
        { id: crypto.randomUUID(), path: 'Music' },
        { id: crypto.randomUUID(), path: 'MusicBee' }
      ],
      isMusicFolder: false
    },
    {
      id: crypto.randomUUID(),
      path: '/Volumes/External/Music',
      exists: false,
      addedAt: '2026-01-02T00:00:00.000Z',
      lastScannedAt: null,
      exclusions: [],
      isMusicFolder: false
    }
  ];
  // 読み込まないフォルダの追加で、選んだことにするフォルダ（undefinedは、曲のある最初のフォルダ）
  let exclusionFolder: string | null | undefined = undefined;
  // ライブラリフォルダの場所の変更で、選んだことにするフォルダと、預かっている付け替え先
  let relocationFolder: string | null = DEFAULT_RELOCATION_FOLDER;
  let pendingRelocation: { folderId: string; path: string } | null = null;
  // バックアップの保存・復元で選んだことにするファイルと、預かっている復元するファイル
  let backupFile: string | null = DEFAULT_BACKUP_FILE;
  let pendingRestore: string | null = null;

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
  // 形式の変換の実行の状態（同時に実行できる変換は1つ）と、書き出すフォルダを選ぶダイアログの結果
  const conversionJob = { running: false, cancelled: false };
  let convertExportFolder: string | null = '/Users/mock/Export';
  // フォルダを選ぶダイアログ（フォルダのインポート・デバイスの登録・転送先の変更・音楽フォルダ）の結果
  let folderResult: string | null = DEFAULT_FOLDER_RESULT;
  // 音楽フォルダに新しく登録するフォルダで、見つかったことにする曲の数
  let musicFolderFileCount = IMPORT_FILE_COUNT;
  let musicFolderFreeBytes = MUSIC_FOLDER_FREE_BYTES;
  // 実行中のファイルの整理と、その中止
  let isOrganizing = false;
  let organizeCancelled = false;
  /** 参照するかコピーするかを選ぶまで預かっている取り込み（Rustの`PendingImport`） */
  let pendingImport: MockImportTargets | null = null;
  /** 音楽フォルダへコピーした曲（コピー先 → 元のファイル。同じファイルをもう一度取り込んだかの判定に使う） */
  const copiedFrom = new Map<string, string>();
  // 書き出したファイルの場所（もう一度同じ場所へ書き出すと、同じ名前のファイルがあることになる）
  const exportedPaths = new Set<string>();
  // 音量の解析の実行の状態（同時に実行できる解析は1つ）
  const loudnessScan = { running: false, cancelled: false, current: 0, total: 0 };

  const now = () => new Date().toISOString();
  const newestFirst = (a: Track, b: Track) => compareAsc(b.createdAt, a.createdAt);
  const tracksWhere = (predicate: (track: Track) => boolean) =>
    tracks.filter(predicate).sort(newestFirst);

  function findTrack(id: string): Track {
    const track = tracks.find((t) => t.id === id);
    if (!track) fail('NOT_FOUND', 'トラックが見つかりません');
    return track;
  }

  // 再生エンジン（音は鳴らさず、再生位置と曲の切り替わりを時計に合わせて進める）
  const playbackEngine = createMockPlaybackEngine({
    emit: (event) => options.emit('playbackEvent', event),
    crossfadeSeconds: () => settings.crossfadeSeconds,
    durationOf: (trackId) => {
      validateTrackId(trackId);
      const track = findTrack(trackId);
      if (track.isMissing) fail('NOT_FOUND', `ファイルが見つかりません: ${track.filePath}`);
      return track.duration;
    }
  });

  /** トラックを、値ごとにまとめる（1曲が複数の値を持つ場合は、それぞれに入る。値のないトラックは含めない） */
  function tracksByValues(values: (track: Track) => string[]): Map<string, Track[]> {
    const groups = new Map<string, Track[]>();
    for (const track of tracks) {
      for (const value of values(track)) {
        const group = groups.get(value);
        if (group) {
          group.push(track);
        } else {
          groups.set(value, [track]);
        }
      }
    }
    return groups;
  }

  /**
   * アーティストの曲を、アルバムごとにまとめる（アルバム名の順。アルバムのない曲は`UNKNOWN_ALBUM`にまとめる）
   *
   * アルバムは「アルバム名＋まとめるアーティスト」で分ける。アルバムの`artist`は、そのアルバムを
   * まとめるアーティスト（複数のアーティストの曲のアルバムでは、`A; B`の形）。
   */

  /** ジャンルの曲を並べる（アルバムをまとめるアーティスト → アルバム → アルバムの中の並び） */
  const sortGenreTracks = (items: Track[]) =>
    [...items].sort(orderBy(groupArtist, (t) => t.album, ...albumTrackOrder));

  function findPlaylist(id: string): Playlist {
    const playlist = playlists.find((p) => p.id === id);
    if (!playlist) fail('NOT_FOUND', 'プレイリストが見つかりません');
    return playlist;
  }

  function findPlaylistFolder(id: string): PlaylistFolder {
    if (!UUID_PATTERN.test(id)) fail('VALIDATION', '不正なフォルダID形式です');
    const folder = playlistFolders.find((f) => f.id === id);
    if (!folder) fail('NOT_FOUND', 'フォルダが見つかりません');
    return folder;
  }

  function validatePlaylistFolderName(name: string): void {
    if (!name.trim()) fail('VALIDATION', 'フォルダ名を入力してください');
    if ([...name].length > 100) fail('VALIDATION', 'フォルダ名は100文字以内で入力してください');
  }

  /** 手動の並び順で、いちばん後ろになる位置 */
  const nextPosition = (items: readonly { position: number }[]) =>
    Math.max(-1, ...items.map((item) => item.position)) + 1;

  /** 新しいプレイリスト（フォルダの外。手動の並び順では、いちばん後ろ） */
  function newPlaylist(name: string, rules: SmartRules | null, trackIds: string[] = []): Playlist {
    const timestamp = now();
    const playlist: Playlist = {
      id: crypto.randomUUID(),
      name,
      description: null,
      tracks: trackIds.map((trackId, position) => ({ trackId, position, addedAt: timestamp })),
      rules,
      folderId: null,
      position: nextPosition(playlists),
      createdAt: timestamp,
      updatedAt: timestamp
    };
    playlists.push(playlist);
    return playlist;
  }

  // 自動プレイリストの、ランダムな並びの種（「選び直す」で変える）
  let shuffleSeed = 1;
  const smartContext = (): SmartPlaylistContext => ({ now: Date.now(), shuffleSeed });

  function validateSmartRules(rules: SmartRules): void {
    const error = smartRulesError(rules);
    if (error !== null) fail('VALIDATION', error);
  }

  /** プレイリストの曲（プレイリストの中の並び順。自動プレイリストは、条件に合う曲） */
  function playlistTracks(playlist: Playlist): Track[] {
    if (playlist.rules !== null) {
      return findSmartPlaylistTracks(tracks, playlist.rules, smartContext());
    }
    const byId = new Map(tracks.map((track) => [track.id, track]));
    return [...playlist.tracks]
      .sort((a, b) => a.position - b.position)
      .map((entry) => byId.get(entry.trackId))
      .filter((track): track is Track => track !== undefined);
  }

  /** 自動プレイリストの曲は条件で決まるため、追加・削除・並べ替えはできない */
  function ensureManualPlaylist(playlistId: string): void {
    if (playlists.some((p) => p.id === playlistId && p.rules !== null)) {
      fail('VALIDATION', '自動プレイリストの曲は条件で決まるため、変更できません');
    }
  }

  /** 項目の値の一覧（アーティスト・ジャンルは、複数ある曲のそれぞれの値） */
  function uniqueValues(key: 'artist' | 'album' | 'genre'): string[] {
    const values = new Set<string>();
    for (const track of tracks) {
      const value = track[key];
      if (value === null) continue;
      for (const each of key === 'album' ? [value] : splitValues(value)) values.add(each);
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
   * アーティスト・ジャンルの入力を、複数の値の形にそろえる（`A ;B` → `A; B`。Rustの
   * `normalize_multi_values`と同じ）。複数の値でない入力は、そのまま
   */
  function normalizeMultiValue<T extends string | null | undefined>(value: T): T | string {
    return typeof value === 'string' && value.includes(VALUE_SEPARATOR)
      ? splitValues(value).join(`${VALUE_SEPARATOR} `)
      : value;
  }

  /**
   * update_track_metadataと同様、title/artist/album/genre/yearを丸ごと置き換える
   * （実装はファイルのタグへ書き込むが、モックはライブラリの値だけを変える）
   */
  function updateTrackMetadata(trackId: string, metadata: Metadata): KeepOrganizedResult | null {
    validateTrackId(trackId);
    validateMetadataInput(metadata);
    const track = tracks.find((t) => t.id === trackId);
    if (!track) fail('NOT_FOUND', '指定されたトラックが見つかりません');
    // 1曲の編集は、すべての項目を置き換える（値のない項目は空にする）
    track.title = metadata.title ?? null;
    track.artist = normalizeMultiValue(metadata.artist) || null;
    track.album = metadata.album ?? null;
    track.genre = normalizeMultiValue(metadata.genre) || null;
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
    return keepMusicFolderOrganized([trackId]);
  }

  /**
   * タグを保存した曲のうち、音楽フォルダの中の曲を、書式どおりの場所へ移し直す（Rustの
   * `keep_music_folder_organized`。設定がオフ・音楽フォルダの中の曲がなければnull）
   */
  function keepMusicFolderOrganized(trackIds: string[]): KeepOrganizedResult | null {
    if (!settings.keepMusicFolderOrganized) return null;
    const musicFolder = libraryFolders.find((folder) => folder.isMusicFolder);
    if (!musicFolder || !musicFolder.exists) return null;
    const inside = tracks.filter(
      (track) => trackIds.includes(track.id) && track.filePath.startsWith(`${musicFolder.path}/`)
    );
    if (inside.length === 0) return null;
    const { entries, moves } = planOrganize(
      inside,
      libraryFolders.map((folder) => folder.path),
      parseOrganizeTemplate(settings.musicFolderTemplate),
      tracks,
      musicFolder.path
    );
    for (const move of moves) {
      const track = tracks.find((t) => t.id === move.trackId);
      if (!track) continue;
      track.filePath = move.to;
      track.fileName = move.to.slice(move.to.lastIndexOf('/') + 1);
    }
    if (moves.length > 0) options.emit('libraryChanged', null);
    return {
      movedCount: moves.length,
      notMovedCount: entries.filter(
        (entry) => entry.status !== 'move' && entry.status !== 'unchanged'
      ).length,
      errors: []
    };
  }

  /**
   * 値のある項目だけを、トラックに反映する（一括編集・タグの一括ツール）
   *
   * 文字列の項目は、空の値で項目を空にする（Rustの`update_track_metadata_partial`と同じ）。
   */
  function applyPartialMetadata(track: Track, metadata: Metadata, timestamp: string): void {
    const text = (value: string | null | undefined, current: string | null) =>
      value == null ? current : value === '' ? null : value;
    const { year, trackNumber, discNumber } = metadata;
    const before = JSON.stringify(track);

    track.title = text(metadata.title, track.title);
    track.artist = text(normalizeMultiValue(metadata.artist), track.artist);
    track.album = text(metadata.album, track.album);
    track.genre = text(normalizeMultiValue(metadata.genre), track.genre);
    track.albumArtist = text(metadata.albumArtist, track.albumArtist);
    if (year != null) track.year = year;
    if (trackNumber != null) track.trackNumber = trackNumber;
    if (discNumber != null) track.discNumber = discNumber;
    track.sortTags = {
      title: text(metadata.titleSort, track.sortTags.title),
      artist: text(metadata.artistSort, track.sortTags.artist),
      album: text(metadata.albumSort, track.sortTags.album),
      albumArtist: text(metadata.albumArtistSort, track.sortTags.albumArtist)
    };
    if (JSON.stringify(track) !== before) track.updatedAt = timestamp;

    // データベースに保存しない項目は、値のある項目だけを変える
    const fileOnly = Object.fromEntries(
      Object.entries(pickFileOnlyTags(metadata)).filter(([, value]) => value != null)
    );
    const merged: Metadata = { ...fileOnlyTags.get(track.id), ...fileOnly };
    // コンピレーションの印を外す指定は、項目ごと取り除く
    if (merged.compilation === false) delete merged.compilation;
    fileOnlyTags.set(track.id, merged);
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
    // アルバムアートは置き場所に関わらないため、音楽フォルダの曲を移し直さない
    const result: BulkUpdateResult = {
      updatedCount: 0,
      failedCount: 0,
      errors: [],
      organized: null
    };
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
   * フォルダのインポート（フォルダは`setFolderResult`で選んだことにしたもの。nullなら選ばずに閉じた）
   *
   * 実際のスキャンの代わりに、フォルダパスから決定的なファイル一覧を生成する。
   * 同じフォルダを再度インポートすると、2回目は重複として扱われる。
   */
  async function importFolder(duplicateAction: DuplicateAction): Promise<ImportOutcome | null> {
    if (folderResult === null) return null;
    return startImport(
      { files: [], folders: [normalizeFolderPath(folderResult)] },
      duplicateAction
    );
  }

  /** フォルダのインポートで「見つかった」ことにするファイルの名前 */
  const mockFileNames = (count: number) =>
    Array.from(
      { length: count },
      (_, index) => `Mock Track ${String(index + 1).padStart(2, '0')}.mp3`
    );

  /**
   * 音楽フォルダを選ぶ（Rustの`commands/music_folder.rs`の`change_music_folder`）
   *
   * OSの音楽フォルダは`SYSTEM_MUSIC_FOLDER`、ほかのフォルダは`setFolderResult`で選んだことにしたもの
   * （nullなら、選ばずに閉じた）。登録済みのライブラリフォルダなら印を移すだけ、その中のフォルダなら
   * 外側のフォルダを返す。新しいフォルダは、`setMusicFolderFileCount`の数の曲を読み込んで登録する
   * （0なら空のフォルダ）。
   */
  async function setMusicFolder(source: 'system' | 'pick'): Promise<MusicFolderChange | null> {
    const selected = source === 'system' ? SYSTEM_MUSIC_FOLDER : folderResult;
    if (selected === null) return null;
    const path = normalizeFolderPath(selected);
    const registered = libraryFolders.find((folder) => folder.path === path);
    const enclosing = libraryFolders.find(
      (folder) => folder.path !== path && isSameOrWithin(path, folder.path)
    );
    let change: MusicFolderChange;
    if (registered) {
      markMusicFolder(registered);
      change = { kind: 'set', path, imported: null };
    } else if (enclosing) {
      return { kind: 'insideLibraryFolder', path, libraryFolderPath: enclosing.path };
    } else {
      const imported = await importAudioFiles(path, mockFileNames(musicFolderFileCount), 'Skip');
      markMusicFolder(registerLibraryFolder(path)!);
      change = { kind: 'set', path, imported };
    }
    options.emit('libraryChanged', null);
    return change;
  }

  /** ライブラリフォルダを音楽フォルダにする（ほかのフォルダの印は外す） */
  function markMusicFolder(target: MockLibraryFolder): void {
    for (const folder of libraryFolders) folder.isMusicFolder = folder === target;
  }

  /**
   * 音楽フォルダの書式の例（Rustの`preview_music_folder_template`）
   *
   * 最近追加したアルバム（アルバムアーティスト・アルバムごと）の最初の曲を、書式に当てはめる。
   */
  function previewMusicFolderTemplate(templateText: string): string[] {
    const template = parseOrganizeTemplate(templateText);
    const albums = new Map<string, { first: Track; addedAt: string }>();
    const order = (track: Track) =>
      [track.discNumber ?? -1, track.trackNumber ?? -1, track.filePath] as const;
    for (const track of tracks.filter((track) => !track.isMissing)) {
      const key = JSON.stringify([track.album, track.albumArtist ?? track.artist]);
      const album = albums.get(key);
      if (!album) {
        albums.set(key, { first: track, addedAt: track.createdAt });
        continue;
      }
      if (track.createdAt > album.addedAt) album.addedAt = track.createdAt;
      const [disc, number, path] = order(track);
      const [firstDisc, firstNumber, firstPath] = order(album.first);
      if (
        disc < firstDisc ||
        (disc === firstDisc &&
          (number < firstNumber || (number === firstNumber && path < firstPath)))
      ) {
        album.first = track;
      }
    }
    return [...albums.values()]
      .sort(
        (a, b) => compareAsc(b.addedAt, a.addedAt) || compareAsc(a.first.filePath, b.first.filePath)
      )
      .slice(0, TEMPLATE_EXAMPLE_COUNT)
      .map((album) => organizedRelativePath(template, album.first));
  }

  /** 音楽ファイルか（拡張子で決める。大文字と小文字は区別しない） */
  function isAudioFileName(fileName: string): boolean {
    const extension = fileName.includes('.') ? fileName.split('.').pop()!.toLowerCase() : '';
    return AUDIO_EXTENSIONS.includes(extension);
  }

  /**
   * フォルダにあることにした音楽ファイルを、ライブラリへ登録する（フォルダ・ファイルのインポートで共通）
   *
   * 実ファイルは存在しないため、曲の内容はファイル名から決める。移動・改名されたファイルの
   * 対応付けは再現しない。
   */
  async function importAudioFiles(
    folder: string,
    fileNames: string[],
    duplicateAction: DuplicateAction
  ): Promise<ImportResult> {
    const result = emptyImportResult();

    for (const [index, fileName] of fileNames.entries()) {
      const filePath = `${folderPrefix(folder)}${fileName}`;
      options.emit('importProgress', {
        phase: 'reading',
        current: index + 1,
        total: fileNames.length,
        currentFile: fileName
      });
      await sleep(IMPORT_STEP_MS);

      const existing = tracks.find((track) => track.filePath === filePath);
      if (existing && duplicateAction === 'Skip') {
        result.skippedCount++;
        continue;
      }

      if (existing) {
        existing.updatedAt = now();
      } else {
        tracks.push(mockTrack(folder, fileName, index));
      }
      result.importedCount++;
    }

    return result;
  }

  const emptyImportResult = (): ImportResult => ({
    importedCount: 0,
    skippedCount: 0,
    relinkedCount: 0,
    errorCount: 0,
    errors: [],
    copiedCount: 0,
    notCopied: []
  });

  /**
   * フォルダにあることにした音楽ファイルの、曲の内容（実ファイルはないため、ファイル名から決める）
   * @param index - フォルダの中での順番（トラック番号・長さに使う）
   */
  function mockTrack(folder: string, fileName: string, index: number): Track {
    const album = folder.split(/[\\/]/).filter(Boolean).pop() || 'Imported';
    const duration = 150 + index * 17;
    const timestamp = now();
    return {
      id: crypto.randomUUID(),
      filePath: `${folderPrefix(folder)}${fileName}`,
      fileName,
      title: fileName.replace(/\.[^.]+$/, ''),
      artist: 'Mock Importer',
      album,
      albumArtist: null,
      genre: 'Demo',
      year: 2026,
      trackNumber: index + 1,
      discNumber: 1,
      duration,
      fileSize: duration * 40_000,
      format: fileFormatName(fileName.split('.').pop()!),
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
    };
  }

  /**
   * 取り込む対象を、ファイルとフォルダに分ける（ドロップ・ファイルを選ぶインポート）
   *
   * 実装は、パスがフォルダかを調べる。モックは、拡張子のない名前をフォルダとする。音楽ファイルでも
   * フォルダでもないものは読み飛ばす。
   */
  function importTargetsIn(folder: string, names: string[]): MockImportTargets {
    return {
      files: names.filter(isAudioFileName).map((fileName) => ({ folder, fileName })),
      folders: names
        .filter((name) => !name.includes('.'))
        .map((name) => `${folderPrefix(folder)}${name}`)
    };
  }

  /** 取り込む曲をコピーする音楽フォルダ（設定がオフか、音楽フォルダが未設定ならnull） */
  const copyDestination = () =>
    settings.copyToMusicFolder
      ? (libraryFolders.find((folder) => folder.isMusicFolder) ?? null)
      : null;

  /** 曲が、すでにライブラリにあるか（登録済みか、ライブラリフォルダの中のファイル） */
  const isInLibrary = (filePath: string) =>
    tracks.some((track) => track.filePath === filePath) ||
    libraryFolders.some((folder) => isSameOrWithin(filePath, folder.path));

  /** 取り込む曲（ファイルと、フォルダの中の曲。フォルダは`IMPORT_FILE_COUNT`曲あることにする） */
  const importSources = (targets: MockImportTargets) => [
    ...targets.files,
    ...targets.folders.flatMap((folder) =>
      mockFileNames(IMPORT_FILE_COUNT).map((fileName) => ({ folder, fileName }))
    )
  ];

  /**
   * 取り込みを始める（Rustの`start_import`）
   *
   * 取り込む曲を音楽フォルダへコピーする設定で、ライブラリフォルダの外のフォルダがあれば、取り込まずに
   * 預かる（コピーする曲がなければ、選ばせずに参照する）。
   */
  async function startImport(
    targets: MockImportTargets,
    duplicateAction: DuplicateAction
  ): Promise<ImportOutcome> {
    if (targets.files.length === 0 && targets.folders.length === 0) {
      fail('VALIDATION', 'インポートできる音楽ファイルがありません');
    }
    const musicFolder = copyDestination();
    if (
      musicFolder &&
      targets.folders.some(
        (folder) => !libraryFolders.some((library) => isSameOrWithin(folder, library.path))
      )
    ) {
      const pending = pendingSummary(targets, musicFolder);
      if (pending.copyCount === 0) return (await runImport(targets, duplicateAction, null)).outcome;
      // 実装と同じく、預かっている取り込みがあれば置き換えない
      if (pendingImport) {
        fail(
          'VALIDATION',
          '取り込み方（参照かコピーか）を選んでいるフォルダがあります。先に選ぶか、やめてから取り込んでください'
        );
      }
      pendingImport = targets;
      return { kind: 'chooseFolderMode', pending };
    }
    return (await runImport(targets, duplicateAction, musicFolder)).outcome;
  }

  /** 預かるフォルダの、画面に出す内容（Rustの`summarize_pending`） */
  function pendingSummary(
    targets: MockImportTargets,
    musicFolder: MockLibraryFolder
  ): PendingFolderImport {
    const sources = importSources(targets).map(({ folder, fileName }, index) =>
      mockTrack(folder, fileName, index)
    );
    const copied = sources.filter((track) => !isInLibrary(track.filePath));
    const bytes = (list: Track[]) => list.reduce((sum, track) => sum + track.fileSize, 0);
    return {
      folders: targets.folders,
      trackCount: sources.length,
      totalBytes: bytes(sources),
      copyCount: copied.length,
      copyBytes: bytes(copied),
      musicFolder: musicFolder.path,
      musicFolderExists: musicFolder.exists,
      freeBytes: musicFolder.exists ? musicFolderFreeBytes : null
    };
  }

  /**
   * 取り込み、ライブラリの変更を知らせる（Rustの`import_and_notify`）
   * @param musicFolder - コピーする音楽フォルダ（nullなら、参照して取り込み、フォルダを記録する）
   * @returns 結果と、音楽フォルダへコピーした曲（とコピー済みだった曲）の、元のパス → 登録したパス
   */
  async function runImport(
    targets: MockImportTargets,
    duplicateAction: DuplicateAction,
    musicFolder: MockLibraryFolder | null
  ): Promise<{ outcome: ImportOutcome; placed: Map<string, string> }> {
    const placed = new Map<string, string>();
    let result = emptyImportResult();
    if (musicFolder === null) {
      for (const folder of new Set(targets.files.map((file) => file.folder))) {
        const fileNames = targets.files
          .filter((file) => file.folder === folder)
          .map((file) => file.fileName);
        mergeImportResult(result, await importAudioFiles(folder, fileNames, duplicateAction));
      }
      for (const folder of targets.folders) {
        mergeImportResult(
          result,
          await importAudioFiles(folder, mockFileNames(IMPORT_FILE_COUNT), duplicateAction)
        );
        // Rustと同じく、フォルダをライブラリフォルダとして記録する
        registerLibraryFolder(folder);
      }
    } else {
      if (!musicFolder.exists) {
        return { outcome: { kind: 'musicFolderMissing', path: musicFolder.path }, placed };
      }
      const copied = await copyIntoMusicFolder(
        importSources(targets),
        duplicateAction,
        musicFolder,
        placed
      );
      if (copied.kind !== 'imported') return { outcome: copied, placed };
      result = copied.result;
    }
    options.emit('libraryChanged', null);
    return { outcome: { kind: 'imported', result }, placed };
  }

  function mergeImportResult(result: ImportResult, other: ImportResult): void {
    result.importedCount += other.importedCount;
    result.skippedCount += other.skippedCount;
    result.relinkedCount += other.relinkedCount;
    result.errorCount += other.errorCount;
    result.errors.push(...other.errors);
    result.copiedCount += other.copiedCount;
    result.notCopied.push(...other.notCopied);
  }

  /**
   * 取り込む曲を、音楽フォルダへコピーしてから登録する（Rustの`import_with_copy`）
   *
   * ライブラリにある曲は参照し、コピー先に同じ名前の曲（ファイル）がある曲・同じコピー先になる曲は、
   * 取り込まずに知らせる。同じファイルを前にコピーしていれば、コピー済みの曲として扱う。空き容量が
   * 足りなければ、何も取り込まない。
   */
  async function copyIntoMusicFolder(
    sources: { folder: string; fileName: string }[],
    duplicateAction: DuplicateAction,
    musicFolder: MockLibraryFolder,
    placed: Map<string, string>
  ): Promise<ImportOutcome> {
    const result = emptyImportResult();
    const template = parseOrganizeTemplate(settings.musicFolderTemplate);
    const key = (path: string) => path.normalize('NFC').toLowerCase();
    const referenced: { folder: string; fileName: string }[] = [];
    const candidates: { track: Track; target: string }[] = [];
    const toCopy = sources.filter(({ folder, fileName }) => {
      if (!isInLibrary(`${folderPrefix(folder)}${fileName}`)) return true;
      referenced.push({ folder, fileName });
      return false;
    });
    for (const [index, { folder, fileName }] of toCopy.entries()) {
      options.emit('importProgress', {
        phase: 'reading',
        current: index + 1,
        total: toCopy.length,
        currentFile: fileName
      });
      await sleep(IMPORT_STEP_MS / 3);
      const track = mockTrack(folder, fileName, index);
      candidates.push({
        track,
        target: `${folderPrefix(musicFolder.path)}${organizedRelativePath(template, track)}`
      });
    }

    const users = new Map<string, number>();
    for (const { target } of candidates) users.set(key(target), (users.get(key(target)) ?? 0) + 1);
    const copies: typeof candidates = [];
    for (const candidate of candidates) {
      const notCopied = (reason: NotCopied['reason']): NotCopied => ({
        source: candidate.track.filePath,
        target: candidate.target.slice(folderPrefix(musicFolder.path).length),
        reason
      });
      if ((users.get(key(candidate.target)) ?? 0) > 1) {
        result.notCopied.push(notCopied('sameTarget'));
      } else if (
        tracks.some((track) => track.isMissing && key(track.filePath) === key(candidate.target))
      ) {
        // ファイルはないが、ライブラリの見つからない曲の場所（実装と同じく、コピー先にしない）
        result.notCopied.push(notCopied('targetRegistered'));
      } else if (tracks.some((track) => key(track.filePath) === key(candidate.target))) {
        if (copiedFrom.get(candidate.target) === candidate.track.filePath) {
          // 同じファイルを前にコピーした: コピー先の曲を参照する
          const slash = candidate.target.lastIndexOf('/');
          referenced.push({
            folder: candidate.target.slice(0, slash),
            fileName: candidate.target.slice(slash + 1)
          });
          placed.set(candidate.track.filePath, candidate.target);
        } else {
          result.notCopied.push(notCopied('targetExists'));
        }
      } else {
        copies.push(candidate);
      }
    }

    const requiredBytes = copies.reduce((sum, { track }) => sum + track.fileSize, 0);
    if (requiredBytes > 0 && requiredBytes + COPY_SPACE_MARGIN_BYTES > musicFolderFreeBytes) {
      return { kind: 'notEnoughSpace', requiredBytes, freeBytes: musicFolderFreeBytes };
    }

    for (const folder of new Set(referenced.map((source) => source.folder))) {
      const fileNames = referenced
        .filter((source) => source.folder === folder)
        .map((source) => source.fileName);
      mergeImportResult(result, await importAudioFiles(folder, fileNames, duplicateAction));
    }
    for (const [index, { track, target }] of copies.entries()) {
      options.emit('importProgress', {
        phase: 'copying',
        current: index + 1,
        total: copies.length,
        currentFile: track.fileName
      });
      await sleep(IMPORT_STEP_MS);
      tracks.push({
        ...track,
        filePath: target,
        fileName: target.slice(target.lastIndexOf('/') + 1)
      });
      copiedFrom.set(target, track.filePath);
      placed.set(track.filePath, target);
      musicFolderFreeBytes -= track.fileSize;
      result.importedCount++;
      result.copiedCount++;
    }
    return { kind: 'imported', result };
  }

  /**
   * 預かっているフォルダを、選んだ方法で取り込む（Rustの`import_pending_folders`）
   *
   * 音楽フォルダへコピーできなかった場合は、預かったままにする。
   */
  async function importPendingFolders(
    mode: FolderImportMode,
    duplicateAction: DuplicateAction
  ): Promise<ImportOutcome> {
    const targets = pendingImport;
    if (!targets) fail('NOT_FOUND', '取り込むフォルダがありません');
    pendingImport = null;
    let musicFolder: MockLibraryFolder | null = null;
    if (mode === 'copy') {
      musicFolder = libraryFolders.find((folder) => folder.isMusicFolder) ?? null;
      if (!musicFolder) fail('NOT_FOUND', '音楽フォルダが設定されていません');
    }
    const { outcome } = await runImport(targets, duplicateAction, musicFolder);
    // 預かり直す（その間に新しく預かったものは置き換えない）
    if (outcome.kind !== 'imported') pendingImport ??= targets;
    return outcome;
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

  /**
   * 再スキャンの進み具合を送る（実装は、追加・変更のあったファイルを読むたびに送る。モックは、
   * フォルダの最初の数曲を読んだことにして、進捗の表示を確かめられるようにする）
   */
  async function sendRescanProgress(folder: string): Promise<void> {
    const files = tracksUnder(folder).slice(0, RESCAN_PROGRESS_STEPS);
    for (const [index, track] of files.entries()) {
      options.emit('libraryScanProgress', {
        current: index + 1,
        total: files.length,
        currentFile: track.fileName
      });
      await sleep(IMPORT_STEP_MS);
    }
  }

  /**
   * 選んだ曲（選んだ順）の、変換の計画（Rustの`transcode::plan`）
   * @param exportFolder - 書き出すフォルダ（選ぶ前はnull。その間は、同じ名前のファイルを調べない）
   */
  function conversionPlan(
    trackIds: string[],
    convertOptions: ConvertOptions,
    exportFolder: string | null
  ) {
    if (trackIds.length === 0) fail('VALIDATION', '変換する曲が指定されていません');
    trackIds.forEach(validateTrackId);
    const { format, mode } = convertOptions;
    if (
      format === 'aac' &&
      !(AAC_BITRATES_KBPS as readonly number[]).includes(convertOptions.aacBitrateKbps)
    ) {
      fail('VALIDATION', 'AACのビットレートが選べる値ではありません');
    }
    const template = parseOrganizeTemplate(convertOptions.template);
    const extension = `.${CONVERT_OUTPUTS[format].extension}`;
    const key = (path: string) => path.normalize('NFC').toLowerCase();
    // 実装と同じく、可逆かどうかは拡張子で決める（形式の名前ではなく）
    const isLossless = (track: Track) => {
      const sourceExtension = track.fileName.split('.').pop()!.toLowerCase();
      if (LOSSLESS_EXTENSIONS.includes(sourceExtension)) return true;
      return sourceExtension === 'm4a' ? (track.bitrate ?? 0) > 500 : false;
    };

    const entries: ConvertEntry[] = [];
    const candidates: { index: number; track: Track; output: string }[] = [];
    const selected = [...new Set(trackIds)]
      .map((id) => tracks.find((track) => track.id === id))
      .filter((track) => track !== undefined);
    for (const track of selected) {
      const entry: ConvertEntry = {
        trackId: track.id,
        sourceName: track.fileName,
        outputPath: null,
        status: 'convert',
        sourceLossless: isLossless(track)
      };
      if (track.isMissing) {
        entry.status = 'missing';
      } else if (mode === 'export') {
        const relative = organizedRelativePath(template, track, extension);
        entry.outputPath = relative;
        candidates.push({
          index: entries.length,
          track,
          output: exportFolder === null ? relative : `${exportFolder}/${relative}`
        });
      } else {
        const output = track.filePath.replace(/\.[^./]+$/, '') + extension;
        entry.outputPath = output.slice(output.lastIndexOf('/') + 1);
        candidates.push({ index: entries.length, track, output });
      }
      entries.push(entry);
    }

    const users: Record<string, number> = Object.create(null);
    for (const { output } of candidates) users[key(output)] = (users[key(output)] ?? 0) + 1;
    const conversions: { track: Track; output: string }[] = [];
    for (const { index, track, output } of candidates) {
      const occupied =
        mode === 'replace'
          ? key(output) !== key(track.filePath) &&
            tracks.some((other) => key(other.filePath) === key(output))
          : exportFolder !== null && exportedPaths.has(key(output));
      if (users[key(output)] > 1) {
        entries[index].status = 'sameTarget';
      } else if (occupied) {
        entries[index].status = 'targetExists';
      } else {
        conversions.push({ track, output });
      }
    }
    return { entries, conversions };
  }

  /**
   * 選んだ曲を変換する（Rustの`commands/convert.rs`）
   *
   * 実ファイルはないため、書き出しは場所を覚えるだけにし、置き換えは曲の場所と形式を書き換える。
   */
  async function convertTracks(
    trackIds: string[],
    convertOptions: ConvertOptions
  ): Promise<ConvertResult | null> {
    // 入力の誤りは、実行中かどうかを調べる前に知らせる
    conversionPlan(trackIds, convertOptions, null);
    if (conversionJob.running) fail('LOCK', '形式の変換を実行中です');
    const { format, mode } = convertOptions;
    const exportFolder = mode === 'export' ? convertExportFolder : null;
    // 書き出すフォルダを選ぶダイアログで、選ばなかった
    if (mode === 'export' && exportFolder === null) return null;

    const { entries, conversions } = conversionPlan(trackIds, convertOptions, exportFolder);
    const result: ConvertResult = {
      convertedCount: 0,
      skippedCount: entries.filter((entry) => entry.status !== 'convert').length,
      failedCount: 0,
      errors: [],
      cancelled: false
    };
    const output = CONVERT_OUTPUTS[format];
    const bitrate = output.bitrate ?? convertOptions.aacBitrateKbps;
    Object.assign(conversionJob, { running: true, cancelled: false });
    let libraryChanged = false;
    try {
      for (const conversion of conversions) {
        if (conversionJob.cancelled) break;
        await sleep(CONVERT_STEP_MS);
        if (conversionJob.cancelled) break;
        const { track } = conversion;
        const sourceName = track.fileName;
        const converted = {
          filePath: conversion.output,
          fileName: conversion.output.slice(conversion.output.lastIndexOf('/') + 1),
          format: fileFormatName(output.extension),
          bitrate
        };
        if (mode === 'replace') {
          Object.assign(track, converted, { updatedAt: now() });
          libraryChanged = true;
        } else {
          exportedPaths.add(conversion.output.normalize('NFC').toLowerCase());
          if (convertOptions.addToLibrary) {
            tracks.push({
              ...structuredClone(track),
              ...converted,
              id: crypto.randomUUID(),
              isFavorite: false,
              playCount: 0,
              skipCount: 0,
              lastPlayedAt: null,
              createdAt: now(),
              updatedAt: now()
            });
            libraryChanged = true;
          }
        }
        result.convertedCount++;
        options.emit('convertProgress', {
          current: result.convertedCount,
          total: conversions.length,
          currentFile: sourceName
        });
      }
      result.cancelled = conversionJob.cancelled;
    } finally {
      conversionJob.running = false;
    }
    if (libraryChanged) options.emit('libraryChanged', null);
    return result;
  }

  /** 音量の解析で、曲をまとめるアルバムのキー（Rustの`loudness_scan::album_key`。アルバムのない曲はnull） */
  const loudnessAlbumKey = (track: Track) =>
    track.album ? `${track.album}\n${track.albumArtist ?? track.artist ?? ''}` : null;

  /**
   * 音量を解析して、ゲインとピークを曲に記録する（Rustの`loudness_scan`の、まとまりごとの解析）
   *
   * 実ファイルはないため、ゲインは曲の長さから決まる値にする（同じ曲は、何度解析しても同じ値）。
   * アルバムのゲインは、アルバムの曲のゲインの平均にする。
   */
  async function startLoudnessScan(trackIds: string[] | null): Promise<LoudnessScanResult> {
    if (trackIds !== null) {
      if (trackIds.length === 0) fail('VALIDATION', '解析する曲が指定されていません');
      trackIds.forEach(validateTrackId);
    }
    if (loudnessScan.running) fail('LOCK', '音量の解析を実行中です');

    const selected = trackIds === null ? null : new Set(trackIds);
    const isTarget = (track: Track) =>
      selected === null ? track.replayGain.trackGain === null : selected.has(track.id);
    // まとまり（アルバムか、アルバムのない1曲）に分け、解析の要るまとまりだけを残す
    const groups: Record<string, Track[]> = Object.create(null);
    for (const track of tracks.filter((t) => !t.isMissing)) {
      (groups[loudnessAlbumKey(track) ?? `\n${track.id}`] ??= []).push(track);
    }
    const units = Object.values(groups).filter(
      (unit) =>
        unit.some(isTarget) ||
        (selected === null &&
          loudnessAlbumKey(unit[0]) !== null &&
          unit.some((track) => track.replayGain.albumGain === null))
    );

    const result: LoudnessScanResult = {
      writtenCount: 0,
      unchangedCount: 0,
      errorCount: 0,
      errors: [],
      cancelled: false
    };
    Object.assign(loudnessScan, {
      running: true,
      cancelled: false,
      current: 0,
      total: units.reduce((sum, unit) => sum + unit.length, 0)
    });
    const gainOf = (track: Track) => -(((track.duration ?? 0) % 90) / 10) - 3;
    try {
      for (const unit of units) {
        // 1. まとまりの全曲を読む（中断したら、このまとまりには何も書かない）
        for (const track of unit) {
          if (loudnessScan.cancelled) break;
          await sleep(LOUDNESS_SCAN_STEP_MS);
          loudnessScan.current++;
          options.emit('loudnessScanProgress', {
            current: loudnessScan.current,
            total: loudnessScan.total,
            currentFile: track.fileName
          });
        }
        if (loudnessScan.cancelled) break;

        // 2. 値を決めて書く
        const isAlbum = loudnessAlbumKey(unit[0]) !== null;
        const albumGain =
          Math.round((unit.reduce((sum, track) => sum + gainOf(track), 0) / unit.length) * 100) /
          100;
        for (const track of unit) {
          const next = { ...track.replayGain };
          if (isTarget(track)) Object.assign(next, { trackGain: gainOf(track), trackPeak: 0.95 });
          if (isAlbum) Object.assign(next, { albumGain, albumPeak: 0.95 });
          if (JSON.stringify(next) === JSON.stringify(track.replayGain)) {
            result.unchangedCount++;
          } else {
            track.replayGain = next;
            result.writtenCount++;
          }
        }
      }
      result.cancelled = loudnessScan.cancelled;
    } finally {
      loudnessScan.running = false;
    }
    if (result.writtenCount > 0) options.emit('libraryChanged', null);
    options.emit('loudnessScanFinished', result);
    return result;
  }

  /**
   * 選んだ曲（選んだ順）の、ファイルの整理の計画（Rustの`commands/organize.rs`の`build_plan`）
   *
   * 音楽フォルダへ集める時は、設定の音楽フォルダの書式を使う（`templateText`は使わない）。
   */
  function organizePlan(
    trackIds: string[],
    templateText: string,
    organizeDestination: OrganizeDestination
  ) {
    if (trackIds.length === 0) fail('VALIDATION', '整理する曲が指定されていません');
    trackIds.forEach(validateTrackId);
    let musicFolder: MockLibraryFolder | null = null;
    if (organizeDestination === 'musicFolder') {
      musicFolder = libraryFolders.find((folder) => folder.isMusicFolder) ?? null;
      if (!musicFolder) fail('NOT_FOUND', '音楽フォルダが設定されていません');
      if (!musicFolder.exists) {
        fail('NOT_FOUND', `音楽フォルダが見つかりません: ${musicFolder.path}`);
      }
    }
    const template = parseOrganizeTemplate(
      musicFolder ? settings.musicFolderTemplate : templateText
    );
    const selected = [...new Set(trackIds)]
      .map((id) => tracks.find((track) => track.id === id))
      .filter((track) => track !== undefined);
    const plan = planOrganize(
      selected,
      libraryFolders.map((folder) => folder.path),
      template,
      tracks,
      musicFolder?.path ?? null
    );
    const copies = plan.moves.filter((move) => move.copy);
    const copyBytes = copies.reduce(
      (sum, move) => sum + (tracks.find((track) => track.id === move.trackId)?.fileSize ?? 0),
      0
    );
    return { ...plan, copyCount: copies.length, copyBytes, musicFolder };
  }

  /**
   * インポートしたフォルダを記録する（フォルダ同士は入れ子にしない）
   *
   * 新しく登録するフォルダは、まとめる中のフォルダの読み込まないフォルダを引き継ぐ（Rustの
   * `import_exclusions`。モックはファイルシステムを読まないため、既知のアプリのフォルダは入れない）。
   * まとめる中のフォルダが音楽フォルダなら、新しく登録するフォルダを音楽フォルダにする。
   *
   * @returns 記録したフォルダ（同じフォルダを含む。登録済みのフォルダの中ならnull）
   */
  function registerLibraryFolder(path: string): MockLibraryFolder | null {
    const same = libraryFolders.find((folder) => folder.path === path);
    if (same) {
      same.lastScannedAt = now();
      return same;
    }
    if (libraryFolders.some((folder) => isSameOrWithin(path, folder.path))) return null;
    const nested = libraryFolders.filter((folder) => isSameOrWithin(folder.path, path));
    libraryFolders = libraryFolders.filter((folder) => !nested.includes(folder));
    const registered: MockLibraryFolder = {
      id: crypto.randomUUID(),
      path,
      exists: true,
      addedAt: now(),
      lastScannedAt: now(),
      exclusions: nested.flatMap((folder) =>
        folder.exclusions.map((exclusion) => ({
          id: crypto.randomUUID(),
          path: `${folder.path}/${exclusion.path}`.slice(folderPrefix(path).length)
        }))
      ),
      isMusicFolder: nested.some((folder) => folder.isMusicFolder)
    };
    libraryFolders.push(registered);
    return registered;
  }

  /** 変更のなかった再スキャンの結果 */
  const unchangedRescanResult = (): RescanResult => ({
    addedCount: 0,
    updatedCount: 0,
    relinkedCount: 0,
    missingCount: 0,
    restoredCount: 0,
    errorCount: 0,
    errors: [],
    missingSkipped: false
  });

  function findLibraryFolder(folderId: string) {
    if (!UUID_PATTERN.test(folderId)) fail('VALIDATION', '不正なライブラリフォルダID形式です');
    const folder = libraryFolders.find((f) => f.id === folderId);
    if (!folder) fail('NOT_FOUND', 'ライブラリフォルダが見つかりません');
    return folder;
  }

  /** 読み込まないフォルダと、それを持つライブラリフォルダ */
  function findExclusion(exclusionId: string) {
    if (!UUID_PATTERN.test(exclusionId)) {
      fail('VALIDATION', '不正な読み込まないフォルダのID形式です');
    }
    for (const folder of libraryFolders) {
      const exclusion = folder.exclusions.find((e) => e.id === exclusionId);
      if (exclusion) return { folder, exclusion };
    }
    fail('NOT_FOUND', '読み込まないフォルダが見つかりません');
  }

  /** 読み込まないフォルダの中にある曲 */
  const tracksInExclusion = (folder: MockLibraryFolder, relative: string) =>
    tracksUnder(`${folder.path}/${relative}`);

  /** 読み込まないフォルダの追加で、選んだことにするフォルダ（Rust側のダイアログの代わり） */
  function pickExclusionFolder(folder: MockLibraryFolder): string | null {
    if (exclusionFolder !== undefined) return exclusionFolder;
    // 曲のある、まだ読み込まないフォルダにしていない最初のフォルダ（アーティストのフォルダ）
    const firstSegments = tracksUnder(folder.path)
      .map((track) => track.filePath.slice(folderPrefix(folder.path).length).split('/')[0])
      .filter((segment) => !folder.exclusions.some((e) => isSameOrWithin(segment, e.path)))
      .sort(compareAsc);
    return firstSegments.length > 0 ? `${folder.path}/${firstSegments[0]}` : null;
  }

  /** 選んだフォルダを、ライブラリフォルダからの相対パスにする（Rustの`relative_to_folder`） */
  function relativeToFolder(folder: MockLibraryFolder, selected: string): string {
    const path = normalizeFolderPath(selected);
    if (path === folder.path) {
      fail('VALIDATION', 'ライブラリフォルダそのものは、読み込まないフォルダにできません');
    }
    if (!isSameOrWithin(path, folder.path)) {
      fail('VALIDATION', 'ライブラリフォルダの中のフォルダを選んでください');
    }
    return path.slice(folderPrefix(folder.path).length);
  }

  /** 付け替えると、パスの先頭を付け替えた曲の新しいパス（Rustの`replace_track_path_prefix`） */
  const relocatedPath = (track: Track, from: string, to: string) =>
    `${folderPrefix(to)}${track.filePath.slice(folderPrefix(from).length)}`;

  /**
   * 付け替えられない場所なら、その理由（Rustの`relocation_blocker`。付け替えられる場所ならnull）
   *
   * 今の場所・ほかのライブラリフォルダ・転送先デバイスのフォルダと重なる場所と、付け替えるとほかの曲と
   * 同じパスになる曲がある場所には付け替えない。
   */
  function relocationBlocker(
    folder: MockLibraryFolder,
    path: string
  ): Exclude<FolderRelocation, { kind: 'ready' }> | null {
    const overlaps = (a: string, b: string) => isSameOrWithin(a, b) || isSameOrWithin(b, a);
    if (overlaps(path, folder.path)) return { kind: 'overlapsCurrent', path };
    const other = libraryFolders.find((f) => f !== folder && overlaps(path, f.path));
    if (other) return { kind: 'overlapsLibraryFolder', path, libraryFolderPath: other.path };
    const device = syncDevices.find((d) => overlaps(path, d.path));
    if (device) return { kind: 'overlapsDevice', path, deviceName: device.name };
    const moved = tracksUnder(folder.path);
    const otherPaths = new Set(
      tracks.filter((track) => !moved.includes(track)).map((track) => track.filePath)
    );
    const conflictCount = moved.filter((track) =>
      otherPaths.has(relocatedPath(track, folder.path, path))
    ).length;
    if (conflictCount > 0) return { kind: 'conflicts', path, conflictCount };
    return null;
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
    if ([...trimmed].length > 100) fail('VALIDATION', 'デバイス名は100文字以内で入力してください');
    if (/\p{Cc}/u.test(trimmed)) fail('VALIDATION', 'デバイス名に使用できない文字が含まれています');
    return trimmed;
  }

  /** 転送先にするフォルダを検証する（ライブラリフォルダと重なるフォルダは使えない） */
  function validateDeviceFolder(folderPath: string): string {
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
      devicePlaylists(device).flatMap((playlist) => playlistTracks(playlist).map((t) => t.id))
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
        options.emit('deviceSyncProgress', {
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
    importFiles: (duplicateAction) =>
      fileImportMode === 'cancel'
        ? null
        : startImport(importTargetsIn(PICKED_FILES_FOLDER, PICKED_FILE_NAMES), duplicateAction),
    // 実装は、OSのドラッグ用のペーストボードからパスを読む。モックは、名前のファイル（拡張子のない
    // 名前はフォルダ）があることにする
    importDroppedFiles: (names) =>
      startImport(importTargetsIn(DROPPED_FILES_FOLDER, names), 'Skip'),
    importPendingFolders,
    cancelPendingImport: () => {
      pendingImport = null;
      return null;
    },
    takeOpenedFiles: () => {
      const taken = openedFiles;
      openedFiles = { tracks: [], errors: [] };
      return taken;
    },
    getAllTracks: () => tracksWhere(() => true),
    // 実装と同じく、空白で区切った語をすべて含む曲を返す（途中の一致。大文字と小文字・
    // 全角と半角・ひらがなとカタカナを同じとみなす）
    searchTracks: (query) => {
      const terms = searchTerms(query);
      if (terms.length === 0) return [];
      return tracksWhere((track) =>
        matchesSearchTerms(
          [track.title, track.artist, track.album, track.genre, track.albumArtist],
          terms
        )
      );
    },
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
    // アルバムアーティスト（なければ曲のアーティスト。複数ある曲は、それぞれ）でまとめる
    getArtists: () =>
      [...tracksByValues(listedArtists)]
        .map(([name, items]): ArtistSummary => {
          const albums = toAlbumGroups(items);
          return {
            name,
            // 読みは、アーティストが1つの曲のものを使う（複数ある曲の読みは、順に対応させる）
            sortName: firstSortName(items, (track) => listedArtistSort(track, name)),
            albumCount: albums.length,
            trackCount: items.length,
            totalDuration: sumDuration(items),
            // 詳細で最初に表示するアルバムの最初の曲
            representativeTrackId: albums[0]?.representativeTrackId ?? ''
          };
        })
        .sort(bySortName),
    getArtistAlbums: (artist) =>
      toAlbumGroups(tracks.filter((track) => listedArtists(track).includes(artist))),
    // アルバムをまとめたアーティストで取得する（複数のアーティストの曲を分けない。アルバムのない曲は含めない）
    getAlbumArtistAlbums: (artist) =>
      toAlbumGroups(
        tracks.filter((track) => track.album !== null && groupArtist(track) === artist)
      ),
    // ジャンルが複数ある曲は、それぞれのジャンルに入る
    getGenres: () =>
      [...tracksByValues((track) => splitValues(track.genre))]
        .map(([name, items]): GenreSummary => ({
          name,
          trackCount: items.length,
          totalDuration: sumDuration(items),
          representativeTrackId: sortGenreTracks(items)[0]?.id ?? ''
        }))
        .sort(bySortName),
    getGenreTracks: (genre) =>
      sortGenreTracks(tracks.filter((track) => hasValue(track.genre, genre))),
    getTrackTags,
    // 実装と同じく、曲と同じ名前の.lrcファイルを優先し、なければ埋め込みの歌詞（タグ）を返す
    getTrackLyrics: (trackId) => {
      validateTrackId(trackId);
      findTrack(trackId);
      const lrc = FIXTURE_LRC_FILES.get(trackId);
      if (lrc !== undefined) return { text: lrc, source: 'lrcFile' };
      // 編集画面で入れた歌詞（空にした場合は、歌詞なし）を、最初からある歌詞より優先する
      const tags = fileOnlyTags.get(trackId);
      const embedded =
        tags && 'lyrics' in tags ? tags.lyrics : FIXTURE_EMBEDDED_LYRICS.get(trackId);
      return embedded ? { text: embedded, source: 'embedded' } : null;
    },
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
      for (const track of targets) applyPartialMetadata(track, metadata, timestamp);
      // モックではファイルへの書き込みに失敗しない
      return {
        updatedCount: targets.length,
        failedCount: 0,
        errors: [],
        organized: keepMusicFolderOrganized(trackIds)
      };
    },
    applyMetadataChanges: (changes) => {
      if (changes.length === 0) fail('VALIDATION', '変更が指定されていません');
      // 1件でも不正な入力があれば、何も書き込まない
      for (const change of changes) {
        validateTrackId(change.trackId);
        validateMetadataInput(change.metadata);
      }
      const result: BulkUpdateResult = {
        updatedCount: 0,
        failedCount: 0,
        errors: [],
        organized: null
      };
      const updated: string[] = [];
      const timestamp = now();
      for (const { trackId, metadata } of changes) {
        const track = tracks.find((t) => t.id === trackId);
        // ファイルが見つからない・ライブラリにないトラックは、書き込めなかった曲として数える
        if (!track || track.isMissing) {
          result.failedCount++;
          result.errors.push(
            track
              ? `${track.filePath}: ファイルのオープンに失敗しました: No such file`
              : `トラックが見つかりません: ${trackId}`
          );
          continue;
        }
        applyPartialMetadata(track, metadata, timestamp);
        result.updatedCount++;
        updated.push(trackId);
      }
      result.organized = keepMusicFolderOrganized(updated);
      return result;
    },
    // モックでは、どの環境でも変換できることにする（実際は、macOSだけ）
    getConvertFormats: () => ['aac', 'alac', 'flac', 'wav', 'aiff'],
    planConversion: (trackIds, convertOptions) => ({
      entries: conversionPlan(trackIds, convertOptions, null).entries
    }),
    convertTracks,
    cancelConversion: () => {
      if (conversionJob.running) conversionJob.cancelled = true;
      return null;
    },
    getLoudnessScanStatus: () => ({
      running: loudnessScan.running,
      current: loudnessScan.running ? loudnessScan.current : 0,
      total: loudnessScan.running ? loudnessScan.total : 0,
      pendingCount: tracks.filter(
        (track) =>
          !track.isMissing &&
          (track.replayGain.trackGain === null ||
            (loudnessAlbumKey(track) !== null && track.replayGain.albumGain === null))
      ).length
    }),
    startLoudnessScan,
    cancelLoudnessScan: () => {
      if (loudnessScan.running) loudnessScan.cancelled = true;
      return null;
    },
    planFileOrganize: (trackIds, template, organizeDestination) => {
      const plan = organizePlan(trackIds, template, organizeDestination);
      return {
        entries: plan.entries,
        copyCount: plan.copyCount,
        copyBytes: plan.copyBytes,
        freeBytes: plan.musicFolder ? musicFolderFreeBytes : null
      };
    },
    // 実ファイルは存在しないため、動かせる曲は常に移せたものとして扱う（1曲ごとに進み具合を送り、
    // 中止されたら次の曲の前で止まる）
    organizeFiles: async (trackIds, template, organizeDestination) => {
      if (isOrganizing) fail('LOCK', 'ファイルの整理を実行中です');
      const { entries, moves, copyBytes, musicFolder } = organizePlan(
        trackIds,
        template,
        organizeDestination
      );
      if (
        musicFolder &&
        copyBytes > 0 &&
        copyBytes + COPY_SPACE_MARGIN_BYTES > musicFolderFreeBytes
      ) {
        fail(
          'VALIDATION',
          `音楽フォルダのドライブの空き容量が足りません（必要: ${copyBytes}バイト、空き: ${musicFolderFreeBytes}バイト）`
        );
      }
      isOrganizing = true;
      organizeCancelled = false;
      let movedCount = 0;
      try {
        for (const [index, move] of moves.entries()) {
          if (organizeCancelled) break;
          const track = tracks.find((t) => t.id === move.trackId);
          if (!track) continue;
          options.emit('organizeProgress', {
            current: index + 1,
            total: moves.length,
            currentFile: track.fileName
          });
          await sleep(ORGANIZE_STEP_MS);
          if (move.copy) musicFolderFreeBytes -= track.fileSize;
          track.filePath = move.to;
          track.fileName = move.to.slice(move.to.lastIndexOf('/') + 1);
          movedCount++;
        }
      } finally {
        isOrganizing = false;
      }
      if (movedCount > 0) options.emit('libraryChanged', null);
      return {
        movedCount,
        skippedCount: entries.filter(
          (entry) => entry.status !== 'move' && entry.status !== 'unchanged'
        ).length,
        failedCount: 0,
        errors: [],
        cancelled: organizeCancelled
      };
    },
    cancelFileOrganize: () => {
      if (isOrganizing) organizeCancelled = true;
      return null;
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
    // 実装と同じく、前後の空白を除いた名前で作る
    createPlaylist: (name) => {
      const trimmed = name.trim();
      validatePlaylistName(trimmed);
      return newPlaylist(trimmed, null);
    },
    createSmartPlaylist: (name, rules) => {
      const trimmed = name.trim();
      validatePlaylistName(trimmed);
      validateSmartRules(rules);
      return newPlaylist(trimmed, rules);
    },
    // 実装と同じく、同じ曲は最初の1回だけを入れ、見つからない曲があれば作らない
    createPlaylistWithTracks: (name, trackIds) => {
      const trimmed = name.trim();
      validatePlaylistName(trimmed);
      trackIds.forEach(validateTrackId);
      const unique = [...new Set(trackIds)];
      if (!unique.every((id) => tracks.some((track) => track.id === id))) {
        fail('NOT_FOUND', 'トラックが見つかりません');
      }
      return newPlaylist(trimmed, null, unique);
    },
    setPlaylistDescription: (playlistId, description) => {
      validatePlaylistId(playlistId);
      if (description !== null && [...description].length > 1000) {
        fail('VALIDATION', '説明は1000文字以内で入力してください');
      }
      const playlist = findPlaylist(playlistId);
      playlist.description = description?.trim() || null;
      playlist.updatedAt = now();
      return null;
    },
    getPlaylistFolders: () =>
      [...playlistFolders].sort(
        (a, b) => a.position - b.position || compareAsc(a.createdAt, b.createdAt)
      ),
    createPlaylistFolder: (name) => {
      const trimmed = name.trim();
      validatePlaylistFolderName(trimmed);
      const folder: PlaylistFolder = {
        id: crypto.randomUUID(),
        name: trimmed,
        position: nextPosition(playlistFolders),
        createdAt: now()
      };
      playlistFolders.push(folder);
      return folder;
    },
    renamePlaylistFolder: (folderId, name) => {
      const folder = findPlaylistFolder(folderId);
      const trimmed = name.trim();
      validatePlaylistFolderName(trimmed);
      folder.name = trimmed;
      return null;
    },
    // 実装と同じく、中のプレイリストは消さず、フォルダの外へ出す
    deletePlaylistFolder: (folderId) => {
      findPlaylistFolder(folderId);
      for (const playlist of playlists) {
        if (playlist.folderId === folderId) playlist.folderId = null;
      }
      playlistFolders = playlistFolders.filter((folder) => folder.id !== folderId);
      return null;
    },
    movePlaylist: (playlistId, folderId) => {
      validatePlaylistId(playlistId);
      if (folderId !== null && !UUID_PATTERN.test(folderId)) {
        fail('VALIDATION', '不正なフォルダID形式です');
      }
      const playlist = playlists.find((p) => p.id === playlistId);
      if (!playlist || (folderId !== null && !playlistFolders.some((f) => f.id === folderId))) {
        fail('NOT_FOUND', 'プレイリストまたはフォルダが見つかりません');
      }
      playlist.position = nextPosition(playlists);
      playlist.folderId = folderId;
      return null;
    },
    reorderPlaylists: (playlistIds) => {
      playlistIds.forEach(validatePlaylistId);
      playlistIds.forEach((id, position) => {
        const playlist = playlists.find((p) => p.id === id);
        if (playlist) playlist.position = position;
      });
      return null;
    },
    reorderPlaylistFolders: (folderIds) => {
      folderIds.forEach((id, position) => {
        if (!UUID_PATTERN.test(id)) fail('VALIDATION', '不正なフォルダID形式です');
        const folder = playlistFolders.find((f) => f.id === id);
        if (folder) folder.position = position;
      });
      return null;
    },
    updateSmartPlaylist: (playlistId, rules) => {
      validatePlaylistId(playlistId);
      validateSmartRules(rules);
      const playlist = findPlaylist(playlistId);
      if (playlist.rules === null) {
        fail('VALIDATION', '通常のプレイリストには、条件を設定できません');
      }
      playlist.rules = rules;
      playlist.updatedAt = now();
      return null;
    },
    countSmartPlaylistTracks: (rules) => {
      validateSmartRules(rules);
      return countSmartPlaylistTracks(tracks, rules, Date.now());
    },
    reshuffleSmartPlaylists: () => {
      shuffleSeed++;
      return null;
    },
    // 実装と同じく、自動プレイリストの曲は持たせない（開く時に条件から求める）
    getPlaylists: () =>
      [...playlists]
        .sort((a, b) => compareAsc(b.createdAt, a.createdAt))
        .map((playlist) => ({
          ...playlist,
          tracks:
            playlist.rules === null
              ? [...playlist.tracks].sort((a, b) => a.position - b.position)
              : []
        })),
    // 実装と同じく、見つからないプレイリストは空の一覧を返す
    getPlaylistTracks: (playlistId) => {
      validatePlaylistId(playlistId);
      const playlist = playlists.find((p) => p.id === playlistId);
      return playlist ? playlistTracks(playlist) : [];
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
      ensureManualPlaylist(playlistId);
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
    // 実装と同じく、入っていないトラックは飛ばし、外したトラック数を返す
    removeTracksFromPlaylist: (playlistId, trackIds) => {
      validatePlaylistId(playlistId);
      if (trackIds.length === 0) {
        fail('VALIDATION', 'トラックIDが指定されていません');
      }
      trackIds.forEach(validateTrackId);
      ensureManualPlaylist(playlistId);
      const playlist = findPlaylist(playlistId);
      const before = playlist.tracks.length;
      // 削除後はpositionを連番に振り直す
      playlist.tracks = playlist.tracks
        .filter((entry) => !trackIds.includes(entry.trackId))
        .sort((a, b) => a.position - b.position)
        .map((entry, position) => ({ ...entry, position }));
      const removed = before - playlist.tracks.length;
      if (removed > 0) playlist.updatedAt = now();
      return removed;
    },
    reorderPlaylistTracks: (playlistId, trackIds) => {
      validatePlaylistId(playlistId);
      trackIds.forEach(validateTrackId);
      ensureManualPlaylist(playlistId);
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
      const playlist = newPlaylist(name, null, trackIds);
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
      return { fileName: `${playlist.name}.m3u8`, trackCount: playlistTracks(playlist).length };
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
      // 再生していた曲と、その後ろの曲がすべてなくなっていた: 前に残っている最後の曲を指す
      if (currentIndex === null && playbackState.currentIndex !== null && queue.length > 0) {
        currentIndex = queue.length - 1;
      }
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
    playbackSetSpeed: (speed) => {
      if (
        speed === null ||
        !Number.isFinite(speed) ||
        speed < MIN_PLAYBACK_SPEED ||
        speed > MAX_PLAYBACK_SPEED
      ) {
        fail(
          'VALIDATION',
          `再生速度は${MIN_PLAYBACK_SPEED}倍から${MAX_PLAYBACK_SPEED}倍の範囲で指定してください`
        );
      }
      playbackEngine.setSpeed(speed);
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
      console.info(`[mock] Reveal in file manager: ${findTrack(trackId).filePath}`);
      return null;
    },
    getSettings: () => settings,
    // 実装と同じく、保存した設定がオンの間だけ登録されている。モックでは「次の曲」だけ、
    // ほかのアプリが使っていて登録できなかったことにする（設定画面の表示の確認用）
    getGlobalShortcuts: () => {
      const modifiers = ['Control', 'Option'];
      return (
        [
          ['toggle', 'P', true],
          ['next', '.', false],
          ['previous', ',', true]
        ] as const
      ).map(([action, key, available]) => ({
        action,
        keys: [...modifiers, key],
        registered: settings.globalShortcuts && available
      }));
    },
    // ウィンドウの大きさは変えられないため、切り替えたことだけを覚える
    setMiniPlayer: (enabled, alwaysOnTop) => {
      miniPlayerWindow = { enabled, alwaysOnTop: enabled && alwaysOnTop };
      return null;
    },
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
        !Number.isInteger(next.normalizationPreampDb) ||
        Math.abs(next.normalizationPreampDb) > MAX_NORMALIZATION_PREAMP_DB
      ) {
        fail(
          'VALIDATION',
          `プリアンプは-${MAX_NORMALIZATION_PREAMP_DB}〜${MAX_NORMALIZATION_PREAMP_DB} dBで指定してください`
        );
      }
      if (
        !(LIBRARY_SCAN_INTERVALS as readonly number[]).includes(next.libraryScanIntervalMinutes)
      ) {
        fail('VALIDATION', '再スキャンの間隔が選べる値ではありません');
      }
      parseOrganizeTemplate(next.musicFolderTemplate);
      settings = { ...next };
      options.emit('settingsChanged', settings);
      return null;
    },
    openProjectPage: () => {
      console.info('[mock] Ignored opening the project page');
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
      return { playCount: track.playCount, lastPlayedAt: timestamp };
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
    // macOSと同じく、ファイルはゴミ箱へ移すものとして扱う
    canMoveToTrash: () => true,
    getLibraryFolders: () => {
      const folders = [...libraryFolders]
        .sort((a, b) => compareAsc(a.path, b.path))
        .map((folder) => ({
          ...folder,
          trackCount: tracksUnder(folder.path).length,
          exclusions: [...folder.exclusions]
            .sort((a, b) => compareAsc(a.path, b.path))
            .map((exclusion) => ({
              ...exclusion,
              trackCount: tracksInExclusion(folder, exclusion.path).length
            }))
        }));
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
      if (removed > 0) options.emit('libraryChanged', null);
      return removed;
    },
    // 音楽フォルダのライブラリフォルダを削除すると、音楽フォルダは未設定に戻る
    removeLibraryFolder: (folderId, removeTracksToo) => {
      const folder = findLibraryFolder(folderId);
      const removed = removeTracksToo
        ? removeTracks(tracksUnder(folder.path).map((track) => track.id))
        : 0;
      libraryFolders = libraryFolders.filter((f) => f.id !== folderId);
      if (removed > 0 || folder.isMusicFolder) options.emit('libraryChanged', null);
      return removed;
    },
    // 実装はRust側のダイアログで選ぶ。モックは`setRelocationFolder`のフォルダを選んだことにする
    planLibraryFolderRelocation: (folderId) => {
      const folder = findLibraryFolder(folderId);
      pendingRelocation = null;
      if (relocationFolder === null) return null;
      const path = normalizeFolderPath(relocationFolder);
      const blocked = relocationBlocker(folder, path);
      if (blocked) return blocked;
      pendingRelocation = { folderId: folder.id, path };
      const moved = tracksUnder(folder.path);
      // 実ファイルは存在しないため、見つからない曲のほかは、新しい場所で見つかったことにする
      return {
        kind: 'ready',
        path,
        trackCount: moved.length,
        foundCount: moved.filter((track) => !track.isMissing).length
      };
    },
    // 実ファイルは存在しないため、付け替えた後の再スキャンは「変更なし」として扱う
    relocateLibraryFolder: async (folderId) => {
      const folder = findLibraryFolder(folderId);
      const held = pendingRelocation;
      pendingRelocation = null;
      if (!held || held.folderId !== folder.id) {
        fail('NOT_FOUND', '付け替える場所が選ばれていません');
      }
      if (relocationBlocker(folder, held.path)) {
        fail('VALIDATION', '選んだ場所には付け替えられなくなりました。もう一度選んでください');
      }
      const moved = tracksUnder(folder.path);
      for (const track of moved) track.filePath = relocatedPath(track, folder.path, held.path);
      folder.path = held.path;
      folder.exists = true;
      // 実装と同じく、付け替えた後に新しい場所を再スキャンする
      await sendRescanProgress(folder.path);
      folder.lastScannedAt = now();
      options.emit('libraryChanged', null);
      return { path: held.path, relocatedCount: moved.length, rescan: unchangedRescanResult() };
    },
    cancelLibraryFolderRelocation: () => {
      pendingRelocation = null;
      return null;
    },
    // 実装はRust側のダイアログで選び、DBを写す。モックは`setBackupFile`のファイルへ保存したことにする
    exportLibraryBackup: () => {
      if (backupFile === null) return null;
      // 拡張子がなければ付ける（Rustの`with_backup_extension`）
      const path = /\.[^/.]+$/.test(backupFile) ? backupFile : `${backupFile}.muspicebackup`;
      return { path, trackCount: tracks.length, playlistCount: playlists.length };
    },
    planLibraryRestore: () => {
      pendingRestore = null;
      if (backupFile === null) return null;
      const fileName = backupFile.slice(backupFile.lastIndexOf('/') + 1);
      if (!/\.(muspicebackup|db)$/i.test(fileName)) return { kind: 'notBackup', fileName };
      if (fileName.includes('damaged')) return { kind: 'damaged', fileName };
      if (fileName.includes('newer')) return { kind: 'newerFormat', fileName };
      pendingRestore = backupFile;
      return {
        kind: 'ready',
        fileName,
        createdAt: '2026-10-20T09:00:00.000Z',
        trackCount: tracks.length,
        playlistCount: playlists.length,
        libraryFolderCount: libraryFolders.length,
        // DBのファイルそのものは、設定を含まない
        includesSettings: fileName.endsWith('.muspicebackup')
      };
    },
    // 実装はアプリを再起動する。モックは、預かったファイルを手放すだけ（ライブラリは変えない）
    restoreLibraryBackup: () => {
      if (pendingRestore === null) fail('NOT_FOUND', '復元するファイルが選ばれていません');
      pendingRestore = null;
      return null;
    },
    cancelLibraryRestore: () => {
      pendingRestore = null;
      return null;
    },
    getMusicFolder: () => {
      const folder = libraryFolders.find((candidate) => candidate.isMusicFolder);
      return {
        folder: folder ? { id: folder.id, path: folder.path, exists: folder.exists } : null,
        systemFolder: SYSTEM_MUSIC_FOLDER
      };
    },
    // 実装はRust側のダイアログで選ぶ。モックは`setFolderResult`のフォルダを選んだことにする
    setMusicFolder: (source) => setMusicFolder(source),
    clearMusicFolder: () => {
      for (const folder of libraryFolders) folder.isMusicFolder = false;
      options.emit('libraryChanged', null);
      return null;
    },
    previewMusicFolderTemplate: (template) => previewMusicFolderTemplate(template),
    // 実装はRust側のダイアログで選ぶ。モックは`setExclusionFolder`のフォルダを選んだことにする
    addLibraryFolderExclusion: (folderId) => {
      const folder = findLibraryFolder(folderId);
      if (!folder.exists) {
        fail(
          'NOT_FOUND',
          `フォルダが見つかりません: ${folder.path}（外付けドライブなどが接続されているか確認してください）`
        );
      }
      const selected = pickExclusionFolder(folder);
      if (selected === null) return null;
      const relative = relativeToFolder(folder, selected);
      if (folder.exclusions.some((e) => isSameOrWithin(relative, e.path))) {
        fail('VALIDATION', 'すでに読み込まないフォルダです（または、その中のフォルダです）');
      }
      // 除外しているフォルダを含むフォルダは、中の除外をまとめる（Rustの`add_exclusion`）
      folder.exclusions = folder.exclusions.filter((e) => !isSameOrWithin(e.path, relative));
      const exclusion = { id: crypto.randomUUID(), path: relative };
      folder.exclusions.push(exclusion);
      return { ...exclusion, trackCount: tracksInExclusion(folder, relative).length };
    },
    removeExcludedTracks: (exclusionId) => {
      const { folder, exclusion } = findExclusion(exclusionId);
      const removed = removeTracks(
        tracksInExclusion(folder, exclusion.path).map((track) => track.id)
      );
      if (removed > 0) options.emit('libraryChanged', null);
      return removed;
    },
    // 実ファイルは存在しないため、再スキャンは「変更なし」として扱う
    removeLibraryFolderExclusion: (exclusionId) => {
      const { folder, exclusion } = findExclusion(exclusionId);
      folder.exclusions = folder.exclusions.filter((e) => e !== exclusion);
      if (!folder.exists) return null;
      folder.lastScannedAt = now();
      return unchangedRescanResult();
    },
    // 実ファイルは存在しないため、見つかるフォルダは「変更なし」として扱う
    rescanLibraryFolder: async (folderId) => {
      const folder = findLibraryFolder(folderId);
      if (!folder.exists) {
        fail(
          'NOT_FOUND',
          `フォルダが見つかりません: ${folder.path}（外付けドライブなどが接続されているか確認してください）`
        );
      }
      await sendRescanProgress(folder.path);
      folder.lastScannedAt = now();
      return unchangedRescanResult();
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
        newPlaylist(
          name,
          null,
          targets.slice(0, 5).map((track) => track.id)
        );
        playlistCount = 1;
      }
      options.emit('libraryChanged', null);
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
      options.emit('libraryChanged', null);
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
    registerSyncDevice: () => {
      if (folderResult === null) return null;
      const path = validateDeviceFolder(folderResult);
      const device: MockSyncDevice = {
        id: crypto.randomUUID(),
        name: suggestDeviceName(path) ?? (settings.language === 'en' ? 'Device' : 'デバイス'),
        path,
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
    relinkSyncDevice: (deviceId) => {
      const device = findSyncDevice(deviceId);
      if (folderResult === null) return null;
      device.path = validateDeviceFolder(folderResult);
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
      // 値の並びがそのまま`commands.xxx()`の位置引数に対応する。
      // IPCと同じくJSONを通して複製する（画面の状態（Svelteのプロキシ）を、そのまま持ち続けない。
      // 持ったままだと、後で結果として返す時に複製できない）
      const params = JSON.parse(JSON.stringify(Object.values(args))) as unknown[];
      const result = await handler(...params);
      // IPCのシリアライズと同様に複製を返し、呼び出し側から状態を書き換えられないようにする
      return structuredClone(result ?? null);
    },
    albumArtUrl(trackId) {
      const track = tracks.find((t) => t.id === trackId);
      return track ? albumArtOf(track).url : null;
    },
    nowPlaying: () => structuredClone(nowPlaying),
    setFileImportMode: (mode) => {
      fileImportMode = mode;
    },
    openFiles: (fileNames) => {
      const audioFiles = fileNames.filter(isAudioFileName);
      const errors = fileNames
        .filter((fileName) => !isAudioFileName(fileName))
        .map((fileName) => `${fileName}: 対応していない形式です`);
      void (async () => {
        // 実装と同じく、取り込む曲を音楽フォルダへコピーする設定なら、コピーしてから登録する
        let placed = new Map<string, string>();
        if (audioFiles.length > 0) {
          const imported = await runImport(
            importTargetsIn(DROPPED_FILES_FOLDER, audioFiles),
            'Skip',
            copyDestination()
          );
          placed = imported.placed;
          if (imported.outcome.kind === 'imported') {
            errors.push(
              ...imported.outcome.result.notCopied.map(
                (entry) => `${entry.source}: コピー先に同じ名前のファイルがあります`
              )
            );
          } else {
            errors.push('音楽フォルダへコピーできませんでした');
          }
        }
        const opened = audioFiles
          .map((fileName) => `${folderPrefix(DROPPED_FILES_FOLDER)}${fileName}`)
          .map((filePath) => placed.get(filePath) ?? filePath)
          .map((filePath) => tracks.find((track) => track.filePath === filePath))
          .filter((track) => track !== undefined);
        openedFiles = {
          tracks: [...openedFiles.tracks, ...opened],
          errors: [...openedFiles.errors, ...errors]
        };
        options.emit('tracksOpened', null);
      })();
    },
    addDuplicateTracks: () => {
      const find = (title: string) => tracks.find((track) => track.title === title);
      const copy = (source: Track | undefined, filePath: string, overrides: Partial<Track>) => {
        if (!source) return;
        tracks.push({
          ...structuredClone(source),
          id: crypto.randomUUID(),
          filePath,
          fileName: filePath.slice(filePath.lastIndexOf('/') + 1),
          isFavorite: false,
          rating: 0,
          playCount: 0,
          skipCount: 0,
          lastPlayedAt: null,
          createdAt: now(),
          updatedAt: now(),
          ...overrides
        });
      };
      const downloads = '/Users/demo/Music/Downloads';
      const paperPlanes = find('Paper Planes');
      const overclock = find('Overclock');
      const neonRain = find('Neon Rain');
      // 同じ形式の写し（整理すると、移動先に同じ名前のファイルがある）
      copy(paperPlanes, `${downloads}/paper planes (1).mp3`, {
        duration: (paperPlanes?.duration ?? 0) + 1
      });
      // 全角・小文字のタイトルで、形式の違う写し（整理すると、移せる）
      copy(overclock, `${downloads}/overclock.flac`, {
        title: 'ｏｖｅｒｃｌｏｃｋ',
        format: 'FLAC',
        bitrate: 1411,
        duration: (overclock?.duration ?? 0) + 2
      });
      // 同じ曲の写しが2つ（整理すると、同じ移動先になる）
      copy(neonRain, `${downloads}/a/neon.mp3`, { format: 'MP3', bitrate: 320 });
      copy(neonRain, `${downloads}/b/neon.mp3`, { format: 'MP3', bitrate: 128 });
      options.emit('libraryChanged', null);
    },
    setExclusionFolder: (path) => {
      exclusionFolder = path;
    },
    setRelocationFolder: (path) => {
      relocationFolder = path;
    },
    setBackupFile: (path) => {
      backupFile = path;
    },
    setConvertExportFolder: (path) => {
      convertExportFolder = path;
    },
    setFolderResult: (path) => {
      folderResult = path;
    },
    setMusicFolderFileCount: (count) => {
      musicFolderFileCount = count;
    },
    setMusicFolderFreeBytes: (bytes) => {
      musicFolderFreeBytes = bytes;
    },
    setM3uImportMode: (mode) => {
      m3uImportMode = mode;
    },
    miniPlayer: () => miniPlayerWindow,
    setAlbumArtPickMode: (mode) => {
      albumArtPickMode = mode;
    }
  };
}
