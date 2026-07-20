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

## データベース仕様（SQLite）

### テーブル

| テーブル          | 用途               | 主なカラム                                                                                                                                                         |
| ----------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `tracks`          | トラック本体       | `id`, `file_path`, `title`, `artist`, `album`, `genre`, `year`, `track_number`, `disc_number`, `duration`, `is_favorite`, `rating`, `play_count`, `last_played_at` |
| `playlists`       | プレイリスト本体   | `id`, `name`, `description`, `created_at`, `updated_at`                                                                                                            |
| `playlist_tracks` | プレイリスト内順序 | `playlist_id`, `track_id`, `position`, `added_at`                                                                                                                  |
| `play_history`    | 再生履歴           | `id`, `track_id`, `played_at`                                                                                                                                      |
| `tracks_fts`      | 全文検索（FTS5）   | `id`, `title`, `artist`, `album`, `genre`                                                                                                                          |

### インデックス/制約

- `tracks(artist|album|genre|title)` にインデックス
- `playlist_tracks(playlist_id)` にインデックス
- `playlist_tracks`, `play_history` は `tracks` / `playlists` への外部キー（`ON DELETE CASCADE`）
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

| コマンド                           | 引数                            | 戻り値                  | 備考                                                 |
| ---------------------------------- | ------------------------------- | ----------------------- | ---------------------------------------------------- |
| `import_folder`                    | `folderPath`, `duplicateAction` | `ImportResult`          | 50件/トランザクション、`import-progress`イベント送信 |
| `delete_tracks_command`            | `trackIds: string[]`            | `number`                | DBからのみ削除                                       |
| `delete_tracks_with_files_command` | `trackIds: string[]`            | `DeleteResult`          | DB+ファイル削除                                      |
| `refresh_library_metadata`         | なし                            | `RefreshMetadataResult` | 全トラックのtrack/disc番号を再抽出                   |

### メタデータ編集

| コマンド                          | 引数                   | 戻り値 | 備考                   |
| --------------------------------- | ---------------------- | ------ | ---------------------- |
| `update_track_metadata`           | `trackId`, `metadata`  | `void` | DBのみ更新             |
| `update_track_metadata_with_file` | `trackId`, `metadata`  | `void` | ファイルタグ+DB更新    |
| `update_multiple_tracks_metadata` | `trackIds`, `metadata` | `void` | None以外の項目のみ更新 |
| `validate_metadata_command`       | `metadata`             | `void` | フロント事前検証用     |

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

| コマンド                     | 引数                      | 戻り値             |
| ---------------------------- | ------------------------- | ------------------ |
| `get_track_file_path`        | `trackId`                 | `string`           |
| `set_current_track`          | `trackId: string \| null` | `void`             |
| `get_current_track`          | なし                      | `Track \| null`    |
| `get_album_art`              | `trackId`                 | `AlbumArt \| null` |
| `toggle_favorite`            | `trackId`                 | `boolean`          |
| `set_rating`                 | `trackId`, `rating`       | `void`             |
| `increment_play_count`       | `trackId`                 | `number`           |
| `get_favorite_tracks`        | なし                      | `Track[]`          |
| `get_most_played_tracks`     | `limit?`                  | `Track[]`          |
| `get_recently_played_tracks` | `limit?`                  | `Track[]`          |
| `show_in_folder`             | `path`                    | `void`             |

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

- Rustコマンドは `Result<T, String>` を返し、ユーザー向け日本語メッセージを返却
- DBロック/クエリエラーは文脈付きメッセージに変換
- フロントエンドは `handleError` を通して統一表示
- ミューテーション成功時はTanStack Queryのinvalidateで整合性を回復

## 実装上の注意

- `update_track_metadata` は現在 `disc_number` を更新対象に含めない。`discNumber` の再同期は `refresh_library_metadata` で実施する
- 検索クエリは `sanitize_search_query` で危険文字を除去してから検索する
- 大量更新系（インポート・一括編集）はトランザクションを使って部分失敗の影響を抑える
