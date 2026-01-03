/**
 * 音楽トラックのデータモデル
 */
export interface Track {
  id: string;
  filePath: string;
  fileName: string;
  title: string | null;
  artist: string | null;
  album: string | null;
  genre: string | null;
  year: number | null;
  trackNumber: number | null;
  discNumber: number | null;
  duration: number | null;
  fileSize: number;
  format: string;
  bitrate: number | null;
  sampleRate: number | null;
  isFavorite: boolean;
  rating: number;
  playCount: number;
  lastPlayedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

/**
 * 再生履歴のデータモデル
 */
export interface PlayHistory {
  id: number;
  trackId: string;
  playedAt: string;
}

/**
 * プレイリストのデータモデル
 */
export interface Playlist {
  id: string;
  name: string;
  description: string | null;
  tracks: PlaylistTrack[];
  createdAt: string;
  updatedAt: string;
}

/**
 * プレイリスト内のトラック情報
 */
export interface PlaylistTrack {
  trackId: string;
  position: number;
  addedAt: string;
}

/**
 * メタデータのデータモデル
 */
export interface Metadata {
  title?: string;
  artist?: string;
  album?: string;
  genre?: string;
  year?: number;
  trackNumber?: number;
  albumArtist?: string;
  composer?: string;
}

/**
 * インポート結果
 */
export interface ImportResult {
  importedCount: number;
  skippedCount: number;
  errorCount: number;
  errors: string[];
}

/**
 * 重複ファイルの処理方法
 */
export enum DuplicateAction {
  Skip = 'Skip',
  Replace = 'Replace'
}

/**
 * アルバムアート情報
 */
export interface AlbumArt {
  data: string;
  mimeType: string;
}

/**
 * アルバムグループ（アルバム表示用）
 */
export interface AlbumGroup {
  name: string;
  artist: string | null;
  trackCount: number;
  totalDuration: number;
  representativeTrackId: string;
  tracks: Track[];
}

/**
 * アーティストグループ（アーティスト表示用）
 */
export interface ArtistGroup {
  name: string;
  albumCount: number;
  trackCount: number;
  totalDuration: number;
  representativeTrackId: string;
  albums: AlbumGroup[];
}

/**
 * ジャンルグループ（ジャンル表示用）
 */
export interface GenreGroup {
  name: string;
  trackCount: number;
  totalDuration: number;
  representativeTrackId: string;
  tracks: Track[];
}
