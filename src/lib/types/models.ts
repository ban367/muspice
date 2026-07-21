/**
 * バックエンド（Rust）と共有するデータモデル
 *
 * 型定義はtauri-spectaが`src/lib/bindings.ts`へ自動生成したものを再エクスポートする。
 * 手動での型定義は追加せず、Rust側の型を変更して再生成すること。
 * 生成は`npm run tauri dev`（デバッグビルド起動時）または
 * `cargo test export_typescript_bindings`で実行される。
 */
export type {
  AlbumArt,
  AlbumGroup,
  AppError,
  ArtistGroup,
  DeleteFailure,
  DeleteResult,
  DuplicateAction,
  FilterOptions,
  GenreGroup,
  ImportResult,
  Metadata,
  Playlist,
  PlaylistTrack,
  RefreshMetadataResult,
  Track
} from '$lib/bindings';
