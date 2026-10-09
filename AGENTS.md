# Muspice

## 言語設定

- すべての応答・コードコメント・エラーメッセージは日本語で記述
- **コミットメッセージは英語**（Conventional Commits形式: `feat:`, `fix:`, `refactor:` 等）
- 技術用語は不自然な日本語訳を避け英語併記可

## プロジェクト概要

Tauri 2 + SvelteKit で構築されたデスクトップ音楽管理アプリ。音楽ファイルのインポート・メタデータ管理・プレイリスト・再生・検索機能を提供。

## ディレクトリ構造

- `src/` - SvelteKitフロントエンド（`routes/`, `lib/components/`, `lib/queries/`, `lib/stores/`, `lib/types/`, `lib/utils/`, `lib/i18n/`, `lib/mocks/`）
- `src-tauri/` - Tauri + Rustバックエンド（`src/commands/`, `db.rs`, `repository.rs`, `models.rs`, `library.rs`, `library_folder.rs`, `metadata.rs`, `playlist.rs`, `playback/`（再生エンジン）, `validation.rs`, `settings.rs`, `state.rs`, `menu.rs` 等）
- `static/` - 静的アセット
- `docs/` - 詳細ドキュメント

## 開発コマンド

```bash
npm install                   # フロントエンド依存関係
npm run tauri dev             # 開発モード（推奨）
npm run dev                   # フロントエンドのみ（ポート1420）
npm run dev:mock              # ブラウザ確認用（Tauri IPCをモック、ポート1430）
npm run check                 # TypeScript型チェック
npm run lint                  # ESLint
npm test                      # Vitest（フロントエンドの単体テスト）
npm run format                # Prettierフォーマット
cd src-tauri && cargo test    # Rustテスト
cd src-tauri && cargo fmt     # Rustフォーマット
cd src-tauri && cargo clippy -- -D warnings  # Clippy
npm run tauri build           # 本番ビルド
```

## 設計方針

- **状態管理**: Svelteの共有状態（UI状態。`src/lib/stores/*.svelte.ts`のRunesのモジュール）+ TanStack Query（データキャッシング）+ Tauri State（バックエンド永続化）
- **DB**: SQLite + FTS5全文検索。スキーマは`tracks`, `playlists`, `playlist_tracks`, `play_history`, `library_folders`, `sync_devices`, `sync_device_playlists`, `tracks_fts`。SQLは`repository.rs`（トラック）・`playlist.rs`（プレイリスト）・`library_folder.rs`（ライブラリフォルダ）・`device.rs`（転送先デバイス）に集約し、コマンド層は`AppState::with_db`経由でアクセスする
- **エラー**: Rust側は`AppResult<T>`（`AppError`）で`{code, message}`を返却（messageは日本語）。フロントは`handleError`でcodeベースに分類し一元管理。トースト通知
- **再生**: 再生はRust側の再生エンジン（`src-tauri/src/playback/`。symphonia + cpal）で行う。フロントは再生キューを持ち、再生コントローラー（`src/lib/stores/playback.svelte.ts`）がコマンドでエンジンを操作して、通知（`PlaybackEvent`）を再生状態へ反映する。WebViewは音声ファイルを読まない（audio要素・Web Audioは使わない）
- **型共有**: Rust⇔TSの型とコマンド呼び出しはtauri-spectaが`src/lib/bindings.ts`へ自動生成（`npm run tauri dev`または`cargo test export_typescript_bindings`）。手動で編集せず、Rust側を変更して再生成する。フロントは`commands.xxx()`経由で呼び出す
- **命名**: Svelte=PascalCase、TypeScript=camelCase、Rust=snake_case。型=PascalCase、定数=UPPER_SNAKE_CASE
- **Svelte 5**: Runes構文（`$props()`, `$state()`, `$derived()`, `$effect()`）を使用
- **TailwindCSS**: カスタムクラスを`@apply`で使わない。スタイルブロック先頭に`@reference`を追加
- **セキュリティ**: WebViewの権限（capability）は最小限にし（assetプロトコルは無効）、ファイルアクセスはRust側のコマンドで行う。WebViewから任意のパスを受け取らずトラックID等で解決する。ローカルデータのみ。外部通信なし
- **動作確認**: Claude CodeでのUI確認は`npm run dev:mock`（`.claude/launch.json`の`web-mock`）でTauri IPCをモックしたブラウザを使う。Rustのコマンドを変更したら`src/lib/mocks/backend.ts`も追随させる（詳細は`docs/design/implementation.md`）
- **パフォーマンス**: バッチインポート（50件/TX）、FTS5検索、DBインデックス、デバウンス（300ms）。一覧は件数の上限なく全件を取得し、`VirtualList`で見えている行だけを描画する（仮想スクロール。詳細は`docs/design/implementation.md`）

## ドキュメント参照ルール

- エントリポイントは `docs/design-doc.md`（ドキュメント構成表あり）
- 実装タスクでは以下を優先参照する:
  - `docs/design/detailed-design.md` - データモデル・API仕様
  - `docs/design/implementation.md` - ファイル配置・コーディング規約
- アーキテクチャ全体の確認が必要な場合は `docs/design/architecture.md` を参照する
- 機能の背景・スコープを確認する場合のみ `docs/design/overview.md` を参照する
- 設計の意図・判断・制約が変わった場合は、実装と同時に該当ドキュメントを更新する:
  - データモデル・APIの変更 → `docs/design/detailed-design.md`
  - ディレクトリ構成・技術スタック・規約の変更 → `docs/design/implementation.md`
  - コンポーネント構成・データフローの変更 → `docs/design/architecture.md`
  - 採用しなかった代替案・トレードオフ → `docs/design/decisions.md`
- ドキュメントと実装の乖離を発見した場合は、ドキュメントを実態に合わせて修正する
