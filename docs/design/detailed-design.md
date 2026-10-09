# 詳細設計: データモデル・API仕様・エラーハンドリング

## データモデル

### 主要型（TypeScript / Rust対応）

```typescript
export interface Track {
  id: string;
  filePath: string;
  fileName: string;
  title: string | null;
  artist: string | null;
  album: string | null;
  albumArtist: string | null; // タグにない曲はnull（一覧では、ない場合にartistでまとめる）
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
  playCount: number; // 曲の半分か4分を聴いた回数
  skipCount: number; // 再生回数に数える前に、別の曲へ移った回数
  lastPlayedAt: string | null; // 最後に再生回数に数えた日時
  createdAt: string;
  updatedAt: string;
  replayGain: ReplayGain;
  isMissing: boolean; // ファイルが見つからない曲（再スキャンで見つからなくなり、利用者が外すまで残す）
}

// 音量の正規化に使うゲイン（タグにない項目はnull）
export interface ReplayGain {
  trackGain: number | null; // dB
  trackPeak: number | null; // 1.0 = フルスケール
  albumGain: number | null;
  albumPeak: number | null;
}

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

export interface Playlist {
  id: string;
  name: string;
  description: string | null;
  tracks: PlaylistTrack[];
  createdAt: string;
  updatedAt: string;
}
```

### インポート/削除結果型

- `ImportResult`: `importedCount`, `skippedCount`, `relinkedCount`（見つからない曲を、移動・改名された先のファイルに結び付けた数）, `errorCount`, `errors[]`
- `DeleteResult`: `successCount`, `failedCount`, `failedTracks[]`
- `DuplicateAction`: `Skip | Replace`

### ライブラリフォルダ型

- `LibraryFolder`: `id`, `path`, `trackCount`, `exists`（フォルダが今も見つかるか）, `addedAt`, `lastScannedAt`
- `LibraryFolderList`: `folders[]`, `unregisteredTrackCount`（どのフォルダにも属さない曲数）, `missingTrackCount`（見つからない曲の数）
- `RescanResult`: `addedCount`, `updatedCount`, `relinkedCount`（移動・改名されたファイルに結び付けた曲数）, `missingCount`（見つからない曲にした曲数）, `restoredCount`（ファイルが同じ場所に戻り、見つかる曲に戻した曲数）, `errorCount`, `errors[]`, `missingSkipped`（音楽ファイルが1件も見つからず、見つからない曲にしなかった）

### 転送先デバイス型

- `SyncDevice`: `id`, `name`, `path`（転送先のフォルダ）, `syncAll`（全曲を同期するか）, `playlistIds`（同期するプレイリスト）, `removeUnselected`（対象から外れた曲をデバイスから削除するか）, `connected`（転送先のフォルダに、このデバイスの管理ファイルがあるか）, `freeBytes` / `totalBytes`（接続されていなければnull）, `createdAt`, `lastSyncedAt`
- `SyncDeviceConfig`: `name`, `syncAll`, `playlistIds`, `removeUnselected`
- `DeviceSyncPlan`: `copyCount` / `copyBytes`, `deleteCount` / `deleteBytes`, `renameCount`, `unchangedCount`, `playlistCount`, `missingSourceCount`（元のファイルが見つからない曲数）, `freeBytes`, `requiredBytes`（コピーする容量から、削除で空く容量を引いたもの）, `hasEnoughSpace`
- `DeviceSyncResult`: `copiedCount`, `deletedCount`, `renamedCount`, `playlistCount`, `errorCount`, `errors[]`, `cancelled`

## データベース仕様（SQLite）

### テーブル

| テーブル                | 用途                           | 主なカラム                                                                                                                                                                                                                                                                               |
| ----------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tracks`                | トラック本体                   | `id`, `file_path`, `title`, `artist`, `album`, `album_artist`, `album_artist_read`, `genre`, `year`, `track_number`, `disc_number`, `duration`, `file_size`, `file_modified_at`, `missing_since`, `is_favorite`, `rating`, `play_count`, `skip_count`, `last_played_at`, `replay_gain_*` |
| `library_folders`       | ライブラリフォルダ             | `id`, `path`（UNIQUE）, `added_at`, `last_scanned_at`                                                                                                                                                                                                                                    |
| `playlists`             | プレイリスト本体               | `id`, `name`, `description`, `created_at`, `updated_at`                                                                                                                                                                                                                                  |
| `playlist_tracks`       | プレイリスト内順序             | `playlist_id`, `track_id`, `position`, `added_at`                                                                                                                                                                                                                                        |
| `play_history`          | 再生履歴                       | `id`, `track_id`, `played_at`                                                                                                                                                                                                                                                            |
| `tracks_fts`            | 全文検索（FTS5・trigram）      | `text`（タイトル・アーティスト・アルバム・ジャンル・アルバムアーティストを、検索用に正規化してつないだ文字列）。`rowid`は`tracks`と同じ                                                                                                                                                  |
| `sync_devices`          | 転送先デバイス                 | `id`, `name`, `path`, `sync_all`, `remove_unselected`, `created_at`, `last_synced_at`                                                                                                                                                                                                    |
| `sync_device_playlists` | デバイスに同期するプレイリスト | `device_id`, `playlist_id`                                                                                                                                                                                                                                                               |

### インデックス/制約

- `tracks(artist|album|genre|title)` にインデックス
- `playlist_tracks(playlist_id)` にインデックス
- `playlist_tracks`, `play_history` は `tracks` / `playlists` への外部キー（`ON DELETE CASCADE`）
- `tracks.file_modified_at` はファイルの更新日時（UNIX時間の秒）。再スキャンで、サイズとあわせて変更の検出に使う。列を追加する前に登録したトラックはNULLで、再スキャンで（ファイルを読み直さずに）記録する。サイズが同じで更新日時がちょうど1時間（±2秒）ずれた場合は、夏時間の切り替えによるずれ（更新日時をローカル時刻で記録するFAT32など）とみなし、読み直さずに日時だけ記録する
- `tracks.replay_gain_track_gain` / `replay_gain_track_peak` / `replay_gain_album_gain` / `replay_gain_album_peak`（REAL）はファイルのタグから読んだReplayGain。`REPLAYGAIN_*`を優先し、ゲインがない場合はOpusの`R128_*_GAIN`（Q7.8形式、基準が-23 LUFSのため+5 dBしてReplayGainの基準にそろえる）を使う。範囲外（ゲインは±60 dB、ピークは0以下）は読まない。列を追加する前に登録したトラックはNULLで、`refresh_library_metadata`か再スキャン（ファイルが変わった場合）で読み込む
- `library_folders` とトラックの対応は `tracks.file_path` の前方一致（フォルダのパス＋区切り文字）で判定する。`file_path >= 接頭辞 AND file_path < 上限`（接頭辞の最後の文字を次の文字にしたもの）の範囲で比較する（二分比較のため大文字・小文字を区別し、`file_path`のUNIQUEのインデックスを使える。`LIKE`はASCIIの大文字・小文字を区別しない）。フォルダ同士は入れ子にしない（中のフォルダはインポートしても登録せず、外のフォルダをインポートすると中のフォルダの記録をまとめる）
- `sync_device_playlists` は `sync_devices` / `playlists` への外部キー（`ON DELETE CASCADE`）。取得するときも `playlists` と結合し、削除済みのプレイリストを含めない
- `sync_devices.id` はデバイス側の管理ファイル（後述）の `deviceId` と同じ値。デバイスにコピーしたファイルの一覧はDBに持たず、管理ファイルに記録する
- `tracks.missing_since` は、再スキャンでファイルが見つからなくなった日時（見つかる間はNULL）。見つからない曲はライブラリから外さず、`Track.isMissing`で区別する（ADR-023）。対応付けの候補として取得するため、NULLでない行だけのインデックス（`idx_tracks_missing`）を持つ
- `tracks.album_artist` はファイルのタグから読んだアルバムアーティスト（空の値はNULL）。アルバム・アーティストの一覧は`COALESCE(album_artist, artist)`（アルバムアーティスト。なければ曲のアーティスト）でまとめる（ADR-022）。アーティストの詳細の絞り込み用に、同じ式のインデックス（`idx_tracks_album_artist`）を持つ
- `tracks.album_artist_read` は、アルバムアーティストをファイルから読んだか（0/1）。インポート・再スキャン・`refresh_library_metadata`でファイルを読んだトラックは1にする。列を追加する前に登録したトラックは0で、起動時にバックグラウンドで読み込む（後述の「既存のトラックのアルバムアーティストの読み込み」）
- `tracks_fts` は `tracks` とINSERT/UPDATE/DELETEトリガーで同期する。UPDATEは、検索の対象の列（`title`・`artist`・`album`・`genre`・`album_artist`）を変えた時だけ同期する（再生回数・評価などの更新では索引を更新しない）
  - トークナイザーはtrigram（3文字の並びを索引にする）。語の途中の一致（区切りのない日本語を含む）を探せる
  - 入れる文字列は、SQLの関数`search_text(...)`で作る（`search_text.rs`が接続に登録する。NFKC → 小文字 → カタカナをひらがなに、の順で正規化し、項目を改行でつなぐ）。トリガーがこの関数を使うため、アプリの外（`sqlite3`コマンドなど）から`tracks`の対象の列を書き換えると、関数がないエラーになる
  - 表の定義の版を`PRAGMA user_version`に記録する（`db.rs`の`FTS_SCHEMA_VERSION`。現在は3）。古い版の表は、起動時に削除して`tracks`から作り直す（1: 旧トリガー（直接DELETE/UPDATE方式）で壊れた可能性のあるインデックスを作り直した。2: 検索の対象にアルバムアーティストを加えた。3: トークナイザーをtrigramにし、検索用に正規化した文字列を入れるようにした）

### 一覧の取得

- 一覧・検索・フィルタ・お気に入りに件数の上限はない。全曲の一覧（`get_all_tracks`）はライブラリの全曲を1回で返し、フロントは見えている行だけを描画する（ADR-021）
- アルバム・アーティスト・ジャンルは、一覧（名前・曲数・合計の長さ・代表の曲）と曲を分けて返す。一覧は曲を含まず、曲はそのアルバムなどの分だけを取得する
- アルバムは「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめる。同じ名前でもアーティストが違うアルバムは別のアルバムになり、アルバムアーティストが同じ曲は、曲ごとのアーティストが違っても1つのアルバムになる（コンピレーション・フィーチャリング）。アーティストの一覧も、アルバムアーティスト（なければ曲のアーティスト）でまとめる（ADR-022）
- アルバムの中の曲の並びは、ディスク番号（ない場合は1） → トラック番号 → タイトルの順（`ALBUM_TRACK_ORDER`）

### 検索

- 検索語を空白（全角を含む）で区切り、すべての語を、タイトル・アーティスト・アルバム・ジャンル・アルバムアーティストのどこかに含む曲を返す（語の途中の一致を含む。1つの語が項目をまたぐ一致は含めない）
- 大文字と小文字・全角と半角・ひらがなとカタカナの違いは同じとみなす（索引に入れる文字列と検索語を、同じ規則で正規化する。表示するタグの内容は変えない）
- すべての語が3文字以上なら、全文検索の索引（`tracks_fts MATCH`。語ごとのフレーズをANDでつなぐ）で探す。3文字未満の語がある場合は、trigramの索引では探せないため、`tracks_fts`の文字列を`LIKE`で走査する（ADR-003）
- 検索語の記号（`%`・`_`・二重引用符・`AND`などの演算子）は、文字として探す
- フロントの一覧の絞り込み（アルバム・アーティスト・ジャンル）とモックの検索は、`#lib/utils/searchText`で同じ規則にする

## Tauriコマンド仕様

フロントエンドからは `invoke()` で呼び出す。引数キーはcamelCase（例: `trackId`）で渡す。

### ライブラリ取得・検索

| コマンド                           | 引数                                      | 戻り値            | 備考                                                                                                                                                       |
| ---------------------------------- | ----------------------------------------- | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_all_tracks`                   | なし                                      | `Track[]`         | 全曲。作成日時降順                                                                                                                                         |
| `search_tracks`                    | `query: string`                           | `Track[]`         | 部分一致の検索（「検索」を参照）。作成日時降順                                                                                                             |
| `filter_tracks`                    | `filters: { artist?, album?, genre? }`    | `Track[]`         | 完全一致フィルタ。作成日時降順                                                                                                                             |
| `get_unique_artists/albums/genres` | なし                                      | `string[]`        | フィルタ候補用                                                                                                                                             |
| `get_albums`                       | なし                                      | `AlbumSummary[]`  | アルバムの一覧（名前順）。`artist`は、アルバムをまとめたアーティスト（アルバムアーティスト。なければ曲のアーティスト）。代表の曲は、アルバムの最初の曲     |
| `get_album_tracks`                 | `album: string`, `artist: string \| null` | `Track[]`         | アルバムの曲（ディスク番号 → トラック番号 → タイトル）。`artist`には`AlbumSummary`の`artist`を渡す                                                         |
| `get_artists`                      | なし                                      | `ArtistSummary[]` | アーティストの一覧（アルバムアーティストでまとめる。名前順）。代表の曲は、名前順で最初のアルバムの最初の曲                                                 |
| `get_artist_albums`                | `artist: string`                          | `AlbumGroup[]`    | アルバムアーティストが一致する曲を、アルバムごとに返す（アルバム名順。曲ごとのアーティストが違う曲も含む）。アルバムのない曲は「不明なアルバム」にまとめる |
| `get_genres`                       | なし                                      | `GenreSummary[]`  | ジャンルの一覧（名前順）。代表の曲は、ジャンルの最初の曲                                                                                                   |
| `get_genre_tracks`                 | `genre: string`                           | `Track[]`         | ジャンルの曲（アルバムをまとめるアーティスト → アルバム → アルバムの中の並び）                                                                             |

一覧（`AlbumSummary`・`ArtistSummary`・`GenreSummary`）は`name`・`trackCount`・`totalDuration`・`representativeTrackId`（アルバムアートの取得に使う）を持ち、曲は含まない（アルバムは`artist`、アーティストは`albumCount`も持つ）。見つからないアルバムなどの曲は、空の一覧を返す。

アルバムは名前だけでは決まらないため、フロントは`name`と`artist`の組（`#lib/utils/albumKey`の`albumKey`）で識別する。

#### 既存のトラックのアルバムアーティストの読み込み（`album_artist_backfill.rs`）

アルバムアーティストの列を追加する前に登録したトラック（`album_artist_read = 0`）は、アルバムアーティストが空で、コンピレーションが曲のアーティストごとの別のアルバムに分かれて見える。起動の5秒後に、別スレッドでファイルのタグから読み込む。

- 読み込むのはアルバムアーティストだけで、ほかの項目と`updated_at`は変えない（`refresh_library_metadata`と違い、以前のバージョンでDBだけに保存した編集内容を失わない）
- 200曲ずつ、ファイルの読み取りはDBロックの外で行い、1つのトランザクションで記録する。読み込んだトラックは`album_artist_read`を1にするため、対象がなくなれば次回以降の起動では何もしない
- ファイルを読めなかったトラック（外付けドライブが外れているなど）は未読のまま残し、次回の起動で読み直す。途中でアプリを終了した場合も、次回の起動で続きから読み込む
- アルバムアーティストのあるトラックが1曲でもあれば、終わった時に`LibraryChanged`を送る（一覧のまとめ方が変わるため）

### インポート・削除

| コマンド                           | 引数                            | 戻り値                  | 備考                                                                                                                                                                                                                                                                                                                                                             |
| ---------------------------------- | ------------------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `import_folder`                    | `folderPath`, `duplicateAction` | `ImportResult`          | 50件/トランザクション、`ImportProgress`イベント送信。音楽ファイルが見つかったフォルダをライブラリフォルダとして記録し、`LibraryChanged`を送る。新しいファイルが見つからない曲の移動・改名の先と判定できた場合は、新しい曲として登録せず、その曲に結び付ける（`relinkedCount`）。登録済みのファイルが見つかった場合は、見つからない曲にしていても見つかる曲に戻す |
| `delete_tracks_command`            | `trackIds: string[]`            | `number`                | DBからのみ削除                                                                                                                                                                                                                                                                                                                                                   |
| `delete_tracks_with_files_command` | `trackIds: string[]`            | `DeleteResult`          | DB+ファイル削除                                                                                                                                                                                                                                                                                                                                                  |
| `refresh_library_metadata`         | なし                            | `RefreshMetadataResult` | 全トラックのファイルを読み直し、タグの内容（タイトルなど・評価・track/disc番号・ReplayGain）を反映する。お気に入り・再生回数は変えない                                                                                                                                                                                                                           |

### ライブラリフォルダ

| コマンド                | 引数                       | 戻り値              | 備考                                                                                                                                                                                                                                                                                                                                                           |
| ----------------------- | -------------------------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_library_folders`   | なし                       | `LibraryFolderList` | パス順                                                                                                                                                                                                                                                                                                                                                         |
| `rescan_library_folder` | `folderId`                 | `RescanResult`      | 追加・変更（サイズか更新日時が違う）のファイルを読み込み、ファイルが見つからない曲を「見つからない曲」にする（外さない）。新しいファイルが見つからない曲の移動・改名の先と判定できた場合は、その曲に結び付ける。フォルダが見つからない場合は`NOT_FOUND`。`LibraryScanProgress`を送る。途中で失敗した場合も、それまでに書き込んだ分は`LibraryChanged`で通知する |
| `remove_library_folder` | `folderId`, `removeTracks` | `number`            | 記録を削除する。`removeTracks`ならフォルダ内の曲もライブラリから外す（ファイルは消さない）。外した曲数を返す                                                                                                                                                                                                                                                   |
| `remove_missing_tracks` | なし                       | `number`            | 見つからない曲を、すべてライブラリから外す（ファイルには触れない）。外した曲数を返す                                                                                                                                                                                                                                                                           |

再スキャン・削除でライブラリの曲が変わると`LibraryChanged`イベントを送る。

#### 見つからない曲と、移動・改名されたファイルの対応付け（`track_relink.rs`）

再スキャンでファイルが見つからなくなった曲は、ライブラリから外さず「見つからない曲」（`missing_since`）として残す。お気に入り・再生回数・再生履歴・プレイリストへの登録は、そのまま残る（ADR-023）。

- 再スキャンの順序: （1）見つからなくなった曲を見つからない曲にし、ファイルが同じ場所に戻った曲を見つかる曲に戻す → （2）ライブラリにないファイルを読み込み、見つからない曲（ほかのライブラリフォルダの分・以前の再スキャンの分を含む）のどれかと同じ曲なら、新しい曲として登録せず、その曲のパスを付け替える（IDは変えない） → （3）スキャン日時を記録する
- 同じ曲とみなす条件: ファイルサイズが同じで、さらに「更新日時が同じ」か「タグの内容（タイトル・アーティスト・アルバム・トラック番号・ディスク番号）と長さが同じ」。候補が複数ある場合はファイル名が同じものに絞り、それでも1つに決まらなければ対応付けない（新しい曲として登録し、見つからない曲は残す）
- 結び付けた曲は、パス・ファイル名・ファイルの更新日時だけを変える（同じファイルのため、タグの内容は読み直さない）
- フォルダ自体が見つからない場合（`NOT_FOUND`）と、音楽ファイルが1件も見つからない場合（`missingSkipped`）は、見つからない曲にもしない（外付けドライブが外れている場合などに、全曲を見つからない曲にしないため）
- 見つからない曲は一覧に残り（薄く表示する）、アルバム・アーティスト・ジャンルの曲数にも含める。再生キューには入れない（選んで再生しようとした場合は通知する）
- 見つからない曲を外すのは利用者の操作にする: 曲ごとの「ライブラリから削除」か、設定の「ライブラリ」の「見つからない曲を外す」（`remove_missing_tracks`）

#### 変更の自動反映（`library_sync.rs`）

設定（`watchLibraryFolders`・`libraryScanIntervalMinutes`）に応じて、`rescan_library_folder`と同じ処理でライブラリフォルダを自動で再スキャンする（`LibraryScanProgress`は送らない）。

- 起動時: どちらかが有効なら、起動の5秒後に全フォルダを再スキャンする
- 定期: 設定した間隔ごとに全フォルダを再スキャンする（あわせて監視するフォルダを更新する）
- 監視: ファイルの変更の通知を3秒まとめ、変更のあったパスを含むフォルダだけを再スキャンする。対象は音楽ファイル・今あるフォルダ・なくなったパスのうちライブラリの曲を含むもの（なくなった一時ファイル・画像などでは再スキャンしない）。アプリのデータ（DB・ログなど）のフォルダの中の変更は無視する
- 見つからないフォルダ（外付けドライブが外れているなど）は飛ばす。監視は、監視を始めた時点で見つかるフォルダだけが対象（フォルダの追加・削除・定期の再スキャンのときに更新する。監視を作れなかった場合も、そのときに作り直す）
- `save_settings`は、自動反映への反映（監視の開始）を待たずに返る。反映は、その時点で保存済みの設定を読む
- インポート・再スキャン・ライブラリフォルダの削除は`AppState::lock_library_scan`で1つずつ行う（手動と自動が同じファイルを同時に登録しない。削除したフォルダの曲を再スキャンが書き戻さない）

### 転送先デバイス

| コマンド               | 引数                     | 戻り値             | 備考                                                                                                                                                                                                                            |
| ---------------------- | ------------------------ | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_sync_devices`     | なし                     | `SyncDevice[]`     | 名前順。接続の確認と容量の取得のため、ファイルシステムにアクセスする                                                                                                                                                            |
| `register_sync_device` | `folderPath`, `name`     | `SyncDevice`       | フォルダに管理ファイルがなければ新しく作る。あれば、そのデバイスとして扱う（登録済みなら転送先を更新、未登録なら管理ファイルを引き継いで登録）。ライブラリフォルダと重なるフォルダは`VALIDATION`、フォルダがなければ`NOT_FOUND` |
| `update_sync_device`   | `deviceId`, `config`     | `SyncDevice`       | 名前と同期する対象を更新する。見つからないプレイリストは無視する                                                                                                                                                                |
| `relink_sync_device`   | `deviceId`, `folderPath` | `SyncDevice`       | 転送先のフォルダを変える。別のデバイスの管理ファイルがあるフォルダは`VALIDATION`。管理ファイルがなければ、このデバイスの管理ファイルを作る                                                                                      |
| `remove_sync_device`   | `deviceId`               | `void`             | 登録を解除する。デバイス上の曲と管理ファイルは消さない                                                                                                                                                                          |
| `plan_device_sync`     | `deviceId`               | `DeviceSyncPlan`   | 差分を調べる（デバイスには書き込まない）。接続されていなければ`NOT_FOUND`                                                                                                                                                       |
| `run_device_sync`      | `deviceId`               | `DeviceSyncResult` | 同期する。`DeviceSyncProgress`を送る。他の同期が実行中なら`LOCK`、対象を選んでいない・空き容量が足りない場合は`VALIDATION`。ファイル単位の失敗は`errors`に入れて続け、デバイスが外れた場合は`IO`で中止する                      |
| `cancel_device_sync`   | なし                     | `void`             | 実行中の同期を中止する（コピー中のファイルの途中でも止める）                                                                                                                                                                    |

フォルダのパスを受け取るのは`register_sync_device`と`relink_sync_device`だけで、それ以外はデバイスのIDからDBで解決する。

#### 同期する曲とプレイリスト

- 曲: `syncAll`ならライブラリの全曲（件数の上限なし。`repository::find_transfer_tracks`）、そうでなければ選んだプレイリストの曲（複数のプレイリストにある曲は1回だけコピーする）
- プレイリスト: 選んだプレイリストを、デバイスのフォルダの直下に`プレイリスト名.m3u8`（拡張M3U・UTF-8・改行はCRLF）で書き出す。曲のパスはデバイスのフォルダからの相対パス（区切りは`/`。直下に置くため`..`を含まない）で、デバイスにある曲だけを曲順どおりに書く

#### デバイス上の配置（`device_sync.rs`）

- `アーティスト/アルバム/01 タイトル.拡張子`。ディスク番号があれば`1-01 タイトル`。タグがない場合は`Unknown Artist` / `Unknown Album`（表示の言語によらず固定）、タイトルは元のファイル名
- フォルダ名・ファイル名は、NFCにそろえる、FAT32・exFAT・Windowsで使えない文字（`<>:"/\|?*`と制御文字）を`_`にする、先頭のドットと前後の空白・末尾のドットを除く、80文字かつ200バイトで切り詰める、Windowsで使えない名前（`CON`など）には`_`を付ける
- パスは大文字・小文字を区別せずに比べる。同じパスになる曲には連番（` (2)`）を付ける
- 一度決めたパスは管理ファイルに記録し、タグが変わらない限り変えない（連番の付いたパスも、そのまま使い続ける）

#### 差分の計算（`device_sync::plan_sync`）

| 状態                                                           | 処理                                                               |
| -------------------------------------------------------------- | ------------------------------------------------------------------ |
| 記録がない曲                                                   | コピー                                                             |
| 記録がなく、同じ場所に同じサイズのファイルがある               | コピー済みとして記録する（対象から外れた曲の記録なら、付け替える） |
| 記録があり、元のファイルが変わった（サイズか更新日時）         | 同じ場所にコピーし直す                                             |
| 記録があり、デバイス上のファイルがない・サイズが違う           | 同じ場所にコピーし直す                                             |
| 記録があり、タグが変わって配置が変わった（元のファイルは同じ） | デバイス上で名前を変える                                           |
| 記録があり、配置も元のファイルも変わった                       | 古いファイルを削除し、新しい場所にコピー                           |
| 記録があり、元のファイルが見つからない                         | そのまま残す（`missingSourceCount`に数える）                       |
| 記録があるが、対象から外れた（トラックが削除された場合を含む） | `removeUnselected`なら削除、そうでなければ残す                     |

- 元のファイルが変わったかは、コピーした時点のサイズ・更新日時（管理ファイルの記録）と、今のファイルを比べて判定する。デバイス側の更新日時は見ない。サイズが同じで更新日時がちょうど1時間（±2秒）ずれた場合は、夏時間の切り替えによるずれとみなす
- タグ・評価の編集はファイルへ書き込まれるため（ADR-019）、編集した曲は「元のファイルが変わった」としてコピーし直す。デバイス上で名前を変えるのは、元のファイルが変わらずに配置だけが変わった場合に限られる
- 削除・上書きするのは、管理ファイルに記録のあるファイルだけ。記録のないファイルがある場所は避ける（連番を付ける）
- プレイリストのファイルは毎回書き直す。名前を変えたプレイリストは古いファイルを消す。対象から外れたプレイリストのファイルは、`removeUnselected`なら削除する

#### 同期の実行（`device_transfer::execute_plan`）

- 順序は「削除 → リネーム → コピー → プレイリスト」。空になったフォルダは削除する
- コピーは別名（`.part`）に書いてから置き換える。アルバムの曲順にコピーする
- 管理ファイルは、削除・リネームの後、50曲コピーするごと、最後に保存する（別名で書いてから置き換える）
- 中止された場合と、デバイスの容量が足りなくなった場合は、プレイリストを書き出さずに終わる（コピー済みの曲は記録する）。最終同期の日時は、中止されなかった場合だけ記録する
- 空き容量は、コピーする容量から削除で空く容量を引いたものに1MiBを足して判定する

#### デバイス側の管理ファイル（`device_manifest.rs`）

転送先のフォルダの`.muspice/manifest.json`。

```json
{
  "version": 1,
  "deviceId": "（UUID）",
  "files": [
    {
      "path": "Artist/Album/01 Title.flac",
      "trackId": "（UUID）",
      "size": 12345678,
      "modifiedAt": 1700000000
    }
  ],
  "playlists": [{ "path": "通勤.m3u8", "playlistId": "（UUID）" }]
}
```

- `path`はデバイスのフォルダからの相対パス（区切りは`/`）。`size` / `modifiedAt`はコピーした時点の元のファイルのもの
- 読むときに、フォルダの外を指すパス（空の要素・`.`・`..`・絶対パス・`\`や`:`を含むもの・`.muspice`の中）の記録は無視する。壊れている場合は`IO`、`version`が新しい場合は`VALIDATION`でエラーにする（空として扱わない）

### 設定

| コマンド        | 引数                 | 戻り値     | 備考                                                                                                                                                                                                                                    |
| --------------- | -------------------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_settings`  | なし                 | `Settings` | `settings.json`がない・壊れている場合は既定値                                                                                                                                                                                           |
| `save_settings` | `settings: Settings` | `void`     | アクセントカラーは`#rrggbb`、クロスフェードは0〜12秒、再スキャンの間隔は選択肢の値。保存後に`SettingsChanged`イベントを送り、ライブラリフォルダの自動反映と、再生エンジン（出力デバイス・音量の正規化・クロスフェード）に設定を反映する |

`Settings`: `{ language: 'ja' \| 'en', startupPage: 'lastOpened' \| 'songs', theme: 'dark' \| 'light' \| 'system', accentColor: string, volumeNormalization: 'off' \| 'track' \| 'album', gaplessPlayback: boolean, crossfadeSeconds: number, outputDeviceId: string \| null, watchLibraryFolders: boolean, libraryScanIntervalMinutes: number }`。既定値は`ja`・`lastOpened`・`dark`・`#3b82f6`・`off`・`true`・`0`・`null`・`false`・`0`。`crossfadeSeconds`は0〜12（整数）、`libraryScanIntervalMinutes`は0（しない）・15・30・60・360のいずれか、`outputDeviceId`は`get_output_devices`が返すID（`null`はOSの既定のデバイス。空文字・512バイトを超える値は不可）で、それ以外は`VALIDATION_ERROR`。`outputDeviceId`のデバイスが接続されていない間は、既定のデバイスで再生する（設定は変えない）。以前のバージョンが保存した、今はない項目（`playbackEngine`）は無視する。ファイルにない項目は既定値で補う（項目を追加しても古いファイルを読める）。

### カスタムプロトコル

| URL                                                                                | 応答                                                                                       | 備考                                                                                        |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| `albumart://localhost/<trackId>`（Windowsは`http://albumart.localhost/<trackId>`） | 画像（`Content-Type`は埋め込み画像のMIMEタイプ） / アートなし・未登録は404 / 不正なIDは400 | `<img>`から読み込む。フロントは`albumArtUrl(trackId)`でURLを作る。`Cache-Control: no-store` |

### イベント（バックエンド→フロントエンド）

`src-tauri/src/events.rs` で定義し、`bindings.ts` の `events` から型付きで購読する。イベント名は型名のケバブケース。

| イベント              | ペイロード                                                         | 送信元                                                                             |
| --------------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------------------- |
| `ImportProgress`      | `{ current, total, currentFile }`                                  | `import_folder`（1ファイルごと）                                                   |
| `LibraryScanProgress` | `{ current, total, currentFile }`                                  | `rescan_library_folder`（読み込むファイルごと）                                    |
| `LibraryChanged`      | なし                                                               | インポートの後、再スキャン（自動を含む）・ライブラリフォルダの削除で曲が変わった時 |
| `DeviceSyncProgress`  | `{ deviceId, current, total, bytesDone, bytesTotal, currentFile }` | `run_device_sync`（コピーするファイルごと。大きなファイルでは途中でも送る）        |
| `ShowAboutDialog`     | なし                                                               | メニュー「Muspice について」                                                       |
| `OpenImportDialog`    | なし                                                               | メニュー「フォルダをインポート...」                                                |
| `ToggleSidebar`       | なし                                                               | メニュー「サイドバーを表示/隠す」                                                  |
| `SettingsChanged`     | `Settings`                                                         | `save_settings`                                                                    |
| `PlaybackEvent`       | 下の「再生エンジン」を参照                                         | 再生エンジン（再生位置・曲の切り替わり・終了・失敗）                               |
| `PlaybackControl`     | 下の「メディアキー・Now Playingと、再生メニュー」を参照            | メニューバーの「再生」メニュー、OSのメディアキー・コントロールセンターなど         |

### メタデータ編集

メタデータ（タイトル・アーティスト・アルバム・ジャンル・年）と評価は、音楽ファイルのタグを正とする（ADR-019）。編集は常にファイルへ書き込み、DBには同じ値を記録する（一覧・検索のため）。ファイルへ書き込めない場合はエラーにし、DBも変えない。書き込み後のファイルのサイズ・更新日時も記録し、再スキャンで自分の書き込みを変更とみなさない。

| コマンド                          | 引数                   | 戻り値                | 備考                                                                                                                                                 |
| --------------------------------- | ---------------------- | --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `update_track_metadata`           | `trackId`, `metadata`  | `void`                | ファイルのタグとDBを更新する。タイトル・アーティスト・アルバム・ジャンル・年は、値がなければタグからも取り除く                                       |
| `update_multiple_tracks_metadata` | `trackIds`, `metadata` | `BulkUpdateResult`    | 値がある項目だけを、各トラックのファイルのタグとDBに反映する。書き込めなかったトラックはDBも変えず、理由を結果に入れて残りを続ける                   |
| `write_library_metadata_to_files` | なし                   | `WriteMetadataResult` | DBだけにある編集内容・評価をファイルへ書き出し、ファイルを読み直してDBに反映する（以前のバージョンのデータの移行用）。終わると`LibraryChanged`を送る |

- `BulkUpdateResult`: `updatedCount`, `failedCount`, `errors[]`
- `WriteMetadataResult`: `writtenCount`, `unchangedCount`（ファイルと同じ）, `skippedCount`（ファイルが見つからない）, `errorCount`, `errors[]`
- `write_library_metadata_to_files`は、DBの値がファイルと違う項目だけを書き込む。DBに値がない項目（評価なしを含む）と、タグにタイトルがないファイルの既定のタイトル（ファイル名）は書き出さない（ファイルにだけある値を消さないため）

### プレイリスト

| コマンド                     | 引数                     | 戻り値       |
| ---------------------------- | ------------------------ | ------------ |
| `create_playlist`            | `name`                   | `Playlist`   |
| `get_playlists`              | なし                     | `Playlist[]` |
| `get_playlist_tracks`        | `playlistId`             | `Track[]`    |
| `rename_playlist`            | `playlistId`, `name`     | `void`       |
| `delete_playlist`            | `playlistId`             | `void`       |
| `add_tracks_to_playlist`     | `playlistId`, `trackIds` | `number`     |
| `remove_track_from_playlist` | `playlistId`, `trackId`  | `void`       |
| `reorder_playlist_tracks`    | `playlistId`, `trackIds` | `void`       |

`add_tracks_to_playlist`は、複数のトラックを渡した順に1つのトランザクションで追加する。すでに入っているトラックは飛ばし、追加したトラック数を返す。見つからないトラックがある場合は`NOT_FOUND`で、1曲も追加しない。

### 再生・統計・システム

| コマンド                 | 引数                                                     | 戻り値                  |
| ------------------------ | -------------------------------------------------------- | ----------------------- |
| `set_current_track`      | `trackId: string \| null`                                | `void`                  |
| `get_current_track`      | なし                                                     | `Track \| null`         |
| `save_playback_state`    | `cursor: PlaybackCursor`, `queue: PlaybackQueue \| null` | `void`                  |
| `get_playback_state`     | なし                                                     | `RestoredPlaybackState` |
| `toggle_favorite`        | `trackId`                                                | `boolean`               |
| `set_rating`             | `trackId`, `rating`                                      | `void`                  |
| `increment_play_count`   | `trackId`                                                | `number`                |
| `increment_skip_count`   | `trackId`                                                | `number`                |
| `get_favorite_tracks`    | なし                                                     | `Track[]`               |
| `get_most_played_tracks` | `limit?`                                                 | `Track[]`               |
| `get_play_history`       | なし                                                     | `PlayHistoryEntry[]`    |
| `show_in_folder`         | `trackId`                                                | `void`                  |
| `open_project_page`      | なし                                                     | `void`                  |

`set_rating`は、評価をファイルのタグへ書き込み、同じ値をDBに記録する（0は評価のタグを取り除く）。評価はloftyの`ItemKey::Popularimeter`（ID3v2 `POPM` / Vorbis `RATING` / MP4 `rate` / RIFF `IRTD`）で読み書きし、インポート・再スキャン・`refresh_library_metadata`でタグから読み込む。評価の数値の付け方は書き込んだアプリごとに違うため、すでに評価があるファイルではその書き手の付け方のまま星の数だけを変え、ない場合はMusicBeeの付け方（ID3v2は1・64・128・196・255、それ以外は20刻み）で書く。Vorbisコメント（FLAC）は、書き手を付けない`RATING`に数値だけを書く（`RATING=80`。すでに星の数の1〜5で書かれていればその付け方を保つ）。loftyの汎用タグは、Vorbisコメントの数値だけの`RATING`を評価として読まず、書き出す時も変換しないため、`metadata.rs`で読み書きする。

`toggle_favorite`・`increment_play_count`（お気に入り・再生回数・再生履歴）とプレイリストは、タグでは持てないためDBだけに保存する。

再生回数・スキップ回数・再生履歴（ADR-030）:

- `increment_play_count`は、再生回数を1増やし、最後に再生した日時を更新して、再生履歴（`play_history`）に1件加える（新しい再生回数を返す）。`increment_skip_count`は、スキップ回数を1増やす（新しいスキップ回数を返す。更新日時・再生履歴は変えない）
- どちらも、いつ呼ぶかはフロントの再生コントローラーが決める（`playTracker.ts`）
  - 再生回数: 実際に鳴らした時間の合計が、曲の長さの半分か4分（短いほう）に届いた時。シークで飛ばした分は含めない。曲の長さが分からなければ、4分か曲の終わり。同じ曲を頭から再生し直した場合（1曲リピートの繰り返し・「前へ」での頭出し）は、聴いた時間を数え直す
  - スキップ回数: 再生回数に数える前に、別の曲を再生し始めた時。鳴らした時間が2秒に満たない曲・最後まで再生された曲・停止・再生の失敗では数えない
- `get_play_history`は、再生履歴を新しい順に返す（件数の上限はない。同じ曲が何度も出る）。`PlayHistoryEntry`: `{ id: number, trackId: string, playedAt: string }`（`playedAt`はRFC 3339のUTC）。曲の情報は含めず、フロントが全曲の一覧（`get_all_tracks`のキャッシュ）からトラックIDで引く。ライブラリから外した曲の履歴は、外部キーで消える
- `get_most_played_tracks`は、再生回数の多い順（同じ回数なら、最近再生した曲が先）に返す（`limit`の既定は50）

再生状態の保存と復元（ADR-028）:

- `save_playback_state`は、再生状態をアプリデータ配下の`playback-state.json`に保存する。`PlaybackCursor`: `{ volume: number, shuffle: boolean, repeat: 'off' \| 'all' \| 'one', currentIndex: number \| null }`（音量は0〜1に収める。`currentIndex`は、キューの中の再生していた曲の位置。キューの範囲を外れていれば`null`として保存する）。`PlaybackQueue`: `{ trackIds: string[], originalTrackIds: string[] \| null }`（再生する順と、シャッフル中だけ、シャッフルする前の順）。`queue`が`null`なら、保存してあるキューを変えない。トラックIDの形式でない値・50万曲を超えるキューは`VALIDATION`
- `get_playback_state`は、保存してある再生状態を今のライブラリに合わせて返す。`RestoredPlaybackState`: `{ volume, shuffle, repeat, queue: Track[], originalTrackIds: string[] \| null, currentIndex: number \| null }`。ライブラリからなくなった曲・ファイルが見つからない曲は`queue`・`originalTrackIds`から除き、`currentIndex`は除いた後の位置にする（再生していた曲がなくなっていれば、その次に残っている曲。なければ`null`）。ファイルがない・壊れている場合は、何も再生していない状態（音量1・キューは空）を返す
- 再生位置は保存しない。フロントは、起動時に復元して（再生は始めない）、再生ボタンで復元した曲を頭から再生する

### 再生エンジン

フロントの再生コントローラー（`playback.svelte.ts`）が使う（ADR-025）。再生キューはフロントが持ち、エンジンへは「再生する曲」と「続けて再生する曲」だけを伝える。

| コマンド                 | 引数                               | 戻り値              | 備考                                                                                                                                                                   |
| ------------------------ | ---------------------------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `playback_play`          | `trackId`, `token`                 | `PlaybackTrackInfo` | トラックを頭から再生する（再生中の曲は止める）。ファイルを開けない・デコードできない・出力を開けない場合はエラー（再生は止まる）。`{ duration: number \| null }`を返す |
| `playback_set_next`      | `trackId: string \| null`, `token` | `void`              | 再生中の曲に続けて再生するトラックを用意する（`null`で取り消す）。用意に失敗した場合はエラーを返すだけで、再生は続く                                                   |
| `playback_pause`         | なし                               | `void`              | 一時停止（少し後に出力を止める）                                                                                                                                       |
| `playback_resume`        | なし                               | `void`              | 再開                                                                                                                                                                   |
| `playback_seek`          | `position`（秒）                   | `void`              | 鳴っている曲の中で移動する。曲の長さを超える位置は曲の終わりになる。負の値・数値でない値は`VALIDATION`                                                                 |
| `playback_set_volume`    | `volume`（0.0〜1.0）               | `void`              | 範囲外の値は丸める                                                                                                                                                     |
| `playback_stop`          | なし                               | `void`              | 再生を止め、再生中の曲・続けて再生する曲を手放す                                                                                                                       |
| `playback_set_equalizer` | `enabled`, `gains: number[]`       | `void`              | イコライザの設定を変える（すぐに効く）。`gains`はバンドごとのゲイン（dB。31Hz〜16kHzの10個。-12〜12に収める）。10個でなければ`VALIDATION`                              |
| `get_output_devices`     | なし                               | `OutputDevice[]`    | `{ id, name, isDefault }`。`id`は接続し直しても変わらない                                                                                                              |

- `token`は、フロントが再生する曲ごとに振る、増えていく番号。エンジンは、その曲についての通知に同じ番号を付ける。より大きい番号の`playback_play`を受け取った後に届いた、小さい番号の`playback_play`はエラーにし（コマンドは別々のスレッドから届くため、続けて出した要求が逆の順で届くことがある）、`playback_set_next`は無視する
- `PlaybackEvent`（`type`で見分ける）
  - `{ type: 'position', token, position }`: 再生位置（秒）。再生中は0.25秒ごと、シークの直後・一時停止した時にも届く
  - `{ type: 'advanced', token, duration }`: 続けて再生する曲へ、切れ目なく切り替わった（`token`は`playback_set_next`で渡した番号）
  - `{ type: 'ended', token }`: 曲が最後まで鳴り終わり、続けて再生する曲がなかった
  - `{ type: 'failed', token, error }`: 再生を続けられなくなった（ファイルを読めない・出力デバイスを使えない）。再生は止まっている
- 再生位置は、出力のコールバックが取り出したフレーム数から求める（鳴っている位置。リングバッファにたまっている約0.5秒分だけ、デコードしている位置より手前になる）
- 曲の頭と終わりの余分（エンコーダーの遅延・パディング）は取り除く。MP3（LAMEのタグ）・Ogg Vorbis・Opus・M4AのAAC（`iTunSMPB`か編集リストがある場合）が対象で、ADTSのAAC・MP2はファイルに情報がないため取り除けない
- 再生回数・スキップ回数の記録（`increment_play_count`・`increment_skip_count`）と再生中のトラックの通知（`set_current_track`）は、フロントが行う（数える条件は、上の「再生回数・スキップ回数・再生履歴」）
- 音量の正規化: `playback_play` / `playback_set_next`の時に、Rust側がトラックのReplayGain（DB）と設定の`volumeNormalization`から倍率を決める（計算は`playback/normalization.rs`。ADR-012）。設定を変えると、再生中の曲・続けて再生する曲の倍率も決め直す
- クロスフェード: 設定の`crossfadeSeconds`が1以上で、続けて再生する曲（`playback_set_next`）があれば、エンジンが前の曲の終わりと重ねる。重ねる長さは、設定の秒数を上限に、どちらの曲も長さの半分まで、かつ前の曲の残りまで。同じ曲の繰り返し（同じファイル）と、長さの分からない曲からは、重ねずに切れ目なく続ける。`advanced`は、重なりが鳴り始めた時点で届く
- イコライザとリミッターは、出力の直前にかける（ADR-026）。イコライザの設定は、エンジンを使う再生コントローラーが、起動時と変更のたびに`playback_set_equalizer`で送る。リミッターは常に有効で、設定はない

### メディアキー・Now Playingと、再生メニュー

OSのメディアキー・コントロールセンター・イヤホンのボタンと、メニューバーの「再生」メニューからの操作（ADR-029）。OSとの連携は、macOSだけに対応する。

| コマンド          | 引数                               | 戻り値 | 備考                                                                                                             |
| ----------------- | ---------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------- |
| `set_now_playing` | `update: NowPlayingUpdate \| null` | `void` | プレーヤーバーの状態をOSへ伝える（`null`は、再生している曲がない）。対応していないOSでは、検証だけして何もしない |

- `NowPlayingUpdate`: `{ trackId: string, playing: boolean, position: number, duration: number \| null }`（`position`・`duration`は秒）
  - 曲の情報（タイトル・アーティスト・アルバム）とアルバムアートは、Rust側が`trackId`からライブラリで読む。タイトルがなければファイル名を出す。アルバムアートは、曲が変わったときだけ読む
  - `duration`は、再生エンジンがファイルから読んだ値。`null`・0以下ならライブラリの値を使う。`position`は0〜曲の長さに収める
  - ライブラリにない曲は`NOT_FOUND`（OSの表示は消す）。トラックIDの形式でなければ`VALIDATION`
- フロントは、曲・再生中かどうか・曲の長さ（秒未満の違いを除く）が変わったときと、再生位置が「前に伝えた位置 + 経過時間」と1秒以上ずれたときに呼ぶ。呼び出しは1つずつ行い、待っている間の変更は最後の状態だけを伝える（`nowPlaying.ts`）
- OSへは、再生中かどうか（`playbackState`）・再生位置・再生の速さ（再生中は1、一時停止中は0）も渡す。OSは、渡した時点からの経過で今の位置を計算して表示する
- `PlaybackControl`（`type`で見分ける。フロントの`Player.svelte`が、ウィンドウの中のキー操作と同じ処理を行う）

| `type`          | 内容                                            | 送信元                                 |
| --------------- | ----------------------------------------------- | -------------------------------------- |
| `toggle`        | 再生と一時停止を切り替える                      | メニュー・OS                           |
| `play`          | 再生する（再生中なら何もしない）                | OS                                     |
| `pause`         | 一時停止する（一時停止中なら何もしない）        | OS                                     |
| `next`          | 次の曲へ進む                                    | メニュー・OS                           |
| `previous`      | 前の曲へ戻る（3秒以上再生していれば、曲の頭へ） | メニュー・OS                           |
| `seek`          | `position`（秒）へ移動する                      | OS（コントロールセンターのシークバー） |
| `volumeUp`      | 音量を0.1上げる                                 | メニュー                               |
| `volumeDown`    | 音量を0.1下げる                                 | メニュー                               |
| `toggleMute`    | ミュートを切り替える                            | メニュー                               |
| `toggleShuffle` | シャッフルを切り替える                          | メニュー                               |
| `toggleRepeat`  | リピートを切り替える（オフ → 全曲 → 1曲）       | メニュー                               |

- 「再生」メニューの項目のID（`playback_toggle`など）と操作の対応は、`menu.rs`の`PLAYBACK_ITEMS`に持つ。項目にキーは割り当てない

## バリデーション仕様

| 対象                       | ルール                                                      |
| -------------------------- | ----------------------------------------------------------- |
| `track_id` / `playlist_id` | UUID形式（36文字、ハイフン区切り、16進数）                  |
| `device_id`                | UUID形式（同上）                                            |
| デバイス名                 | 必須（前後の空白を除く）、100バイト以内、制御文字禁止       |
| `playlist_name`            | 必須、100文字以内、危険文字禁止（`<>:"/\\\|?*`）            |
| ファイルパス               | 空/Null文字禁止、`..`による親ディレクトリ遡り禁止、長さ制限 |
| `Metadata.year`            | 1000〜9999                                                  |
| `Metadata.trackNumber`     | 1〜999                                                      |
| 文字列長                   | title/artist/album: 255、genre: 100                         |
| `rating`                   | 0〜5                                                        |

## エラーハンドリング

- Rustコマンドは `AppResult<T>` を返し、エラーは `{ code, message }`（messageはユーザー向けの日本語）
- DBロック/クエリエラーは文脈付きメッセージに変換
- フロントエンドは `handleError` を通して統一表示する。表示の文言はcodeごとの汎用メッセージ（`LOCK`・`DATABASE`・`IO`・`METADATA`・`PLAYBACK`）か、日本語の`NOT_FOUND`・`VALIDATION`ではバックエンドのメッセージ。英語ではすべてcodeごとの汎用メッセージにする
- ミューテーション成功時はTanStack Queryのinvalidateで整合性を回復する。評価・お気に入りのように1曲の一部だけが変わる操作は、全曲の一覧を取り直さず、キャッシュの該当曲を書き換える（`queries/trackCache.ts`）

## 実装上の注意

- `update_track_metadata`・`update_multiple_tracks_metadata` は、`albumArtist`が指定された場合だけアルバムアーティストを変える（編集画面に項目がなく、ファイルのタグも指定された場合だけ書き込む）
- `update_track_metadata` は `track_number`・`disc_number` を変えない（編集画面に項目がない）。これらはファイルを読み直した時（再スキャン・`refresh_library_metadata`）に反映する
- メタデータの編集・評価の変更では、DBはコマンドが書き込んだ項目だけを更新する（ファイル全体を読み直さない）。ファイルの内容をDBにそのまま反映するのは、インポート・再スキャン（変更のあったファイル）・`refresh_library_metadata`・`write_library_metadata_to_files`
- 検索クエリは `sanitize_search_query` で危険文字を除去してから検索する
- 大量更新系（インポート・一括編集）はトランザクションを使って部分失敗の影響を抑える
