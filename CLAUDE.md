# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 言語設定 / Language Settings

**このプロジェクトでは日本語でコミュニケーションを行ってください。**

- すべての応答は日本語で行うこと
- コードコメントは日本語で記述すること
- エラーメッセージは日本語で記述すること
- **コミットメッセージは英語で記述すること**
- ユーザーとのやり取りは常に日本語で行うこと

## プロジェクト概要

MuspiceはTauri 2とSvelteKitで構築されたデスクトップ音楽管理アプリケーションです。ユーザーは音楽ファイルのインポート、整理、メタデータ管理、プレイリスト作成、音楽再生が可能です。

## 技術スタック

- **フロントエンド**: SvelteKit 2.x (SPAモード) + Svelte 5 + TypeScript + TailwindCSS 4 + DaisyUI
- **バックエンド**: Tauri 2.x + Rust
- **データベース**: SQLite with FTS5 (全文検索)
- **状態管理**: ハイブリッドアプローチ
  - **Svelte Stores**: UI状態（再生状態、音量、進行状況）
  - **Tauri State**: データ永続化（ライブラリ、プレイリスト）
  - **TanStack Query**: データフェッチとキャッシング
- **音楽メタデータ**: lofty (Rust crate)
- **音楽再生**: HTML5 Audio API
- **ビルドツール**: Vite 6

## 開発コマンド

### セットアップ

```bash
npm install                    # フロントエンド依存関係をインストール
cd src-tauri && cargo build   # Rustバックエンドをビルド
```

### 開発

```bash
npm run tauri dev             # Tauriアプリを開発モードで実行（推奨）
npm run dev                   # フロントエンドのみ実行（ポート1420）
```

### テスト

```bash
cd src-tauri && cargo test    # Rustユニットテストを実行
```

### コード品質

```bash
npm run check                 # TypeScript型チェック
npm run lint                  # ESLintを実行
npm run lint:fix              # ESLint自動修正
npm run format                # Prettierでフォーマット
npm run format:check          # フォーマットチェック
```

### ビルド

```bash
npm run build                 # フロントエンドをビルド
npm run tauri build           # 本番用Tauriアプリをビルド
```

## 機能要件

### 1. 音楽ファイルのインポートと整理

- フォルダ選択による再帰的なファイルスキャン
- MP3、FLAC、WAV、M4Aファイル形式のサポート
- 自動メタデータ抽出（タイトル、アーティスト、アルバム、ジャンル、年）
- メタデータがない場合はファイル名をデフォルトタイトルとして使用
- 重複ファイルの検出と処理（スキップ/置換オプション）

### 2. メタデータ編集

- 単一トラックの編集（タイトル、アーティスト、アルバム、ジャンル、年）
- 複数トラックの一括編集（共通フィールド）
- データベースのみ更新 OR データベース+ファイル両方更新（2つのコマンド）
- メタデータバリデーション
- エラー時の自動ロールバック
- 即座のライブラリ表示更新

### 3. プレイリスト管理

- カスタム名での新規プレイリスト作成
- ドラッグ&ドロップによるトラック追加
- プレイリスト内トラックの並び替え
- 元のファイルを削除せずにプレイリストからトラックを削除
- プレイリストデータの永続化と復元

### 4. 音楽再生

- トラックのダブルクリックで再生開始
- 再生コントロール（再生、一時停止、停止、前へ、次へ）
- シーク可能な進行バー
- 音量調整
- 現在再生中トラック情報の表示（タイトル、アーティスト、アルバム）
- プレイリストからの自動連続再生

### 5. 検索とフィルタリング

- リアルタイム検索（タイトル、アーティスト、アルバム、ジャンル対応）
- FTS5全文検索による高速検索
- マッチしたテキストのハイライト表示
- アーティスト、アルバム、ジャンル別フィルター
- 検索結果なしメッセージの表示

## アーキテクチャ

### フロントエンドアーキテクチャ

**SPAモード**: TauriにはNode.jsサーバーがないため、アプリはadapter-staticを使用したSPAモード（SSRなし）でSvelteKitを使用します。

**ハイブリッド状態管理アプローチ**:

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

**状態管理の使い分け**:

| 状態の種類     | 管理方法       | 理由                   |
| -------------- | -------------- | ---------------------- |
| 再生状態・音量 | Svelte Stores  | リアルタイム更新が必要 |
| UI表示モード   | Svelte Stores  | ローカルUI状態         |
| 音楽ライブラリ | TanStack Query | キャッシングが有効     |
| 検索結果       | TanStack Query | 重複リクエスト防止     |
| DB接続         | Tauri State    | バックエンドリソース   |

**コンポーネント構造**:

- `src/routes/+layout.svelte`: ドロワーナビゲーションとグローバルPlayerコンポーネントを含むルートレイアウト
- `src/lib/components/`: 再利用可能なコンポーネント（Player、Library、Playlist、MetadataEditor、ImportDialog）
- `src/lib/components/ui/`: 基本UIコンポーネント（Button、Input、Card、Modal、Toast、LoadingSpinner）
- `src/lib/queries/`: データフェッチ用のTanStack Queryフック（`tracks.ts`、`playlists.ts`）
- `src/lib/stores/`: クライアント側状態用のSvelteストア（`player.ts`、`ui.ts`）
- `src/lib/types/`: Rustモデルに対応するTypeScript型定義

**フロントエンド・バックエンド間通信**:

- `@tauri-apps/api/core`のinvoke()を使用してRustコマンドを呼び出し
- Tauriコマンドからのエラーは`handleError`ユーティリティで一元管理

### バックエンドアーキテクチャ

**モジュール構成** (`src-tauri/src/`):

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

**データベーススキーマ**:

- `tracks`: メタデータを含むコアトラック情報（id、file_path、title、artist、album、genre、year、duration、file_size、format、bitrate、sample_rate）
- `playlists`: プレイリストメタデータ（id、name、description）
- `playlist_tracks`: プレイリストとトラックを位置順序でリンクする中間テーブル（playlist_id、track_id、position、added_at）
- `tracks_fts`: 自動同期トリガー付きの全文検索用FTS5仮想テーブル
- クエリパフォーマンス向上のためartist、album、genre、titleにインデックス

**Tauriコマンド** (`lib.rs`に登録):

- トラック管理: `get_all_tracks`、`search_tracks`、`filter_tracks`、`get_unique_artists`、`get_unique_albums`、`get_unique_genres`
- メタデータ: `update_track_metadata`（DBのみ）、`update_track_metadata_with_file`（DB+ファイル）、`update_multiple_tracks_metadata`、`validate_metadata_command`
- インポート: `import_folder`（トランザクション付きバッチ処理）
- プレイリスト: `create_playlist`、`get_playlists`、`add_track_to_playlist`、`remove_track_from_playlist`、`reorder_playlist_tracks`
- プレイヤー: `get_track_file_path`、`set_current_track`、`get_current_track`

**パフォーマンス最適化**:

- 50ファイルごとにトランザクションコミットを行うバッチインポート
- LIKE検索への自動フォールバック付きFTS5全文検索
- 頻繁にクエリされるフィールドへのデータベースインデックス
- クエリ結果の制限（1000トラック）
- デバウンス処理（検索300ms）
- 仮想スクロール（100曲以上のリスト）

### データフロー

**音楽再生フロー**:

1. ユーザーがトラックをクリック
2. TanStack Queryがキャッシュからトラック情報を取得
3. Tauri commandでファイルパスを取得
4. HTML5 Audioで再生開始
5. Svelte Storesで再生状態を更新
6. UIがリアクティブに更新

**検索フロー**:

1. ユーザーが検索ワードを入力
2. デバウンス処理（300ms）
3. TanStack Queryが検索を実行
4. キャッシュがあれば即座に表示
5. なければTauri commandで検索
6. 結果をキャッシュして表示

**一般的なデータフロー**:

1. ユーザーがSvelteコンポーネントと対話
2. コンポーネントがTanStack Queryフック（`useTracksQuery`など）を呼び出し
3. Queryフックが`invoke()`経由でTauriコマンドを呼び出し
4. Rustコマンドハンドラーが入力を検証し、データベースと対話
5. 結果がシリアライズされてフロントエンドに返される
6. TanStack Queryが結果をキャッシュしてUIを更新

### オーディオファイルサポート

アプリは`lofty`クレートを使用してMP3、FLAC、WAV、M4A形式をサポートします。

## 重要な規約

### エラーハンドリング

**エラータイプ**:

1. `FileNotFoundError`: 音楽ファイルが見つからない
2. `UnsupportedFormatError`: サポートされていないファイル形式
3. `MetadataExtractionError`: メタデータ抽出失敗
4. `DatabaseError`: データベース操作エラー
5. `PlaybackError`: 音楽再生エラー
6. `ValidationError`: データバリデーションエラー

**エラーハンドリング戦略**:

- Rustコマンドは日本語のユーザーフレンドリーなエラーメッセージを持つ`Result<T, String>`を返す
- フロントエンドは一貫したエラー表示のため`handleError`ユーティリティを使用
- すべてのデータベース操作は適切な場合トランザクションでラップ
- グローバルエラーハンドラーで未処理エラーをキャッチしてログ出力
- エラーリカバリー機能（可能な場合は自動復旧）
- すべてのエラーをログファイルに記録
- エラートースト通知でユーザーに通知

### バリデーション

- 入力バリデーションはデータベース操作の前にRustで実行
- `validation.rs`のバリデーションユーティリティ: パス検証、ID検証、文字列長制限
- データ整合性のためのメタデータバリデーション
- フロントエンドでも入力サニタイゼーションを実施
- SQLインジェクション対策

### コードスタイルと命名規則

**ファイル命名**:

- **Svelte**: PascalCase（例: `Library.svelte`, `Player.svelte`）
- **TypeScript**: camelCase（例: `audioPlayer.ts`, `metadataUtils.ts`）
- **Rust**: snake_case（例: `music_library.rs`, `playlist_manager.rs`）

**コード命名**:

- **変数・関数**: camelCase (TypeScript), snake_case (Rust)
- **クラス・型**: PascalCase（両言語共通）
- **定数**: UPPER_SNAKE_CASE（両言語共通）

**コメントとドキュメント**:

- **コードコメント**: 日本語で記述（Rust、TypeScriptともに）
- **エラーメッセージ**: ユーザー向けメッセージは日本語必須
- **ログ出力**: デバッグ情報は英語可、ユーザー向けメッセージは日本語必須
- **技術用語**: 不自然な日本語訳は英語併記

**コード規約**:

- Rust: 標準的なRust規約、日本語での詳細なコメント
- TypeScript: Strictモード有効、型安全なTauri呼び出し
- 利用するファイルに合わせてフォーマットを必ず適用

### 状態管理

- Mutex-wrappedされたConnectionを持つ`AppState`経由での共有データベース状態
- 現在再生中のトラックは`AppState.current_track_id`に保存
- バックエンド呼び出しを減らすためのTanStack Queryによるフロントエンドキャッシング

## セキュリティ考慮事項

### Tauri固有のセキュリティ設定

- allowlistでファイルシステムアクセスを制限
- ファイルスコープ設定（`$AUDIO`、`$DATA`）
- ユーザーが選択したディレクトリのみアクセス許可
- パストラバーサル攻撃の防止

### データ保護

- ローカルデータのみ使用
- 外部サーバーへのデータ送信なし
- すべてのデータはプラットフォーム固有のアプリデータディレクトリに保存

## テスト戦略

### 手動テスト項目

**音楽インポート**:

- フォルダ選択とインポート進行状況
- サポートされているファイル形式の確認
- サブディレクトリの再帰的スキャン
- メタデータ抽出とデフォルト値
- 重複ファイル処理（スキップ/置換）

**プレイリスト**:

- 新規作成、トラック追加、並び替え、削除
- ドラッグ&ドロップ機能
- プレイリスト永続化

**音楽再生**:

- 基本的な再生コントロール
- シーク機能、音量調整
- プレイリスト連続再生
- トラック情報表示

**検索・フィルタリング**:

- リアルタイム検索とハイライト
- フィルター適用と組み合わせ
- 検索結果なしメッセージ

**メタデータ編集**:

- 単一/複数トラック編集
- バリデーションエラー
- 即座のライブラリ更新

**エラーハンドリング**:

- ファイルが見つからない場合
- サポートされていない形式
- データベースエラー

**パフォーマンス**:

- 1000曲以上のライブラリでの動作
- スクロールの滑らかさ
- 検索速度

### 自動テスト

- **Rustユニットテスト**: `cargo test`でサービス層とユーティリティをテスト
- カバレッジ目標: Rustサービス層90%以上

## 重要な注意事項

- アプリはプラットフォーム固有のアプリデータディレクトリにデータを保存（実行時に決定）
- データベースファイル: `{app_data_dir}/muspice.db`
- ログディレクトリ: `{app_data_dir}/logs`
- SvelteKitはSPAモードで実行（サーバーサイドレンダリングなし）
- Tauri統合のためVite開発サーバーはポート1420で実行
- すべてのメタデータ操作はデータベースのみ、またはデータベース+ファイルの両方を対象にできる（2つの別々のコマンド）

## 開発のベストプラクティス

- 新機能追加時は既存の状態管理パターンに従うこと
- エラーメッセージは常に日本語でユーザーフレンドリーに
- データベース操作は必ずトランザクション内で実行
- パフォーマンスに影響する変更はバッチ処理を検討
- セキュリティ関連の変更は慎重に review
- コミット前に`npm run check`と`cargo test`を実行
