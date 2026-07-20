# 実装方針: 技術スタック・ディレクトリ・規約・テスト

## 技術スタック

| 層               | 技術                            | バージョン（2026-02-27時点）                        | 備考                             |
| ---------------- | ------------------------------- | --------------------------------------------------- | -------------------------------- |
| フロントエンド   | SvelteKit + Svelte + TypeScript | `@sveltejs/kit` 2.49.x / `svelte` 5.46.x / TS 5.9.x | SPA構成（adapter-static）        |
| UIスタイル       | TailwindCSS + DaisyUI           | Tailwind 4.1.x / DaisyUI 5.5.x                      | `@apply`運用に制限あり           |
| データ取得       | TanStack Query（Svelte）        | 6.0.x                                               | Queryキャッシュ/再取得制御       |
| デスクトップ基盤 | Tauri                           | 2.x                                                 | `invoke()`でRustコマンド呼び出し |
| バックエンド     | Rust                            | edition 2021（stable）                              | コアロジック/DBアクセス          |
| DB               | SQLite + FTS5                   | rusqlite 0.38（bundled）                            | 全文検索・ローカル保存           |
| メタデータ       | lofty                           | 0.24                                                | タグ読み書き/アルバムアート抽出  |

## ディレクトリ構成

```text
src/
├── routes/
│   ├── (app)/
│   │   ├── library/
│   │   └── playlists/
│   └── settings/
└── lib/
    ├── components/
    │   ├── ui/
    │   └── library/
    ├── queries/
    ├── stores/
    ├── types/
    └── utils/

src-tauri/src/
├── commands/
│   ├── import.rs
│   ├── metadata_cmd.rs
│   ├── player.rs
│   ├── playlist_cmd.rs
│   ├── stats.rs
│   ├── system.rs
│   └── tracks.rs
├── lib.rs
├── db.rs
├── repository.rs
├── library.rs
├── playlist.rs
├── metadata.rs
├── models.rs
├── validation.rs
├── error.rs
└── state.rs
```

## 実装規約

### 命名

- Svelteコンポーネント: PascalCase（例: `Player.svelte`）
- TypeScript関数/変数: camelCase
- Rust関数/変数: snake_case
- 型名: PascalCase
- 定数: UPPER_SNAKE_CASE

### Svelte 5

- Runes構文（`$props`, `$state`, `$derived`, `$effect`）を使用
- UIローカル状態は`stores`に集約し、データ取得状態はQueryに分離する

### TailwindCSS

- カスタムクラスを`@apply`で適用しない
- コンポーネントの`<style>`先頭に、対象ファイルから`src/app.css`への相対パスで`@reference`を記述する

### VS Codeワークスペース運用

- `muspice.code-workspace` は `root` / `docs` / `tauri` の3ルート構成とする
- `.vscode/settings.json` の `files.exclude.docs = true` は appルート内での重複表示を避けるために維持し、編集は workspace の各ルートから行う

### エラーハンドリング

- Rustコマンドは `Result<T, String>` で日本語メッセージを返す
- フロントエンドでは `handleError` を必ず経由する

### 状態管理の使い分け

| 状態                           | 管理方式         |
| ------------------------------ | ---------------- |
| 再生状態・UI表示状態           | Svelte Stores    |
| トラック/プレイリスト/検索結果 | TanStack Query   |
| DB接続・現在トラックID         | Tauri `AppState` |

## 開発・品質コマンド

```bash
npm install
npm run tauri dev
npm run dev
npm run check
npm run lint
npm run format
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## テスト方針

- Rustユニットテストを `cargo test` で実行
- 変更前後で最低限以下を確認する
  - 型チェック（`npm run check`）
  - Lint（`npm run lint`）
  - Rust静的検査（`cargo clippy ... -D warnings`）
  - Rustテスト（`cargo test`）

## CI方針

`.github/workflows/ci.yml` で以下を実行:

1. Frontend Check（type-check, lint, format:check）
2. Backend Check（fmt --check, clippy, test）
3. Build Test（PR時のみ、Tauri build）

## 実装時のドキュメント同期ルール

- データモデル・API仕様変更: `docs/design/detailed-design.md`
- 技術スタック・構成・規約変更: `docs/design/implementation.md`
- 全体構成・データフロー変更: `docs/design/architecture.md`
- 非機能要件（パフォーマンス・セキュリティ・可用性）に関わる変更: `docs/design/non-functional.md`
- 代替案・トレードオフ: `docs/design/decisions.md`
