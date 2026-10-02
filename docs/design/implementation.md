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
├── hooks.client.ts        # dev:mock時のみTauri IPCモックを初期化
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
    ├── mocks/             # ブラウザ確認用のTauri IPCモック（dev:mock専用）
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

- コンポーネント・ページは `commands` を直接呼ばず、`$lib/queries` のクエリ・ミューテーションを経由する（ESLintの`no-restricted-imports`で禁止）。キャッシュの無効化（`onSuccess`）とエラーのトースト通知（`withErrorToast`）をここに集約するため
  - キャッシュに影響しない操作（例: ファイルの場所を開く）も、失敗を通知するためミューテーションとして定義する
  - 画面内にエラーを表示する場合は`String(error)`ではなく`toErrorMessage(error)`を使う（`AppError`はオブジェクトのため"[object Object]"になる）
  - 再生制御（`getTrackFilePath`・`setCurrentTrack`）はクエリではなく`$lib/stores/playback`の再生コントローラーが呼ぶ
- アルバムアートのキャッシュは`$lib/stores/albumArtCache`に一本化する（一覧・詳細・プレイヤーで共有する）

### 再生制御

- audio要素の操作・キュー遷移・リピート・再生回数の記録・イコライザの接続は`$lib/stores/playback`の`createPlaybackController(audio)`が担う。`Player.svelte`は表示と操作の受付だけを行い、コントローラーのメソッドを呼ぶ
- 次・前のトラックの決定は`$lib/stores/player`のキュー操作（`playNextTrack`・`playPreviousTrack`）が担う。キュー操作の結果が再生中と同じトラックだった場合（1曲リピート、3秒以上再生中の「前へ」、1曲だけのキューの全曲リピート）はトラックIDが変わらず読み込みが走らないため、コントローラーが頭から再生し直す
- 再生中かどうか（`isPlaying`）はaudio要素の`play`/`pause`イベントから更新する
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
npm run dev:mock
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

## ブラウザでの動作確認（IPCモック）

Tauriのウィンドウ（macOSではWKWebView）はブラウザ自動化ツールから操作できないため、Claude Code等でのUI確認はTauri IPCをモックしたブラウザで行う（判断の経緯は`decisions.md`のADR-006）。

- 起動: `npm run dev:mock`（Viteの`mock`モード・ポート1430）。Claude Codeでは`.claude/launch.json`の`web-mock`でプレビューを起動する
- 仕組み: `src/hooks.client.ts`が`mock`モードのときだけ`$lib/mocks/tauri`を読み込み、`@tauri-apps/api/mocks`で`window.__TAURI_INTERNALS__`を差し替える。`tauri dev`・本番ビルドではバンドルに含まれない
- `src/lib/mocks/`の構成:
  - `backend.ts`: `commands`の全コマンドをメモリ上で再現するバックエンド。ハンドラ表の型を`bindings.ts`から導出しているため、Rust側でコマンドを追加・変更したら型エラーに従ってここも更新する
  - `fixtures.ts`: 初期データ。状態はメモリ上のみで、リロードすると初期状態に戻る
  - `media.ts`: アルバムアート（SVG）と再生用トーン（20秒のWAV）の生成
  - `tauri.ts`: event・dialog・windowプラグインと`convertFileSrc`の差し替え
- ネイティブメニューのイベントや確認ダイアログの回答は、開発者ツールから`window.__MUSPICE_MOCK__`で操作する
  - `window.__MUSPICE_MOCK__.emit('open-import-dialog')`（`toggle-sidebar` / `show-about-dialog`も同様）
  - `window.__MUSPICE_MOCK__.setConfirmResult(false)`で、以降の確認ダイアログを「キャンセル」にする
- 確認できないもの: Rust側の処理（SQLite・FTS5・ファイルI/O・タグ読み書き）、実ファイルの再生、CSP・capabilityによる制約。これらは`cargo test`と`npm run tauri dev`で確認する

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
