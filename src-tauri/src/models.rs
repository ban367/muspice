use serde::{Deserialize, Serialize};

/// 音楽トラックのデータモデル
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Track {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub duration: Option<i32>,
    pub file_size: i64,
    pub format: String,
    pub bitrate: Option<i32>,
    pub sample_rate: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

/// プレイリストのデータモデル
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tracks: Vec<PlaylistTrack>,
    pub created_at: String,
    pub updated_at: String,
}

/// プレイリスト内のトラック情報
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlaylistTrack {
    pub track_id: String,
    pub position: i32,
    pub added_at: String,
}

/// メタデータのデータモデル
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Metadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub album_artist: Option<String>,
    pub composer: Option<String>,
}
