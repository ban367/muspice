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
  duration: number | null;
  fileSize: number;
  format: string;
  bitrate: number | null;
  sampleRate: number | null;
  createdAt: string;
  updatedAt: string;
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
