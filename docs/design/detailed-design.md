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

### 転送先デバイス型

- `SyncDevice`: `id`, `name`, `path`（転送先のフォルダ）, `syncAll`（全曲を同期するか）, `playlistIds`（同期するプレイリスト）, `removeUnselected`（対象から外れた曲をデバイスから削除するか）, `connected`（転送先のフォルダに、このデバイスの管理ファイルがあるか）, `freeBytes` / `totalBytes`（接続されていなければnull）, `createdAt`, `lastSyncedAt`
- `SyncDeviceConfig`: `name`, `syncAll`, `playlistIds`, `removeUnselected`
- `DeviceSyncPlan`: `copyCount` / `copyBytes`, `deleteCount` / `deleteBytes`, `renameCount`, `unchangedCount`, `playlistCount`, `missingSourceCount`（元のファイルが見つからない曲数）, `freeBytes`, `requiredBytes`（コピーする容量から、削除で空く容量を引いたもの）, `hasEnoughSpace`
- `DeviceSyncResult`: `copiedCount`, `deletedCount`, `renamedCount`, `playlistCount`, `errorCount`, `errors[]`, `cancelled`

## データベース仕様（SQLite）

### テーブル

| テーブル                | 用途                           | 主なカラム                                                                                                                                                                                                           |
| ----------------------- | ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tracks`                | トラック本体                   | `id`, `file_path`, `title`, `artist`, `album`, `genre`, `year`, `track_number`, `disc_number`, `duration`, `file_size`, `file_modified_at`, `is_favorite`, `rating`, `play_count`, `last_played_at`, `replay_gain_*` |
| `library_folders`       | ライブラリフォルダ             | `id`, `path`（UNIQUE）, `added_at`, `last_scanned_at`                                                                                                                                                                |
| `playlists`             | プレイリスト本体               | `id`, `name`, `description`, `created_at`, `updated_at`                                                                                                                                                              |
| `playlist_tracks`       | プレイリスト内順序             | `playlist_id`, `track_id`, `position`, `added_at`                                                                                                                                                                    |
| `play_history`          | 再生履歴                       | `id`, `track_id`, `played_at`                                                                                                                                                                                        |
| `tracks_fts`            | 全文検索（FTS5）               | `id`, `title`, `artist`, `album`, `genre`                                                                                                                                                                            |
| `sync_devices`          | 転送先デバイス                 | `id`, `name`, `path`, `sync_all`, `remove_unselected`, `created_at`, `last_synced_at`                                                                                                                                |
| `sync_device_playlists` | デバイスに同期するプレイリスト | `device_id`, `playlist_id`                                                                                                                                                                                           |

### インデックス/制約

- `tracks(artist|album|genre|title)` にインデックス
- `playlist_tracks(playlist_id)` にインデックス
- `playlist_tracks`, `play_history` は `tracks` / `playlists` への外部キー（`ON DELETE CASCADE`）
- `tracks.file_modified_at` はファイルの更新日時（UNIX時間の秒）。再スキャンで、サイズとあわせて変更の検出に使う。列を追加する前に登録したトラックはNULLで、再スキャンで（ファイルを読み直さずに）記録する。サイズが同じで更新日時がちょうど1時間（±2秒）ずれた場合は、夏時間の切り替えによるずれ（更新日時をローカル時刻で記録するFAT32など）とみなし、読み直さずに日時だけ記録する
- `tracks.replay_gain_track_gain` / `replay_gain_track_peak` / `replay_gain_album_gain` / `replay_gain_album_peak`（REAL）はファイルのタグから読んだReplayGain。`REPLAYGAIN_*`を優先し、ゲインがない場合はOpusの`R128_*_GAIN`（Q7.8形式、基準が-23 LUFSのため+5 dBしてReplayGainの基準にそろえる）を使う。範囲外（ゲインは±60 dB、ピークは0以下）は読まない。列を追加する前に登録したトラックはNULLで、`refresh_library_metadata`か再スキャン（ファイルが変わった場合）で読み込む
- `library_folders` とトラックの対応は `tracks.file_path` の前方一致（フォルダのパス＋区切り文字）で判定する。`file_path >= 接頭辞 AND file_path < 上限`（接頭辞の最後の文字を次の文字にしたもの）の範囲で比較する（二分比較のため大文字・小文字を区別し、`file_path`のUNIQUEのインデックスを使える。`LIKE`はASCIIの大文字・小文字を区別しない）。フォルダ同士は入れ子にしない（中のフォルダはインポートしても登録せず、外のフォルダをインポートすると中のフォルダの記録をまとめる）
- `sync_device_playlists` は `sync_devices` / `playlists` への外部キー（`ON DELETE CASCADE`）。取得するときも `playlists` と結合し、削除済みのプレイリストを含めない
- `sync_devices.id` はデバイス側の管理ファイル（後述）の `deviceId` と同じ値。デバイスにコピーしたファイルの一覧はDBに持たず、管理ファイルに記録する
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
| `refresh_library_metadata`         | なし                            | `RefreshMetadataResult` | 全トラックのtrack/disc番号とReplayGainを再抽出                                                                                                |

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

### メタデータ編集

| コマンド                          | 引数                   | 戻り値 | 備考                                                                                                |
| --------------------------------- | ---------------------- | ------ | --------------------------------------------------------------------------------------------------- |
| `update_track_metadata`           | `trackId`, `metadata`  | `void` | DBのみ更新                                                                                          |
| `update_track_metadata_with_file` | `trackId`, `metadata`  | `void` | ファイルタグ+DB更新（書き込み後のファイルのサイズ・更新日時も記録し、再スキャンで変更とみなさない） |
| `update_multiple_tracks_metadata` | `trackIds`, `metadata` | `void` | None以外の項目のみ更新                                                                              |

### プレイリスト

| コマンド                     | 引数                     | 戻り値       |
| ---------------------------- | ------------------------ | ------------ |
| `create_playlist`            | `name`                   | `Playlist`   |
| `get_playlists`              | なし                     | `Playlist[]` |
| `rename_playlist`            | `playlistId`, `name`     | `void`       |
| `delete_playlist`            | `playlistId`             | `void`       |
| `add_track_to_playlist`      | `playlistId`, `trackId`  | `void`       |
| `remove_track_from_playlist` | `playlistId`, `trackId`  | `void`       |
| `reorder_playlist_tracks`    | `playlistId`, `trackIds` | `void`       |

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
- フロントエンドは `handleError` を通して統一表示する。表示の文言はcodeごとの汎用メッセージ（`LOCK`・`DATABASE`・`IO`・`METADATA`）か、日本語の`NOT_FOUND`・`VALIDATION`ではバックエンドのメッセージ。英語ではすべてcodeごとの汎用メッセージにする
- ミューテーション成功時はTanStack Queryのinvalidateで整合性を回復

## 実装上の注意

- `update_track_metadata` は現在 `disc_number` を更新対象に含めない。`discNumber` の再同期は `refresh_library_metadata` で実施する
- 検索クエリは `sanitize_search_query` で危険文字を除去してから検索する
- 大量更新系（インポート・一括編集）はトランザクションを使って部分失敗の影響を抑える
