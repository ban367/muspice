# Muspice

## 言語設定

- すべての応答・コードコメント・エラーメッセージは日本語で記述
- **コミットメッセージは英語**（Conventional Commits形式: `feat:`, `fix:`, `refactor:` 等）
- 技術用語は不自然な日本語訳を避け英語併記可

## プロジェクト概要

Tauri 2 + SvelteKit で構築されたデスクトップ音楽管理アプリ。音楽ファイルのインポート・メタデータ管理・プレイリスト・再生・検索機能を提供。

## ディレクトリ構造

- `src/` - SvelteKitフロントエンド（`routes/`, `lib/components/`, `lib/queries/`, `lib/stores/`, `lib/types/`, `lib/utils/`）
- `src-tauri/` - Tauri + Rustバックエンド（`src/`配下に`commands.rs`, `db.rs`, `models.rs`, `library.rs`, `metadata.rs`, `playlist.rs`, `validation.rs`, `error.rs`, `state.rs`等）
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
- **DB**: SQLite + FTS5全文検索。スキーマは`tracks`, `playlists`, `playlist_tracks`, `tracks_fts`
- **エラー**: Rust側は`Result<T, String>`で日本語メッセージ返却。フロントは`handleError`で一元管理。トースト通知
- **命名**: Svelte=PascalCase、TypeScript=camelCase、Rust=snake_case。型=PascalCase、定数=UPPER_SNAKE_CASE
- **Svelte 5**: Runes構文（`$props()`, `$state()`, `$derived()`, `$effect()`）を使用
- **TailwindCSS**: カスタムクラスを`@apply`で使わない。スタイルブロック先頭に`@reference`を追加
- **セキュリティ**: Tauriのallowlistでアクセス制限。ローカルデータのみ。外部通信なし
- **パフォーマンス**: バッチインポート（50件/TX）、FTS5検索、DBインデックス、クエリ制限（1000件）、デバウンス（300ms）、仮想スクロール

## 詳細ドキュメント

- [docs/tech-stack.md](docs/tech-stack.md) - 言語設定、技術スタック、オーディオサポート
- [docs/architecture.md](docs/architecture.md) - フロントエンド/バックエンド構成、DB、データフロー
- [docs/features.md](docs/features.md) - 機能要件（インポート、メタデータ、プレイリスト、再生、検索）
- [docs/conventions.md](docs/conventions.md) - コードスタイル、Clippy/Tailwind/UIコンポーネント規約、Svelte 5パターン
- [docs/development.md](docs/development.md) - 開発コマンド詳細、テスト、CI/CD、Git、トラブルシューティング
