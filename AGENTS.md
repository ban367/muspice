# Muspice

## 言語設定

- すべての応答・コードコメント・エラーメッセージは日本語で記述
- **コミットメッセージは英語**（Conventional Commits形式: `feat:`, `fix:`, `refactor:` 等）
- 技術用語は不自然な日本語訳を避け英語併記可

## プロジェクト概要

Tauri 2 + SvelteKit で構築されたデスクトップ音楽管理アプリ。音楽ファイルのインポート・メタデータ管理・プレイリスト・再生・検索機能を提供。

## ディレクトリ構造

- `src/` - SvelteKitフロントエンド（`routes/`, `lib/components/`, `lib/queries/`, `lib/stores/`, `lib/types/`, `lib/utils/`）
- `src-tauri/` - Tauri + Rustバックエンド（`src/commands/`, `db.rs`, `repository.rs`, `models.rs`, `library.rs`, `metadata.rs`, `playlist.rs`, `validation.rs`, `logger.rs`, `state.rs` 等）
- `static/` - 静的アセット
- `docs/` - 詳細ドキュメント

## 開発コマンド

```bash
npm install                   # フロントエンド依存関係
npm run tauri dev             # 開発モード（推奨）
npm run dev                   # フロントエンドのみ（ポート1420）
npm run check                 # TypeScript型チェック
npm run lint                  # ESLint
npm run format                # Prettierフォーマット
cd src-tauri && cargo test    # Rustテスト
cd src-tauri && cargo fmt     # Rustフォーマット
cd src-tauri && cargo clippy -- -D warnings  # Clippy
npm run tauri build           # 本番ビルド
```

## 設計方針

- **状態管理**: Svelte Stores（UI状態）+ TanStack Query（データキャッシング）+ Tauri State（バックエンド永続化）
- **DB**: SQLite + FTS5全文検索。スキーマは`tracks`, `playlists`, `playlist_tracks`, `play_history`, `tracks_fts`。SQLは`repository.rs`（トラック）と`playlist.rs`（プレイリスト）に集約し、コマンド層は`AppState::with_db`経由でアクセスする
- **エラー**: Rust側は`AppResult<T>`（`AppError`）で`{code, message}`を返却（messageは日本語）。フロントは`handleError`でcodeベースに分類し一元管理。トースト通知
- **命名**: Svelte=PascalCase、TypeScript=camelCase、Rust=snake_case。型=PascalCase、定数=UPPER_SNAKE_CASE
- **Svelte 5**: Runes構文（`$props()`, `$state()`, `$derived()`, `$effect()`）を使用
- **TailwindCSS**: カスタムクラスを`@apply`で使わない。スタイルブロック先頭に`@reference`を追加
- **セキュリティ**: Tauriのallowlistでアクセス制限。ローカルデータのみ。外部通信なし
- **パフォーマンス**: バッチインポート（50件/TX）、FTS5検索、DBインデックス、クエリ制限（1000件）、デバウンス（300ms）、仮想スクロール

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
