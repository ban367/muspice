/**
 * バックエンド（Rust）と共有するデータモデル
 *
 * 型定義はtauri-spectaが`src/lib/bindings.ts`へ自動生成したものを再エクスポートする。
 * 手動での型定義は追加せず、Rust側の型を変更して再生成すること。
 * 生成は`npm run tauri dev`（デバッグビルド起動時）または
 * `cargo test export_typescript_bindings`で実行される。
 */
export type {
  AlbumArtInfo,
  AlbumArtSource,
  AlbumGroup,
  AlbumSummary,
  AppError,
  ArtistSummary,
  BulkUpdateResult,
  DeleteFailure,
  DeleteResult,
  DeviceSyncPlan,
  DeviceSyncProgress,
  DeviceSyncResult,
  DuplicateAction,
  FilterOptions,
  GenreSummary,
  ImportResult,
  Language,
  LibraryFolder,
  LibraryFolderList,
  LibraryXmlImportResult,
  M3uExportResult,
  M3uImportResult,
  Metadata,
  NowPlayingUpdate,
  OutputDevice,
  PlayHistoryEntry,
  PlaybackControl,
  PlaybackEvent,
  Playlist,
  PlaylistTrack,
  RefreshMetadataResult,
  ReplayGain,
  RescanResult,
  Settings,
  SidebarItem,
  SortTags,
  StartupPage,
  SyncDevice,
  SyncDeviceConfig,
  Theme,
  Track,
  TrackMetadataChange,
  VolumeNormalization,
  WriteMetadataResult
} from '#lib/bindings.js';
