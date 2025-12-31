# CODEX.md

このリポジトリでCodex系エージェントが作業する際の指針です。最上位の基準は常に`CLAUDE.md`であり、内容に差異がある場合は`CLAUDE.md`を優先してください。ディレクトリ配下により深い階層の`AGENTS.md`がある場合はその指示を最優先とします。

## 言語・コミュニケーション

- すべて日本語で応答すること。
- コードコメント・ユーザー向けエラーメッセージは日本語。ログの技術情報は英語可。
- コミットメッセージは英語でConventional Commits形式（例: `feat: add playlist deletion`）。

## 開発フロー

1. 該当ディレクトリの`AGENTS.md`を確認し、CLAUDE.mdの方針を尊重する。
2. 作業前に生成物ディレクトリ（`build/`, `src-tauri/target/`）をコミット対象から除外する。
3. 変更時は以下のコマンドを参考に検証する:
   - `npm run check`
   - `npm run lint` / `npm run lint:fix`
   - `npm run format` / `npm run format:check`
   - `npm run build`
   - `cd src-tauri && cargo fmt`
   - `cd src-tauri && cargo clippy -- -D warnings`
   - `cd src-tauri && cargo test`
4. ブランチ戦略: `main`は安定版、作業は`feature/*`で行いPRを作成する。

## コーディング規約（抜粋）

- 命名: SvelteはPascalCase、TypeScriptはcamelCase、Rustはsnake_case。型はPascalCase、定数はUPPER_SNAKE_CASE。
- Tailwind: カスタムクラスを`@apply`で使わない。スタイルブロックでは`@reference`を先頭に追加。
- Svelte 5のRunes構文を推奨。既存フォーマットに従いPrettier/ESLint/rustfmtを適用。
- エラーはユーザーフレンドリーな日本語で返す。

## コミット・PRガイドライン

- PRには目的、検証手順、UI変更時のスクリーンショット、関連Issue（あれば）を含める。

## セキュリティと構成

- Tauriのallowlistとファイルスコープを遵守し、ユーザー選択ディレクトリのみアクセスする。
- DBとログはアプリ固有データディレクトリに保存（`{app_data_dir}/muspice.db`, `{app_data_dir}/logs`）。

## 追加メモ

- CLAUDE.mdが「現在のマスター」となるため、将来的にルールを更新する場合はまず`CLAUDE.md`に反映し、本ファイルや`AGENTS.md`はその要約として整合性を保つ。
