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
  replayGain: ReplayGain;
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

- `ImportResult`: `importedCount`, `skippedCount`, `errorCount`, `errors[]`
- `DeleteResult`: `successCount`, `failedCount`, `failedTracks[]`
- `DuplicateAction`: `Skip | Replace`

### ライブラリフォルダ型

- `LibraryFolder`: `id`, `path`, `trackCount`, `exists`（フォルダが今も見つかるか）, `addedAt`, `lastScannedAt`
- `LibraryFolderList`: `folders[]`, `unregisteredTrackCount`（どのフォルダにも属さない曲数）
- `RescanResult`: `addedCount`, `updatedCount`, `removedCount`, `errorCount`, `errors[]`, `removalSkipped`（音楽ファイルが見つからず、曲を外さなかった）

## データベース仕様（SQLite）

### テーブル

| テーブル          | 用途               | 主なカラム                                                                                                                                                                                                           |
| ----------------- | ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tracks`          | トラック本体       | `id`, `file_path`, `title`, `artist`, `album`, `genre`, `year`, `track_number`, `disc_number`, `duration`, `file_size`, `file_modified_at`, `is_favorite`, `rating`, `play_count`, `last_played_at`, `replay_gain_*` |
| `library_folders` | ライブラリフォルダ | `id`, `path`（UNIQUE）, `added_at`, `last_scanned_at`                                                                                                                                                                |
| `playlists`       | プレイリスト本体   | `id`, `name`, `description`, `created_at`, `updated_at`                                                                                                                                                              |
| `playlist_tracks` | プレイリスト内順序 | `playlist_id`, `track_id`, `position`, `added_at`                                                                                                                                                                    |
| `play_history`    | 再生履歴           | `id`, `track_id`, `played_at`                                                                                                                                                                                        |
| `tracks_fts`      | 全文検索（FTS5）   | `id`, `title`, `artist`, `album`, `genre`                                                                                                                                                                            |

### インデックス/制約

- `tracks(artist|album|genre|title)` にインデックス
- `playlist_tracks(playlist_id)` にインデックス
- `playlist_tracks`, `play_history` は `tracks` / `playlists` への外部キー（`ON DELETE CASCADE`）
- `tracks.file_modified_at` はファイルの更新日時（UNIX時間の秒）。再スキャンで、サイズとあわせて変更の検出に使う。列を追加する前に登録したトラックはNULLで、再スキャンで（ファイルを読み直さずに）記録する。サイズが同じで更新日時がちょうど1時間（±2秒）ずれた場合は、夏時間の切り替えによるずれ（更新日時をローカル時刻で記録するFAT32など）とみなし、読み直さずに日時だけ記録する
- `tracks.replay_gain_track_gain` / `replay_gain_track_peak` / `replay_gain_album_gain` / `replay_gain_album_peak`（REAL）はファイルのタグから読んだReplayGain。`REPLAYGAIN_*`を優先し、ゲインがない場合はOpusの`R128_*_GAIN`（Q7.8形式、基準が-23 LUFSのため+5 dBしてReplayGainの基準にそろえる）を使う。範囲外（ゲインは±60 dB、ピークは0以下）は読まない。列を追加する前に登録したトラックはNULLで、`refresh_library_metadata`か再スキャン（ファイルが変わった場合）で読み込む
- `library_folders` とトラックの対応は `tracks.file_path` の前方一致（フォルダのパス＋区切り文字）で判定する。`file_path >= 接頭辞 AND file_path < 上限`（接頭辞の最後の文字を次の文字にしたもの）の範囲で比較する（二分比較のため大文字・小文字を区別し、`file_path`のUNIQUEのインデックスを使える。`LIKE`はASCIIの大文字・小文字を区別しない）。フォルダ同士は入れ子にしない（中のフォルダはインポートしても登録せず、外のフォルダをインポートすると中のフォルダの記録をまとめる）
- `tracks_fts` は `tracks` とINSERT/UPDATE/DELETEトリガーで同期
  - external contentテーブル（`content=tracks`）のため、UPDATE/DELETEは`'delete'`コマンドパターンで古いトークンを除去する
  - 旧トリガー（直接DELETE/UPDATE方式）によるインデックス破損対策として、`PRAGMA user_version < 1` の場合に起動時へ一度だけ`rebuild`を実行する

### クエリ制限

- 一覧/検索の既定上限: 1000件（`DEFAULT_QUERY_LIMIT`）
- 検索は FTS5 優先、失敗時に `LIKE` へフォールバック

## Tauriコマンド仕様

フロントエンドからは `invoke()` で呼び出す。引数キーはcamelCase（例: `trackId`）で渡す。

### ライブラリ取得・検索

| コマンド                           | 引数                                   | 戻り値          | 備考                     |
| ---------------------------------- | -------------------------------------- | --------------- | ------------------------ |
| `get_all_tracks`                   | なし                                   | `Track[]`       | 作成日時降順、最大1000件 |
| `search_tracks`                    | `query: string`                        | `Track[]`       | sanitize後にFTS5検索     |
| `filter_tracks`                    | `filters: { artist?, album?, genre? }` | `Track[]`       | 完全一致フィルタ         |
| `get_unique_artists/albums/genres` | なし                                   | `string[]`      | フィルタ候補用           |
| `get_albums_grouped`               | なし                                   | `AlbumGroup[]`  | アルバム表示用           |
| `get_artists_grouped`              | なし                                   | `ArtistGroup[]` | アーティスト表示用       |
| `get_genres_grouped`               | なし                                   | `GenreGroup[]`  | ジャンル表示用           |

### インポート・削除

| コマンド                           | 引数                            | 戻り値                  | 備考                                                                                                                                          |
| ---------------------------------- | ------------------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `import_folder`                    | `folderPath`, `duplicateAction` | `ImportResult`          | 50件/トランザクション、`ImportProgress`イベント送信。音楽ファイルが見つかったフォルダをライブラリフォルダとして記録し、`LibraryChanged`を送る |
| `delete_tracks_command`            | `trackIds: string[]`            | `number`                | DBからのみ削除                                                                                                                                |
| `delete_tracks_with_files_command` | `trackIds: string[]`            | `DeleteResult`          | DB+ファイル削除                                                                                                                               |
| `refresh_library_metadata`         | なし                            | `RefreshMetadataResult` | 全トラックのファイルを読み直し、タグの内容（タイトルなど・評価・track/disc番号・ReplayGain）を反映する。お気に入り・再生回数は変えない        |

### ライブラリフォルダ

| コマンド                | 引数                       | 戻り値              | 備考                                                                                                                                                                                                                                     |
| ----------------------- | -------------------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_library_folders`   | なし                       | `LibraryFolderList` | パス順                                                                                                                                                                                                                                   |
| `rescan_library_folder` | `folderId`                 | `RescanResult`      | 追加・変更（サイズか更新日時が違う）のファイルを読み込み、見つからないファイルの曲を外す。フォルダが見つからない場合は`NOT_FOUND`。`LibraryScanProgress`を送る。途中で失敗した場合も、それまでに書き込んだ分は`LibraryChanged`で通知する |
| `remove_library_folder` | `folderId`, `removeTracks` | `number`            | 記録を削除する。`removeTracks`ならフォルダ内の曲もライブラリから外す（ファイルは消さない）。外した曲数を返す                                                                                                                             |

再スキャン・削除でライブラリの曲が変わると`LibraryChanged`イベントを送る。

#### 変更の自動反映（`library_sync.rs`）

設定（`watchLibraryFolders`・`libraryScanIntervalMinutes`）に応じて、`rescan_library_folder`と同じ処理でライブラリフォルダを自動で再スキャンする（`LibraryScanProgress`は送らない）。

- 起動時: どちらかが有効なら、起動の5秒後に全フォルダを再スキャンする
- 定期: 設定した間隔ごとに全フォルダを再スキャンする（あわせて監視するフォルダを更新する）
- 監視: ファイルの変更の通知を3秒まとめ、変更のあったパスを含むフォルダだけを再スキャンする。対象は音楽ファイル・今あるフォルダ・なくなったパスのうちライブラリの曲を含むもの（なくなった一時ファイル・画像などでは再スキャンしない）。アプリのデータ（DB・ログなど）のフォルダの中の変更は無視する
- 見つからないフォルダ（外付けドライブが外れているなど）は飛ばす。監視は、監視を始めた時点で見つかるフォルダだけが対象（フォルダの追加・削除・定期の再スキャンのときに更新する。監視を作れなかった場合も、そのときに作り直す）
- `save_settings`は、自動反映への反映（監視の開始）を待たずに返る。反映は、その時点で保存済みの設定を読む
- インポート・再スキャン・ライブラリフォルダの削除は`AppState::lock_library_scan`で1つずつ行う（手動と自動が同じファイルを同時に登録しない。削除したフォルダの曲を再スキャンが書き戻さない）

### 設定

| コマンド        | 引数                 | 戻り値     | 備考                                                                                                                                                                      |
| --------------- | -------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_settings`  | なし                 | `Settings` | `settings.json`がない・壊れている場合は既定値                                                                                                                             |
| `save_settings` | `settings: Settings` | `void`     | アクセントカラーは`#rrggbb`、クロスフェードは0〜12秒、再スキャンの間隔は選択肢の値。保存後に`SettingsChanged`イベントを送り、ライブラリフォルダの自動反映に設定を反映する |

`Settings`: `{ language: 'ja' \| 'en', startupPage: 'lastOpened' \| 'songs', theme: 'dark' \| 'light' \| 'system', accentColor: string, volumeNormalization: 'off' \| 'track' \| 'album', gaplessPlayback: boolean, crossfadeSeconds: number, watchLibraryFolders: boolean, libraryScanIntervalMinutes: number }`。既定値は`ja`・`lastOpened`・`dark`・`#3b82f6`・`off`・`true`・`0`・`false`・`0`。`crossfadeSeconds`は0〜12（整数）、`libraryScanIntervalMinutes`は0（しない）・15・30・60・360のいずれかで、それ以外は`VALIDATION_ERROR`。ファイルにない項目は既定値で補う（項目を追加しても古いファイルを読める）。

### カスタムプロトコル

| URL                                                                                | 応答                                                                                       | 備考                                                                                        |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| `albumart://localhost/<trackId>`（Windowsは`http://albumart.localhost/<trackId>`） | 画像（`Content-Type`は埋め込み画像のMIMEタイプ） / アートなし・未登録は404 / 不正なIDは400 | `<img>`から読み込む。フロントは`albumArtUrl(trackId)`でURLを作る。`Cache-Control: no-store` |

### イベント（バックエンド→フロントエンド）

`src-tauri/src/events.rs` で定義し、`bindings.ts` の `events` から型付きで購読する。イベント名は型名のケバブケース。

| イベント              | ペイロード                        | 送信元                                                                             |
| --------------------- | --------------------------------- | ---------------------------------------------------------------------------------- |
| `ImportProgress`      | `{ current, total, currentFile }` | `import_folder`（1ファイルごと）                                                   |
| `LibraryScanProgress` | `{ current, total, currentFile }` | `rescan_library_folder`（読み込むファイルごと）                                    |
| `LibraryChanged`      | なし                              | インポートの後、再スキャン（自動を含む）・ライブラリフォルダの削除で曲が変わった時 |
| `ShowAboutDialog`     | なし                              | メニュー「Muspice について」                                                       |
| `OpenImportDialog`    | なし                              | メニュー「フォルダをインポート...」                                                |
| `ToggleSidebar`       | なし                              | メニュー「サイドバーを表示/隠す」                                                  |
| `SettingsChanged`     | `Settings`                        | `save_settings`                                                                    |

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
| `rename_playlist`            | `playlistId`, `name`     | `void`       |
| `delete_playlist`            | `playlistId`             | `void`       |
| `add_tracks_to_playlist`     | `playlistId`, `trackIds` | `number`     |
| `remove_track_from_playlist` | `playlistId`, `trackId`  | `void`       |
| `reorder_playlist_tracks`    | `playlistId`, `trackIds` | `void`       |

`add_tracks_to_playlist`は、複数のトラックを渡した順に1つのトランザクションで追加する。すでに入っているトラックは飛ばし、追加したトラック数を返す。見つからないトラックがある場合は`NOT_FOUND`で、1曲も追加しない。

### 再生・統計・システム

| コマンド                     | 引数                      | 戻り値          |
| ---------------------------- | ------------------------- | --------------- |
| `get_track_file_path`        | `trackId`                 | `string`        |
| `set_current_track`          | `trackId: string \| null` | `void`          |
| `get_current_track`          | なし                      | `Track \| null` |
| `toggle_favorite`            | `trackId`                 | `boolean`       |
| `set_rating`                 | `trackId`, `rating`       | `void`          |
| `increment_play_count`       | `trackId`                 | `number`        |
| `get_favorite_tracks`        | なし                      | `Track[]`       |
| `get_most_played_tracks`     | `limit?`                  | `Track[]`       |
| `get_recently_played_tracks` | `limit?`                  | `Track[]`       |
| `show_in_folder`             | `trackId`                 | `void`          |
| `open_project_page`          | なし                      | `void`          |

`set_rating`は、評価をファイルのタグへ書き込み、同じ値をDBに記録する（0は評価のタグを取り除く）。評価はloftyの`ItemKey::Popularimeter`（ID3v2 `POPM` / Vorbis `RATING` / MP4 `rate` / RIFF `IRTD`）で読み書きし、インポート・再スキャン・`refresh_library_metadata`でタグから読み込む。評価の数値の付け方は書き込んだアプリごとに違うため、すでに評価があるファイルではその書き手の付け方のまま星の数だけを変え、ない場合はMusicBeeの付け方（ID3v2は1・64・128・196・255、それ以外は20刻み）で書く。

`toggle_favorite`・`increment_play_count`（お気に入り・再生回数・再生履歴）とプレイリストは、タグでは持てないためDBだけに保存する。

## バリデーション仕様

| 対象                       | ルール                                                      |
| -------------------------- | ----------------------------------------------------------- |
| `track_id` / `playlist_id` | UUID形式（36文字、ハイフン区切り、16進数）                  |
| `playlist_name`            | 必須、100文字以内、危険文字禁止（`<>:"/\\\|?*`）            |
| ファイルパス               | 空/Null文字禁止、`..`による親ディレクトリ遡り禁止、長さ制限 |
| `Metadata.year`            | 1000〜9999                                                  |
| `Metadata.trackNumber`     | 1〜999                                                      |
| 文字列長                   | title/artist/album: 255、genre: 100                         |
| `rating`                   | 0〜5                                                        |

## エラーハンドリング

- Rustコマンドは `AppResult<T>` を返し、エラーは `{ code, message }`（messageはユーザー向けの日本語）
- DBロック/クエリエラーは文脈付きメッセージに変換
- フロントエンドは `handleError` を通して統一表示する。表示の文言はcodeごとの汎用メッセージ（`LOCK`・`DATABASE`・`IO`・`METADATA`）か、日本語の`NOT_FOUND`・`VALIDATION`ではバックエンドのメッセージ。英語ではすべてcodeごとの汎用メッセージにする
- ミューテーション成功時はTanStack Queryのinvalidateで整合性を回復

## 実装上の注意

- `update_track_metadata` は `track_number`・`disc_number` を変えない（編集画面に項目がない）。これらはファイルを読み直した時（再スキャン・`refresh_library_metadata`）に反映する
- メタデータの編集・評価の変更では、DBはコマンドが書き込んだ項目だけを更新する（ファイル全体を読み直さない）。ファイルの内容をDBにそのまま反映するのは、インポート・再スキャン（変更のあったファイル）・`refresh_library_metadata`・`write_library_metadata_to_files`
- 検索クエリは `sanitize_search_query` で危険文字を除去してから検索する
- 大量更新系（インポート・一括編集）はトランザクションを使って部分失敗の影響を抑える
