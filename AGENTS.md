# Repository Guidelines

## プロジェクト構成

- `src/`: SvelteKitフロントエンド。`src/routes/`に画面、`src/lib/`に共通ロジック。
- `src-tauri/`: Tauri + Rustバックエンド。`src-tauri/src/`にRust、`src-tauri/tauri.conf.json`に設定。
- `static/`: アイコンなどの静的アセット。
- 生成物は`build/`と`src-tauri/target/`。コミット対象外。

## ビルド・テスト・開発コマンド

- `npm install`: フロントエンド依存関係の導入。
- `npm run tauri dev`: デスクトップアプリの開発モード（推奨）。
- `npm run dev`: フロントエンドのみ起動（Viteポート1420）。
- `npm run build`: フロントエンドビルド。`npm run tauri build`は本番アプリを生成。
- `npm run check`: SvelteKit同期と型チェック。
- `npm run lint` / `npm run lint:fix`: ESLint実行。
- `npm run format` / `npm run format:check`: Prettierフォーマット。
- `cd src-tauri && cargo test`: Rustユニットテスト。

## コーディング規約

- コメントとユーザー向けエラーメッセージは日本語。ログの技術情報は英語可。
- 命名: SvelteはPascalCase、TypeScriptはcamelCase、Rustはsnake_case。型はPascalCase、定数はUPPER_SNAKE_CASE。
- 既存フォーマットに従い、Prettier/ESLint/rustfmtを適用。
- Markdownは一般的なルールに従って整形する（見出し階層、リストのインデント、空行の使い分け）。

## テスト指針

- Rustは`cargo test`でサービス層・ユーティリティを検証（目標カバレッジ90%）。
- UIは手動テストを重視（インポート、再生、検索、メタデータ編集）。
- 追加テストは対象モジュール近くに配置し、内容が分かる名称にする。

## コミット・PRガイドライン

- コミットメッセージは英語で簡潔に（例: `feat: add playlist deletion`）。
- PRには目的、検証手順、UI変更時のスクリーンショットを含める。
- 関連Issueがあればリンク。

## セキュリティと構成の注意

- Tauriのallowlistとファイルスコープを遵守し、ユーザー選択ディレクトリのみアクセス。
- DBとログはアプリ固有データディレクトリに保存（`{app_data_dir}/muspice.db`, `{app_data_dir}/logs`）。
- 変更時は`npm run check`と`cargo test`の実行を推奨。
