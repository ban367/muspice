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
    /// アルバムアーティスト（タグにない曲は値なし。一覧では、ない場合に`artist`でまとめる）
    pub album_artist: Option<String>,
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
    /// 再生回数（曲の半分か4分を聴いた回数）
    pub play_count: i32,
    /// スキップ回数（再生回数に数える前に、別の曲へ移った回数）
    pub skip_count: i32,
    /// 最後に再生回数に数えた日時
    pub last_played_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// 音量の正規化に使うゲイン（タグにない項目は値なし）
    pub replay_gain: ReplayGain,
    /// ファイルが見つからない（再スキャンで見つからなくなった曲。利用者が外すまで残す）
    pub is_missing: bool,
}

/// 再生履歴の1件（再生回数に数えた再生）
///
/// 曲の情報は持たない（フロントエンドが、全曲の一覧のキャッシュからトラックIDで引く）。
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayHistoryEntry {
    /// 履歴のID（同じ曲が何度も出るため、行の識別に使う）
    #[specta(type = specta_typescript::Number)]
    pub id: i64,
    pub track_id: String,
    /// 再生回数に数えた日時（RFC 3339。UTC）
    pub played_at: String,
}

/// 音量の正規化に使うゲインとピーク（ReplayGainのタグ、またはEBU R128のタグから読み取る）
///
/// ゲインはReplayGainの基準（-18 LUFS）に合わせたdB。ピークは最大振幅（1.0がフルスケール）。
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReplayGain {
    pub track_gain: Option<f64>,
    pub track_peak: Option<f64>,
    pub album_gain: Option<f64>,
    pub album_peak: Option<f64>,
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

/// アルバムの一覧の1件（曲は含めない。曲は`get_album_tracks`で取得する）
///
/// アルバムは「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめる。
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AlbumSummary {
    pub name: String,
    /// アルバムをまとめたアーティスト（アルバムアーティスト。なければ曲のアーティスト）
    pub artist: Option<String>,
    pub track_count: i32,
    pub total_duration: i32,
    pub representative_track_id: String,
}

/// アーティストの一覧の1件（アルバムと曲は含めない。`get_artist_albums`で取得する）
///
/// アーティストは、アルバムアーティスト（なければ曲のアーティスト）でまとめる。
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtistSummary {
    pub name: String,
    pub album_count: i32,
    pub track_count: i32,
    pub total_duration: i32,
    pub representative_track_id: String,
}

/// ジャンルの一覧の1件（曲は含めない。曲は`get_genre_tracks`で取得する）
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenreSummary {
    pub name: String,
    pub track_count: i32,
    pub total_duration: i32,
    pub representative_track_id: String,
}

/// アルバムとその曲（アーティストの詳細で、アルバムごとに曲を表示するために使う）
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
