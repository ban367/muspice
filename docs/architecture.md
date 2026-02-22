# アーキテクチャ

## フロントエンドアーキテクチャ

**SPAモード**: TauriにはNode.jsサーバーがないため、アプリはadapter-staticを使用したSPAモード（SSRなし）でSvelteKitを使用します。

### ハイブリッド状態管理アプローチ

1. **Svelte Stores（UI状態）**:
   - リアルタイムで変化するUI状態を管理
   - 再生状態（isPlaying、currentTime、duration、volume）
   - UI表示モード（viewMode、selectedTracks）
   - 派生store（progress）

2. **TanStack Query（サーバー状態キャッシング）**:
   - 音楽ライブラリデータの取得とキャッシング
   - 検索結果のキャッシング
   - プレイリスト一覧の取得
   - 自動再取得とバックグラウンド更新
   - staleTime: 5-10分、gcTime: 15-30分

3. **Tauri State（バックエンド永続状態）**:
   - データベース接続（Mutex-wrapped）
   - 現在再生中のトラックID
   - アプリケーション設定

### 状態管理の使い分け

| 状態の種類     | 管理方法       | 理由                   |
| -------------- | -------------- | ---------------------- |
| 再生状態・音量 | Svelte Stores  | リアルタイム更新が必要 |
| UI表示モード   | Svelte Stores  | ローカルUI状態         |
| 音楽ライブラリ | TanStack Query | キャッシングが有効     |
| 検索結果       | TanStack Query | 重複リクエスト防止     |
| DB接続         | Tauri State    | バックエンドリソース   |

### コンポーネント構造

- `src/routes/+layout.svelte`: ドロワーナビゲーションとグローバルPlayerコンポーネントを含むルートレイアウト
- `src/lib/components/`: 再利用可能なコンポーネント（Player、Library、Playlist、MetadataEditor、ImportDialog）
- `src/lib/components/ui/`: 基本UIコンポーネント（Button、Input、Card、Modal、Toast、LoadingSpinner）
- `src/lib/queries/`: データフェッチ用のTanStack Queryフック（`tracks.ts`、`playlists.ts`）
- `src/lib/stores/`: クライアント側状態用のSvelteストア（`player.ts`、`ui.ts`）
- `src/lib/types/`: Rustモデルに対応するTypeScript型定義

### フロントエンド・バックエンド間通信

- `@tauri-apps/api/core`のinvoke()を使用してRustコマンドを呼び出し
- Tauriコマンドからのエラーは`handleError`ユーティリティで一元管理

## バックエンドアーキテクチャ

### モジュール構成 (`src-tauri/src/`)

- `lib.rs`: エントリーポイント、コマンド登録、アプリ初期化
- `commands.rs`: Tauriコマンドハンドラー（インポート、検索、CRUD操作）
- `db.rs`: データベース初期化とマイグレーション
- `models.rs`: データモデル（Track、Playlist、Metadata）
- `library.rs`: ファイルスキャンとインポートロジック
- `metadata.rs`: `lofty`クレートを使用したオーディオメタデータ抽出
- `playlist.rs`: プレイリスト管理ロジック
- `validation.rs`: 入力バリデーションユーティリティ
- `error.rs`: カスタムエラー型
- `logger.rs`: ロギング設定
- `state.rs`: アプリケーション状態（Connectionとcurrent_track_idを持つAppState）

### データベーススキーマ

- `tracks`: メタデータを含むコアトラック情報（id、file_path、title、artist、album、genre、year、duration、file_size、format、bitrate、sample_rate）
- `playlists`: プレイリストメタデータ（id、name、description）
- `playlist_tracks`: プレイリストとトラックを位置順序でリンクする中間テーブル（playlist_id、track_id、position、added_at）
- `tracks_fts`: 自動同期トリガー付きの全文検索用FTS5仮想テーブル
- クエリパフォーマンス向上のためartist、album、genre、titleにインデックス

### Tauriコマンド (`lib.rs`に登録)

- トラック管理: `get_all_tracks`、`search_tracks`、`filter_tracks`、`get_unique_artists`、`get_unique_albums`、`get_unique_genres`
- メタデータ: `update_track_metadata`（DBのみ）、`update_track_metadata_with_file`（DB+ファイル）、`update_multiple_tracks_metadata`、`validate_metadata_command`
- インポート: `import_folder`（トランザクション付きバッチ処理）
- プレイリスト: `create_playlist`、`get_playlists`、`add_track_to_playlist`、`remove_track_from_playlist`、`reorder_playlist_tracks`
- プレイヤー: `get_track_file_path`、`set_current_track`、`get_current_track`

### パフォーマンス最適化

- 50ファイルごとにトランザクションコミットを行うバッチインポート
- LIKE検索への自動フォールバック付きFTS5全文検索
- 頻繁にクエリされるフィールドへのデータベースインデックス
- クエリ結果の制限（1000トラック）
- デバウンス処理（検索300ms）
- 仮想スクロール（100曲以上のリスト）

## データフロー

### 音楽再生フロー

1. ユーザーがトラックをクリック
2. TanStack Queryがキャッシュからトラック情報を取得
3. Tauri commandでファイルパスを取得
4. HTML5 Audioで再生開始
5. Svelte Storesで再生状態を更新
6. UIがリアクティブに更新

### 検索フロー

1. ユーザーが検索ワードを入力
2. デバウンス処理（300ms）
3. TanStack Queryが検索を実行
4. キャッシュがあれば即座に表示
5. なければTauri commandで検索
6. 結果をキャッシュして表示

### 一般的なデータフロー

1. ユーザーがSvelteコンポーネントと対話
2. コンポーネントがTanStack Queryフック（`useTracksQuery`など）を呼び出し
3. Queryフックが`invoke()`経由でTauriコマンドを呼び出し
4. Rustコマンドハンドラーが入力を検証し、データベースと対話
5. 結果がシリアライズされてフロントエンドに返される
6. TanStack Queryが結果をキャッシュしてUIを更新
