# 実装方針: 技術スタック・ディレクトリ・規約・テスト

## 技術スタック

| 層               | 技術                            | バージョン（2026-10-01時点）                        | 備考                                                 |
| ---------------- | ------------------------------- | --------------------------------------------------- | ---------------------------------------------------- |
| フロントエンド   | SvelteKit + Svelte + TypeScript | `@sveltejs/kit` 2.70.x / `svelte` 5.57.x / TS 6.0.x | SPA構成（adapter-static）                            |
| ビルド           | Vite                            | 8.3.x                                               | Tailwindは`@tailwindcss/vite`経由（PostCSS設定なし） |
| テスト           | Vitest                          | 5.0.x                                               | ストア・ユーティリティの単体テスト                   |
| UIスタイル       | TailwindCSS + DaisyUI           | Tailwind 4.3.x / DaisyUI 5.7.x                      | `@apply`運用に制限あり                               |
| データ取得       | TanStack Query（Svelte）        | 6.3.x                                               | Queryキャッシュ/再取得制御                           |
| デスクトップ基盤 | Tauri + tauri-specta            | 2.12.x / 2.0.0-rc.25                                | 型付きコマンド呼び出しを自動生成                     |
| バックエンド     | Rust                            | edition 2021（stable）                              | コアロジック/DBアクセス                              |
| DB               | SQLite + FTS5                   | rusqlite 0.40（bundled）                            | 全文検索・ローカル保存                               |
| メタデータ       | lofty                           | 0.25                                                | タグ読み書き/アルバムアート抽出                      |

## ディレクトリ構成

```text
src/
├── routes/
│   ├── (app)/
│   │   ├── library/
│   │   └── playlists/
│   └── settings/
└── lib/
    ├── bindings.ts        # tauri-spectaによる生成物（編集禁止）
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
├── error.rs
├── events.rs
├── repository.rs
├── library.rs
├── playlist.rs
├── metadata.rs
├── models.rs
├── validation.rs
├── logger.rs
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
- 複数コンポーネントで使うSvelteアクション（`use:`）は`$lib/utils/actions.ts`に置く

### TanStack Query

- クエリキーは`src/lib/queries/keys.ts`の`queryKeys`に集約する。クエリ定義・無効化のどちらもここを参照し、`['tracks']`のようなマジック配列を直接書かない
- 無効化はプレフィックス一致で波及するため、キーの階層がそのまま無効化の粒度になる（例: `queryKeys.tracks.all`の無効化は検索・フィルタ・お気に入りにも及ぶ）

### TailwindCSS

- カスタムクラスを`@apply`で適用しない
- コンポーネントの`<style>`先頭に、対象ファイルから`src/app.css`への相対パスで`@reference`を記述する

### VS Codeワークスペース運用

- `muspice.code-workspace` は `root` / `docs` / `tauri` の3ルート構成とする
- `.vscode/settings.json` の `files.exclude.docs = true` は appルート内での重複表示を避けるために維持し、編集は workspace の各ルートから行う

### エラーハンドリング

- Rustコマンドは `AppResult<T>`（`error.rs` の `AppError`）を返す。エラーは `{ code, message }` 形式でシリアライズされ、messageは日本語のユーザー向け文言とする
- エラーコード: `LOCK` / `DATABASE` / `NOT_FOUND` / `VALIDATION` / `IO` / `METADATA`
- フロントエンドでは `handleError` を必ず経由し、codeでエラーを分類する（部分文字列マッチは行わない）
- DBアクセスはコマンド層で `AppState::with_db` を経由し、ロック取得エラーの処理を一元化する
- ファイルI/O・タグ解析・大量のDB書き込みなど重い同期処理は `commands::run_blocking` で実行する（asyncコマンド内で直接行うと非同期ランタイムのワーカーを占有する）。状態が必要な場合は `AppHandle` を受け取り、クロージャ内で `app.state::<AppState>()` から取得する
- トラック関連のSQLは `repository.rs`、プレイリスト関連のSQLは `playlist.rs` に集約する（コマンド層に生SQLを書かない）
- ログは `crate::logger`（`logger.rs`）を使用する（`log` クレートは未初期化のため使用しない）

### 型共有（tauri-specta）

- Rust⇔TypeScriptの型とコマンド呼び出しは tauri-specta が `src/lib/bindings.ts` に自動生成する（生成物のためlint/formatの対象外・手動編集禁止）
- 生成タイミング: デバッグビルド起動時（`npm run tauri dev`）、または `cargo test export_typescript_bindings`
- 新しいコマンドを追加する手順:
  1. コマンド関数に `#[tauri::command]` と `#[specta::specta]` を付与する
  2. 引数・戻り値の型に `specta::Type` を derive する
  3. `lib.rs` の `specta_builder()` 内 `collect_commands![]` に追加する（`invoke_handler`は自動で追随する）
- フロントエンドは `invoke()` を直接使わず `commands.xxx()` を使う（コマンド名・引数・戻り値が型チェックされる）
- バックエンド→フロントエンドのイベントを追加する手順:
  1. `events.rs` に `#[derive(Serialize, Type, Event)]` の型を定義する（イベント名は型名のケバブケース）
  2. `lib.rs` の `collect_events![]` に追加する（未登録のまま `emit` するとパニックする）
  3. Rust側は `イベント.emit(&app_handle)`、フロントは `events.xxx.listen()` を使う（`@tauri-apps/api/event` の `listen` を文字列で直接呼ばない）
- 型定義は `src/lib/types/models.ts` が `bindings.ts` を再エクスポートする。TS側で手書きの重複定義を作らない
- `i64`/`usize` はTypeScriptへ直接エクスポートできない（精度損失防止）。件数は `u32`、`i64` は `#[specta(type = specta_typescript::Number)]` で明示する
- 省略可能な入力（`Option<T>`）は `#[specta(optional)]` を付け、TS側で `field?: T | null` として扱えるようにする

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
npm test
npm run format
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## テスト方針

- Rustユニットテストを `cargo test` で実行
- フロントエンドのロジック（ストア・ユーティリティ）は Vitest で単体テストする（`npm test`）
  - テストは対象と同じディレクトリに `*.test.ts` として置き、Node環境で実行する
  - コンポーネント内の判定ロジックはテストしやすいよう `$lib/utils` の純粋関数へ切り出す（例: `selection.ts`）
- 変更前後で最低限以下を確認する
  - 型チェック（`npm run check`）: 警告も失敗扱い。Tailwindの`@apply`/`@reference`をCSS言語サービスが解釈できず誤警告になるため、CSS診断は対象外（`--diagnostic-sources js,svelte`）
  - Lint（`npm run lint`）: 警告も失敗扱い（`--max-warnings 0`）
  - フロントエンドテスト（`npm test`）
  - Rust静的検査（`cargo clippy ... -D warnings`）
  - Rustテスト（`cargo test`）

## CI方針

`.github/workflows/ci.yml` で以下を実行:

1. Frontend Check（type-check, lint, format:check, test）
2. Backend Check（fmt --check, clippy, test）
3. Build Test（PR時のみ、Tauri build）

`.github/workflows/audit.yml` で依存関係の脆弱性を検査する（毎週・lockfile変更PR時。詳細は `non-functional.md`）。

## 実装時のドキュメント同期ルール

- データモデル・API仕様変更: `docs/design/detailed-design.md`
- 技術スタック・構成・規約変更: `docs/design/implementation.md`
- 全体構成・データフロー変更: `docs/design/architecture.md`
- 非機能要件（パフォーマンス・セキュリティ・可用性）に関わる変更: `docs/design/non-functional.md`
- 代替案・トレードオフ: `docs/design/decisions.md`
