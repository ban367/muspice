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
    /// 並び順に使う値（読みなど。タグにない項目は値なし）
    pub sort_tags: SortTags,
    /// ファイルが見つからない（再スキャンで見つからなくなった曲。利用者が外すまで残す）
    pub is_missing: bool,
}

/// 並び順に使う値（ソート用のタグ。`TITLESORT`・`ARTISTSORT`・`ALBUMSORT`・`ALBUMARTISTSORT`）
///
/// 漢字の名前を読みの順に並べるための読み仮名などが入る。値のない項目は、表示用の値
/// （タイトル・アーティストなど）で並べる。
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SortTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
}

impl SortTags {
    /// どの項目にも値がないか
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// M3Uのファイル1つを読み込んだ結果
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct M3uImportResult {
    /// 読み込んだファイルの名前
    pub file_name: String,
    /// 作ったプレイリストのIDと名前（対応する曲が1つもない・読めなかった場合は作らず、null）
    pub playlist_id: Option<String>,
    pub playlist_name: Option<String>,
    /// プレイリストに入れた曲数
    pub added_count: u32,
    /// 同じ曲が2回以上書かれていて、飛ばした数（プレイリストには、同じ曲を1回だけ入れる）
    pub duplicate_count: u32,
    /// ライブラリの曲と対応が付かなかった行の数
    pub unmatched_count: u32,
    /// 対応が付かなかった行（M3Uに書かれていたまま。多い場合は先頭の一部だけ）
    pub unmatched: Vec<String>,
    /// ファイルを読めなかった場合のエラー
    pub error: Option<String>,
}

/// プレイリストをM3U8へ書き出した結果
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct M3uExportResult {
    /// 書き出したファイルの名前
    pub file_name: String,
    /// 書き出した曲数
    pub track_count: u32,
}

/// 表示しているアルバムアートが、どこの画像か
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AlbumArtSource {
    /// 音楽ファイルに埋め込まれた画像
    Embedded,
    /// 音楽ファイルと同じフォルダの画像（`cover.jpg`など）
    Folder,
}

/// トラックのアルバムアートの情報（アルバムアートの画面に出す）
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AlbumArtInfo {
    pub source: AlbumArtSource,
    /// フォルダの画像のファイル名（埋め込みの画像ではNone）
    pub file_name: Option<String>,
    /// MIMEタイプ（image/jpeg, image/png など）
    pub mime_type: String,
    /// 画像データのサイズ（バイト）
    pub size: u32,
    /// 幅・高さ（ピクセル。読めない場合はNone）
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// ライブラリのXML（iTunes形式）を取り込んだ結果
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryXmlImportResult {
    /// 取り込んだファイルの名前
    pub file_name: String,
    /// XMLの中の曲数（ファイルの場所がある曲）
    pub track_count: u32,
    /// ライブラリの曲と対応が付いた曲数
    pub matched_count: u32,
    /// 再生回数などの値が変わった曲数
    pub updated_count: u32,
    /// ライブラリの曲と対応が付かなかった曲数
    pub unmatched_count: u32,
    /// 対応が付かなかった曲の場所（多い場合は先頭の一部だけ）
    pub unmatched: Vec<String>,
    /// 作ったプレイリストの数
    pub playlist_count: u32,
    /// 対応する曲が1つもなく、作らなかったプレイリストの数
    pub skipped_playlist_count: u32,
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
    /// 入っている曲（自動プレイリストでは、いつも空。曲は開く時に条件から求める）
    pub tracks: Vec<PlaylistTrack>,
    /// 自動プレイリストの条件（値なしは、曲を自分で選ぶ通常のプレイリスト）
    pub rules: Option<crate::smart_playlist::SmartRules>,
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

/// 曲のタグ（編集画面で扱う項目）
///
/// 読み出し（`get_track_tags`）では、ファイルのタグの内容を表す（タグにない項目は値なし）。
/// 書き込み（`update_track_metadata`・`update_multiple_tracks_metadata`）では、値のない項目の
/// 扱いがコマンドによって違う（1曲の編集はタグから取り除き、一括編集は変えない）。
///
/// データベースに保存するのは、一覧・検索に使う項目（タイトル・アーティスト・アルバム・
/// アルバムアーティスト・ジャンル・年・トラック番号・ディスク番号）だけで、そのほかは
/// ファイルのタグだけにある。
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, specta::Type)]
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
    /// アルバムのトラックの総数
    #[specta(optional)]
    pub track_total: Option<i32>,
    /// アルバムのディスクの総数
    #[specta(optional)]
    pub disc_total: Option<i32>,
    /// グループ（作品のまとまりなど）
    #[specta(optional)]
    pub grouping: Option<String>,
    #[specta(optional)]
    pub bpm: Option<i32>,
    /// コンピレーション（複数のアーティストの曲を集めたアルバム）の印
    #[specta(optional)]
    pub compilation: Option<bool>,
    #[specta(optional)]
    pub comment: Option<String>,
    /// 歌詞（時刻のないテキスト）
    #[specta(optional)]
    pub lyrics: Option<String>,
    /// 並び順に使う値（読みなど。`SortTags`を参照）
    #[specta(optional)]
    pub title_sort: Option<String>,
    #[specta(optional)]
    pub artist_sort: Option<String>,
    #[specta(optional)]
    pub album_sort: Option<String>,
    #[specta(optional)]
    pub album_artist_sort: Option<String>,
}

impl Metadata {
    /// 並び順に使う値を、トラックに持たせる形にする
    pub fn sort_tags(&self) -> SortTags {
        SortTags {
            title: self.title_sort.clone(),
            artist: self.artist_sort.clone(),
            album: self.album_sort.clone(),
            album_artist: self.album_artist_sort.clone(),
        }
    }
}

/// アルバムの一覧の1件（曲は含めない。曲は`get_album_tracks`で取得する）
///
/// アルバムは「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめる。
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AlbumSummary {
    pub name: String,
    /// 並び順に使う値（アルバムの曲のソート用のタグ。なければ値なしで、`name`で並べる）
    pub sort_name: Option<String>,
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
    /// 並び順に使う値（アーティストの曲のソート用のタグ。なければ値なしで、`name`で並べる）
    pub sort_name: Option<String>,
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
    /// 並び順に使う値（`AlbumSummary::sort_name`と同じ）
    pub sort_name: Option<String>,
    pub artist: Option<String>,
    pub track_count: i32,
    pub total_duration: i32,
    pub representative_track_id: String,
    pub tracks: Vec<Track>,
}
