use serde::{Deserialize, Serialize};

/// 音楽トラックのデータモデル
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub duration: Option<i32>,
    // ファイルサイズは2^53未満の前提でnumberとしてエクスポートする
    #[specta(type = specta_typescript::Number)]
    pub file_size: i64,
    pub format: String,
    pub bitrate: Option<i32>,
    pub sample_rate: Option<i32>,
    pub is_favorite: bool,
    pub rating: i32,
    pub play_count: i32,
    pub last_played_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 再生履歴のデータモデル
/// 将来の詳細な再生履歴機能で使用予定
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayHistory {
    // 履歴IDは2^53未満の前提でnumberとしてエクスポートする
    #[specta(type = specta_typescript::Number)]
    pub id: i64,
    pub track_id: String,
    pub played_at: String,
}

/// プレイリストのデータモデル
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tracks: Vec<PlaylistTrack>,
    pub created_at: String,
    pub updated_at: String,
}

/// プレイリスト内のトラック情報
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistTrack {
    pub track_id: String,
    pub position: i32,
    pub added_at: String,
}

/// メタデータのデータモデル
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[specta(optional)]
    pub title: Option<String>,
    #[specta(optional)]
    pub artist: Option<String>,
    #[specta(optional)]
    pub album: Option<String>,
    #[specta(optional)]
    pub genre: Option<String>,
    #[specta(optional)]
    pub year: Option<i32>,
    #[specta(optional)]
    pub track_number: Option<i32>,
    #[specta(optional)]
    pub disc_number: Option<i32>,
    #[specta(optional)]
    pub album_artist: Option<String>,
    #[specta(optional)]
    pub composer: Option<String>,
}

/// アルバムグループ（アルバム表示用）
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AlbumGroup {
    pub name: String,
    pub artist: Option<String>,
    pub track_count: i32,
    pub total_duration: i32,
    pub representative_track_id: String,
    pub tracks: Vec<Track>,
}

/// アーティストグループ（アーティスト表示用）
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtistGroup {
    pub name: String,
    pub album_count: i32,
    pub track_count: i32,
    pub total_duration: i32,
    pub representative_track_id: String,
    pub albums: Vec<AlbumGroup>,
}

/// ジャンルグループ（ジャンル表示用）
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenreGroup {
    pub name: String,
    pub track_count: i32,
    pub total_duration: i32,
    pub representative_track_id: String,
    pub tracks: Vec<Track>,
}
