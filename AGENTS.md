# Repository Guidelines

## 優先順位と適用範囲

- このリポジトリの指示体系は**CLAUDE.mdを最上位ソース**とし、本ファイルはそれと整合する形での**リポジトリ共通の運用・開発ガイドライン**を定義する。
- より深い階層にAGENTS.mdがあれば、そちらの内容を優先する。
- 生成物ディレクトリ（`build/`, `src-tauri/target/`）はコミット対象外。
- Codex系エージェントは`CODEX.md`も参照し、CLAUDE.mdおよび本ファイルとの整合を保つこと（`CODEX.md`はCodex系エージェント向けの補助ガイドラインであり、CLAUDE.md/AGENTS.mdの要約ではない）。

## 言語・コミュニケーション

- すべて日本語で応答する。コードコメント・ユーザー向けエラーも日本語。ログの技術情報のみ英語可。
- コミットメッセージは英語で、Conventional Commits形式（例: `feat: add playlist deletion`）。

## プロジェクト構成

- `src/`: SvelteKitフロントエンド。`src/routes/`に画面、`src/lib/`に共通ロジック。
- `src-tauri/`: Tauri + Rustバックエンド。`src-tauri/src/`にRust、`src-tauri/tauri.conf.json`に設定。
- `static/`: アイコンなどの静的アセット。

## 開発・ビルド・テストコマンド

- `npm install`: フロントエンド依存関係の導入。
- `npm run tauri dev`: デスクトップアプリの開発モード（推奨）。
- `npm run dev`: フロントエンドのみ起動（Viteポート1420）。
- `npm run build`: フロントエンドビルド。`npm run tauri build`は本番アプリを生成。
- `npm run check`: SvelteKit同期と型チェック。
- `npm run lint` / `npm run lint:fix`: ESLint実行。
- `npm run format` / `npm run format:check`: Prettierフォーマット。
- `cd src-tauri && cargo fmt` / `cargo clippy -- -D warnings` / `cargo test`: Rustフォーマット・静的解析・テスト。

## コーディング規約

- 命名: SvelteはPascalCase、TypeScriptはcamelCase、Rustはsnake_case。型はPascalCase、定数はUPPER_SNAKE_CASE。
- コメント・ユーザー向けエラーメッセージは日本語。エラーはユーザーフレンドリーに。
- Tailwind: カスタムクラスを`@apply`で使わない。必要ならプロジェクト規約に従いスタイルブロック先頭で`@reference`を追加。
- Svelte 5のRunes構文を推奨。既存フォーマットに従いPrettier/ESLint/rustfmtを適用。

## テスト指針

- Rustは`cargo test`でサービス層・ユーティリティを検証（目標カバレッジ90%）。
- UIは手動テストを重視（インポート、再生、検索、メタデータ編集）。
- 追加テストは対象モジュール近くに配置し、内容が分かる名称にする。

## コミット・PRガイドライン

- コミットは英語のConventional Commits形式で簡潔に。
- PRの作成時は、CLAUDE.mdの「PRチェックリスト」セクションに従うこと。
- ブランチ戦略: `main`は安定版、作業は`feature/*`ブランチで行いPRを作成する。
- PR前の推奨チェック: `npm run format` / `npm run check` / `npm run lint`、`cd src-tauri && cargo fmt` / `cargo clippy -- -D warnings` / `cargo test`。

## セキュリティと構成

- Tauriのallowlistとファイルスコープを遵守し、ユーザー選択ディレクトリのみアクセス。
- DBとログはアプリ固有データディレクトリに保存（`{app_data_dir}/muspice.db`, `{app_data_dir}/logs`）。
