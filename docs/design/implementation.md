# 実装方針: 技術スタック・ディレクトリ・規約・テスト

## 技術スタック

| 層               | 技術                            | バージョン（2026-10-03時点）                       | 備考                                                 |
| ---------------- | ------------------------------- | -------------------------------------------------- | ---------------------------------------------------- |
| フロントエンド   | SvelteKit + Svelte + TypeScript | `@sveltejs/kit` 3.0.x / `svelte` 5.57.x / TS 6.0.x | SPA構成（adapter-static）                            |
| ビルド           | Vite                            | 8.3.x                                              | Tailwindは`@tailwindcss/vite`経由（PostCSS設定なし） |
| テスト           | Vitest                          | 5.0.x                                              | ストア・ユーティリティの単体テスト                   |
| UIスタイル       | TailwindCSS + DaisyUI           | Tailwind 4.3.x / DaisyUI 5.7.x                     | `@apply`運用に制限あり                               |
| データ取得       | TanStack Query（Svelte）        | 6.3.x                                              | Queryキャッシュ/再取得制御                           |
| デスクトップ基盤 | Tauri + tauri-specta            | 2.12.x / 2.0.0-rc.25                               | 型付きコマンド呼び出しを自動生成                     |
| バックエンド     | Rust                            | edition 2024（stable）                             | コアロジック/DBアクセス                              |
| DB               | SQLite + FTS5                   | rusqlite 0.40（bundled）                           | 全文検索・ローカル保存                               |
| メタデータ       | lofty                           | 0.25                                               | タグ読み書き/アルバムアート抽出                      |
| フォルダの監視   | notify-debouncer-mini（notify） | 0.7（notify 8）                                    | ライブラリフォルダの変更の自動反映                   |

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
│   ├── library_folders.rs
│   ├── metadata_cmd.rs
│   ├── player.rs
│   ├── playlist_cmd.rs
│   ├── settings.rs
│   ├── stats.rs
│   ├── system.rs
│   └── tracks.rs
├── lib.rs
├── album_art.rs
├── db.rs
├── error.rs
├── events.rs
├── repository.rs
├── library.rs
├── library_folder.rs
├── library_sync.rs        # ライブラリフォルダの変更の自動反映（起動時・定期・監視）
├── playlist.rs
├── metadata.rs
├── models.rs
├── settings.rs
├── validation.rs
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
- 複数コンポーネントで使うSvelteアクション（`use:`）は`#lib/utils`に置く

### 共有する状態（Runesのモジュール）

- コンポーネントをまたいで共有する状態は`src/lib/stores/*.svelte.ts`に、`$state`のフィールドを持つクラスのインスタンスとして置く（例: `ui.svelte.ts`の`ui`、`error.svelte.ts`の`notifications`、`equalizer.svelte.ts`の`equalizer`、`player.svelte.ts`の`player`）。ユーティリティに付随する状態は、そのユーティリティの`*.svelte.ts`に置く（例: `#lib/utils/dialog.svelte.ts`のテキスト入力の要求`textPrompt`）。`svelte/store`（`writable`など）は使わない（ESLintの`no-restricted-imports`で禁止）
- 読み書きはプロパティを直接使う（例: `ui.isSidebarOpen = false`）。`$`接頭辞や`get()`は不要で、コンポーネント外の`.ts`からも同じように読める
- 配列・オブジェクトを丸ごと置き換える状態は`$state.raw`にし、中身を書き換えず代入で更新する（例: `notifications.items`、`ui.columnWidths`）
- localStorageに保存する状態は、privateな`$state`とgetter/setterで実装し、setterで保存する（例: `ui.isRightSidebarPinned`）。保存値はモジュールの読み込み時に読み、localStorageが使えない環境でも例外にしない
- 値から計算できるものは`$derived`のフィールドにする（例: `player.progress`・`player.upcomingTracks`）
- 状態に付随する操作（追加・削除など）はクラスのメソッドか、モジュールの関数にする（例: `notifications.add()`、キュー操作の`playTrackFromQueue()`）。保存や外部への反映（例: イコライザのゲインをWeb Audioのノードへ）が必要な状態は、getterだけを公開してメソッド経由で変更させる（例: `equalizer.setBandGain()`）
- 状態の変化を受け取る処理は`$effect`で書く。コンポーネントの外（例: 再生コントローラー）では`$effect.root`で作り、破棄時に止める。`$effect`は変更の直後ではなくマイクロタスクで実行されるため、同期的な反映を前提にしない（テストでは`flushSync()`で反映させる）

### SvelteKit 3

- 設定は`vite.config.js`の`sveltekit({...})`に渡す（`svelte.config.js`は使えない）。`tsconfig.json`は`svelte-kit sync`が生成する`$app/tsconfig`を継承する
- `src/lib`は`$lib`ではなく`#lib`（`package.json`の`imports`）で参照し、拡張子を付ける。`.ts`のモジュールは`.js`（例: `#lib/utils/format.js`、`.svelte.ts`は`#lib/stores/ui.svelte.js`）、`index.ts`は`/index.js`（例: `#lib/components/ui/index.js`）、`.svelte`はそのまま
- ページの状態は`$app/state`の`page`を使う（`$app/stores`は削除された）
- `resolve()`（`$app/paths`）に渡すパスは先頭に`/`を付けない（例: `resolve('library/songs')`）。先頭が`/`の文字列はルートIDとして扱われ、`(...)`を含むセグメントがルートグループとして消えるため、ジャンル名などを含むパスは必ずパスとして渡す
- `goto()`はアプリのルートに一致しないURLで拒否（reject）する。保存したパスなど、存在しない可能性がある遷移先は`catch`で代わりの画面へ移動する

### TanStack Query

- コンポーネント・ページは `commands` を直接呼ばず、`#lib/queries` のクエリ・ミューテーションを経由する（ESLintの`no-restricted-imports`で禁止）。キャッシュの無効化（`onSuccess`）とエラーのトースト通知（`withErrorToast`）をここに集約するため
  - キャッシュに影響しない操作（例: ファイルの場所を開く）も、失敗を通知するためミューテーションとして定義する
  - 画面内にエラーを表示する場合は`String(error)`ではなく`toErrorMessage(error)`を使う（`AppError`はオブジェクトのため"[object Object]"になる）
  - 再生制御（`getTrackFilePath`・`setCurrentTrack`）はクエリではなく`#lib/stores/playback.svelte`の再生コントローラーが呼ぶ
- アルバムアートは`#lib/utils/albumArt`の`albumArtUrl(trackId)`をそのまま`<img>`（`AlbumArt`コンポーネント）に渡す。画像データをフロントエンドで取得・保持しない。アートがない場合は読み込みエラーになり、`AlbumArt`がプレースホルダーを表示する
- クエリキーは`src/lib/queries/keys.ts`の`queryKeys`に集約する。クエリ定義・無効化のどちらもここを参照し、`['tracks']`のようなマジック配列を直接書かない
- 無効化はプレフィックス一致で波及するため、キーの階層がそのまま無効化の粒度になる（例: `queryKeys.tracks.all`の無効化は検索・フィルタ・お気に入りにも及ぶ）

### 再生制御

- audio要素の操作・キュー遷移・リピート・再生回数の記録・イコライザの接続・ギャップレス再生は`#lib/stores/playback.svelte`の`createPlaybackController([audio, standbyAudio], options)`が担う。`Player.svelte`は表示と操作の受付だけを行い、コントローラーのメソッドを呼ぶ
- 次・前のトラックの決定は`#lib/stores/player.svelte`のキュー操作（`playNextTrack`・`playPreviousTrack`）が担う。キュー操作の結果が再生中と同じトラックだった場合（1曲リピート、3秒以上再生中の「前へ」、1曲だけのキューの全曲リピート）はトラックIDが変わらず読み込みが走らないため、コントローラーが頭から再生し直す
- 再生中かどうか（`isPlaying`）はaudio要素の`play`/`pause`イベントから更新する
- ギャップレス再生では、audio要素（デッキ）を2つ使い、再生中のデッキ（`active`）と先読みのデッキ（`standby`）を切り替える
  - 先読みするトラックは`#lib/stores/player.svelte`の`peekNextTrack()`（`playNextTrack`の進む先を、状態を変えずに返す）で決める。キュー・リピート・シャッフル・設定が変わると`$effect`で先読みし直す
  - 曲の終わりの1秒前から、残り時間に合わせたタイマーで、終わりの少し前（`GAPLESS_LEAD_SECONDS`）に先読みしたデッキの再生を始め、`playNextTrack()`で再生中のトラックを進める。前の曲は止めずに最後まで鳴らし、鳴り終わってから空いたデッキに次の曲を先読みする
  - タイマーに間に合わなかった場合（`ended`が先に来た場合）も、先読みが済んでいればそのデッキで続ける。先読みが済んでいない・次の曲と一致しない場合は、従来どおり読み込む
  - 「次へ」などで先読みしたトラックへ移った場合も、読み込み直さずにそのデッキへ切り替える
  - イベントは両方のデッキから受け、再生状態（`isPlaying`・再生位置・長さ）には再生中のデッキのものだけを反映する。先読みのエラーは表示せず、切り替えのときの通常の読み込みで改めて扱う
  - 同じ曲を繰り返す切り替え（1曲リピート）は、従来の頭からの再生し直しと同じく、再生回数に数えない
- クロスフェードは、ギャップレス再生の切り替えを曲の終わりの設定した秒数前に早め、`equalizer.svelte`の`fadeDeck`で前の曲のデッキをフェードアウト、次の曲のデッキをフェードインさせる（等パワーの曲線）
  - 秒数が1以上なら、ギャップレス再生の設定にかかわらず先読みする
  - 秒数は、どちらの曲も長さの半分までにする。同じ曲の繰り返し（1曲リピート）・`ended`での切り替え・手動の操作（「次へ」・曲の選択）・Web Audioの経路を作れていない場合はクロスフェードしない
  - クロスフェードの途中で一時停止・シーク・曲の選択などをしたら、前の曲を止めて再生中の曲を通常の音量に戻す（`endCrossfade`）
  - デッキが再生中になるときは、フェードの音量を必ず決め直す（フェードインか1）。前の曲として音量を0まで下げたデッキを、そのまま使わないため
- 音量の正規化は、再生中のトラックの`replayGain`と設定の`volumeNormalization`から`#lib/utils/normalization`の`normalizationGain`で倍率を求め、イコライザの前段のGainNodeへ`setNormalizationGain`で反映する。コントローラーは設定を直接読まず、`options`の関数（`normalizationMode`・`gapless`・`crossfadeSeconds`）で受け取る（テストで差し替えるため）。補正はデッキごとにかけ、各デッキが読み込んだトラックの値を使う。audio要素の`volume`はユーザーの音量のまま変えない
- アルバム・プレイリストなどの「シャッフル再生」は`playShuffled(tracks)`を使う（配列を`sort(() => Math.random() - 0.5)`などで独自に並べ替えない）。シャッフルモードを有効にし、元の順序を保持するため、解除すると元の順序に戻る

### ダイアログ

- モーダルは`#lib/components/ui`の`Modal`（ネイティブの`<dialog>`を`showModal()`で表示）を使い、背景のdivや`svelte-ignore`で独自に実装しない。Escキー・背面の操作の無効化（フォーカスの閉じ込め）・閉じた後のフォーカスの復帰はブラウザに任せる
  - 表示状態は呼び出し側が持ち、閉じる操作で呼ばれる`onClose`で`open`をfalseにする。処理中は`dismissible={false}`で閉じさせない
  - 最大幅は`class`（例: `max-w-md`）で指定する。最初にフォーカスする要素には`data-autofocus`を付ける
- `window.confirm` / `window.prompt`は使わない（ESLintで禁止）。確認は`#lib/utils/dialog`の`confirmDestructive`、テキスト入力は`promptText`を`await`する
  - `confirm`はdialogプラグインにより非同期化されており、同期的に呼ぶと常にtrue扱いになる
  - `prompt`はmacOSのWebView（wry）が実装しておらず、常にnullを返す
- コンテキストメニューからダイアログを開く場合は、使う値を取り出してからメニューを閉じ、その後で`await`する（ダイアログの操作でメニューのコンポーネントが破棄され、`await`後にpropsを読むと失敗するため）

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
- トラック関連のSQLは `repository.rs`、プレイリスト関連のSQLは `playlist.rs`、ライブラリフォルダの記録のSQLは `library_folder.rs` に集約する（コマンド層に生SQLを書かない）
- ログは `log` クレートのマクロ（`log::info!` / `log::warn!` / `log::error!`）で出力する。`lib.rs` の `log_plugin()` が `tauri-plugin-log` を `log` のロガーとして登録しており、Tauriや依存クレートのログも同じ出力先に記録される
  - プラグインのJS API（`log:default` 等）はcapabilityに追加しない。WebViewからログを書き込ませない（ADR-005）

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

| 状態                                           | 管理方式               |
| ---------------------------------------------- | ---------------------- |
| UI表示状態・トースト通知・イコライザ・再生状態 | Runes（`*.svelte.ts`） |
| トラック/プレイリスト/検索結果                 | TanStack Query         |
| DB接続・現在トラックID                         | Tauri `AppState`       |

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
  - テストは対象と同じディレクトリに `*.test.ts` として置き、Node環境で実行する。テスト内で`$state`・`$effect`などのRunesを使う場合は `*.svelte.test.ts` にする
  - Svelteはアプリと同じクライアント向けにコンパイルする（`vitest.environment.ts`の環境と、Vitest実行時の`resolve.conditions: ['browser']`）。組み込みの`node`環境ではサーバー向けになり、`$effect`が実行されない
  - コンポーネント内の判定ロジックはテストしやすいよう `#lib/utils` の純粋関数へ切り出す（例: `selection.ts`）
- 変更前後で最低限以下を確認する
  - 型チェック（`npm run check`）: 警告も失敗扱い。Tailwindの`@apply`/`@reference`をCSS言語サービスが解釈できず誤警告になるため、CSS診断は対象外（`--diagnostic-sources js,svelte`）
  - Lint（`npm run lint`）: 警告も失敗扱い（`--max-warnings 0`）
  - フロントエンドテスト（`npm test`）
  - Rust静的検査（`cargo clippy ... -D warnings`）
  - Rustテスト（`cargo test`）

## ブラウザでの動作確認（IPCモック）

Tauriのウィンドウ（macOSではWKWebView）はブラウザ自動化ツールから操作できないため、Claude Code等でのUI確認はTauri IPCをモックしたブラウザで行う（判断の経緯は`decisions.md`のADR-006）。

- 起動: `npm run dev:mock`（Viteの`mock`モード・ポート1430）。Claude Codeでは`.claude/launch.json`の`web-mock`でプレビューを起動する
- 仕組み: `src/hooks.client.ts`が`mock`モードのときだけ`#lib/mocks/tauri`を読み込み、`@tauri-apps/api/mocks`で`window.__TAURI_INTERNALS__`を差し替える。`tauri dev`・本番ビルドではバンドルに含まれない
- `src/lib/mocks/`の構成:
  - `backend.ts`: `commands`の全コマンドをメモリ上で再現するバックエンド。ハンドラ表の型を`bindings.ts`から導出しているため、Rust側でコマンドを追加・変更したら型エラーに従ってここも更新する
  - `fixtures.ts`: 初期データ。状態はメモリ上のみで、リロードすると初期状態に戻る
  - `media.ts`: アルバムアート（SVGのdata URL）と再生用トーン（20秒のWAV）の生成。`albumart`プロトコルの代わりに`convertFileSrc(id, 'albumart')`がdata URLを返す
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
