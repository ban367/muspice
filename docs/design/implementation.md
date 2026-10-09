# 実装方針: 技術スタック・ディレクトリ・規約・テスト

## 技術スタック

| 層               | 技術                             | バージョン（2026-10-03時点）                       | 備考                                                                                                                                              |
| ---------------- | -------------------------------- | -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| フロントエンド   | SvelteKit + Svelte + TypeScript  | `@sveltejs/kit` 3.0.x / `svelte` 5.57.x / TS 6.0.x | SPA構成（adapter-static）                                                                                                                         |
| ビルド           | Vite                             | 8.3.x                                              | Tailwindは`@tailwindcss/vite`経由（PostCSS設定なし）                                                                                              |
| テスト           | Vitest                           | 5.0.x                                              | ストア・ユーティリティの単体テスト                                                                                                                |
| UIスタイル       | TailwindCSS + DaisyUI            | Tailwind 4.3.x / DaisyUI 5.7.x                     | `@apply`運用に制限あり                                                                                                                            |
| データ取得       | TanStack Query（Svelte）         | 6.3.x                                              | Queryキャッシュ/再取得制御                                                                                                                        |
| デスクトップ基盤 | Tauri + tauri-specta             | 2.12.x / 2.0.0-rc.25                               | 型付きコマンド呼び出しを自動生成                                                                                                                  |
| バックエンド     | Rust                             | edition 2024（stable）                             | コアロジック/DBアクセス                                                                                                                           |
| DB               | SQLite + FTS5                    | rusqlite 0.40（bundled）                           | 全文検索・ローカル保存                                                                                                                            |
| メタデータ       | lofty                            | 0.25                                               | タグ読み書き/アルバムアート抽出                                                                                                                   |
| フォルダの監視   | notify-debouncer-mini（notify）  | 0.7（notify 8）                                    | ライブラリフォルダの変更の自動反映                                                                                                                |
| ウィンドウの状態 | tauri-plugin-window-state        | 2.5.x                                              | メインウィンドウのサイズ・位置の記憶（Rust側のみ）                                                                                                |
| デバイスへの転送 | fs4 / unicode-normalization      | 1.1 / 0.1                                          | 転送先の空き容量の取得 / ファイル名のNFC正規化                                                                                                    |
| M3Uの読み込み    | encoding_rs                      | 0.8                                                | UTF-8以外（Shift_JIS・UTF-16）で書かれたM3Uの文字コードの変換                                                                                     |
| ライブラリのXML  | plist                            | 1.10                                               | iTunes形式のライブラリXML（プロパティリスト）の読み込み。Tauriが使っているものと同じ                                                              |
| 再生エンジン     | symphonia / cpal / rubato / rtrb | 0.6 / 0.18 / 5 / 0.4                               | デコード / 出力 / サンプルレートの変換 / リングバッファ。Opusは`symphonia-adapter-libopus` 0.3（libopusを同梱。ビルドにCコンパイラとcmakeが要る） |
| メディアキー     | objc2-media-player（objc2）      | 0.3（objc2 0.6）                                   | macOSのNow Playing・リモートコマンド（MediaPlayerフレームワーク）。macOSだけの依存で、Tauriが使っているobjc2系のクレートとそろえる                |

## ディレクトリ構成

```text
src/
├── hooks.client.ts        # dev:mock時のみTauri IPCモックを初期化
├── routes/
│   ├── (app)/
│   │   ├── devices/       # 転送先デバイスのページ
│   │   ├── library/
│   │   └── playlists/
│   └── settings/
└── lib/
    ├── bindings.ts        # tauri-spectaによる生成物（編集禁止）
    ├── components/
    │   ├── ui/
    │   └── library/
    ├── i18n/              # 多言語化（i18n.svelte.ts と messages/ja.ts・en.ts）
    ├── mocks/             # ブラウザ確認用のTauri IPCモック（dev:mock専用）
    ├── queries/
    ├── stores/
    ├── types/
    └── utils/

src-tauri/src/
├── commands/
│   ├── album_art.rs       # アルバムアートの埋め込み・取り除き・情報の取得（ADR-035）
│   ├── devices.rs         # 転送先デバイスの登録・設定・同期
│   ├── import.rs
│   ├── library_folders.rs
│   ├── metadata_cmd.rs
│   ├── playback.rs        # 再生エンジンのコマンド
│   ├── player.rs
│   ├── playlist_cmd.rs
│   ├── settings.rs
│   ├── stats.rs
│   ├── system.rs
│   └── tracks.rs
├── lib.rs
├── album_art.rs           # アルバムアートの配信（albumartプロトコル）・フォルダの画像の検索・キャッシュ
├── db.rs
├── device.rs              # 転送先デバイスの記録（SQL）と同期の実行状態
├── device_manifest.rs     # デバイス側の管理ファイル（.muspice/manifest.json）
├── device_sync.rs         # デバイス上の配置と差分の計算（ファイルシステムに触れない）
├── device_transfer.rs     # 同期の実行（削除・リネーム・コピー・プレイリストの書き出し）
├── error.rs
├── events.rs
├── repository.rs
├── search_text.rs         # 検索用の文字列の正規化（全文検索の表のトリガーが使うSQLの関数）
├── library.rs
├── library_folder.rs
├── library_sync.rs        # ライブラリフォルダの変更の自動反映（起動時・定期・監視）
├── library_xml.rs         # iTunes形式のライブラリXMLの読み込み（ほかのプレーヤーの再生回数・追加日・プレイリスト。ADR-033）
├── m3u.rs                 # プレイリストのM3Uの読み込み（文字コードの判定・曲の対応付け）と書き出し（ADR-032）
├── media_controls/        # OSのメディアキー・Now Playing（ADR-029）
│   ├── mod.rs             # OSへ渡す内容の組み立て（曲の情報・アルバムアート）
│   └── macos.rs           # macOSのMediaPlayerフレームワークの呼び出し（macOSだけでコンパイルする）
├── menu.rs                # メニューバー（「再生」メニューを含む）と設定ウィンドウのタイトル（言語に合わせる）
├── playback/              # 再生エンジン（ADR-025・ADR-026）
│   ├── engine.rs          # エンジン本体（コマンドの処理・曲の切り替え・再生位置の通知）
│   ├── decoder.rs         # ファイルのデコード（symphonia + libopus）
│   ├── mp4_gapless.rs     # M4AのAACの、曲の頭と終わりの余分の読み取り
│   ├── convert.rs         # 出力の形式（ステレオ・出力のサンプルレート）への変換
│   ├── normalization.rs   # 音量の正規化（ReplayGain）の倍率の計算
│   ├── effects.rs         # 出力の直前にかける加工（イコライザ・リミッター）
│   ├── render.rs          # 出力のコールバックでの音声の取り出し（イコライザ・音量・一時停止・リミッター）
│   └── output.rs          # 出力デバイスの一覧と、出力のストリーム（cpal）
├── playback_state.rs      # 再生状態（音量・キュー・再生していた曲）の保存と復元（playback-state.json）
├── playlist.rs
├── metadata.rs
├── models.rs
├── settings.rs
├── tag_backfill.rs        # 既存のトラックの、後から追加した項目（アルバムアーティスト・ソート用のタグ）の読み込み（起動時）
├── track_relink.rs        # 移動・改名されたファイルと、見つからない曲の対応付け
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
  - 再生制御（再生エンジンのコマンド・`setCurrentTrack`）はクエリではなく`#lib/stores/playback.svelte`の再生コントローラーが呼ぶ
  - 複数のクエリの状態（読み込み中・エラー）を合わせる時は、`#lib/queries/shared`の`combineQueryStates`を使う。TanStack Queryは、読んだことのあるプロパティが変わった時だけ通知するため、`a.isLoading || b.isLoading`のような短絡評価では、読まなかった側が先に終わった時の通知を取りこぼす（読み込み中の表示のまま止まる）
- アルバムアートは`#lib/utils/albumArt`の`albumArtUrl(trackId)`をそのまま`<img>`（`AlbumArt`コンポーネント）に渡す。画像データをフロントエンドで取得・保持しない。アートがない場合は読み込みエラーになり、`AlbumArt`がプレースホルダーを表示する
  - `albumArtUrl`は、アプリで画像を書き換えた曲の版（`#lib/stores/albumArt.svelte`）を読んでURLに付ける。版の変更に追随させるため、テンプレートか`$derived`の中で呼ぶ（値を変数に取っておくと、書き換えた後も古い画像のままになる）
  - アルバムアートの画面は、`albumArtDialog.open(tracks)`で開く（`(app)/+layout.svelte`が1つだけ表示する。開く側にダイアログを置かない）
- メタデータの編集画面（`MetadataEditor`）は、入力欄の値と`Metadata`の変換・検証を`#lib/utils/metadataForm`に分けている（1曲の編集はすべての項目を渡し、一括編集は入力した項目だけを渡す）。1曲の編集は、開いた時に`useTrackTagsQuery`でファイルのタグを読み、読めるまで・読めない場合は保存できない（ADR-034）
- タグの一括ツール（`TagToolsDialog`。曲の右クリックのメニューから`tagToolsDialog.open(tracks)`で開く）は、変更の計算を`#lib/utils/tagTools`の純粋な関数（`guessFromFileName`・`renumberTracks`・`searchAndReplace`）に分けている。どれも曲ごとの変更（前 → 後）を返すだけで、画面はそれを一覧に出し、「適用」で`toMetadataChanges`の結果を`useApplyMetadataChangesMutation`へ渡す（ADR-037）
- 名前の順に並べる時は、`#lib/utils/nameSort`の`compareNames`・`sortByName`を使う（`localeCompare`・`toLowerCase`での比較を各所に書かない）。アルバム・アーティストなどは、並び順に使う値（`sortName`）があればその値で並べる。曲の一覧の並び替え（`trackSort`）も、タイトル・アーティスト・アルバムは`track.sortTags`の値を優先する
- お気に入りのハートは`FavoriteButton`（`#lib/components/library`）を使う。曲のIDと今の状態を渡すと、押した時に`useSetFavoriteMutation`で切り替え、キャッシュにあるその曲を書き換える（`patchTracksInCache`。複数の曲をまとめて書き換えられる）
  - 再生キューの曲（`player.currentTrack`）は、キューに入れた時点の内容のまま。プレーヤーのハートは、お気に入りの一覧（`useFavoriteTracksQuery`）から状態を調べる
- クエリキーは`src/lib/queries/keys.ts`の`queryKeys`に集約する。クエリ定義・無効化のどちらもここを参照し、`['tracks']`のようなマジック配列を直接書かない
- 無効化はプレフィックス一致で波及するため、キーの階層がそのまま無効化の粒度になる（例: `queryKeys.tracks.all`の無効化は検索・フィルタ・お気に入りにも及ぶ）

### 再生制御

- 再生エンジンの操作・キュー遷移・リピート・再生回数の記録・イコライザの設定の送信は`#lib/stores/playback.svelte`の`createPlaybackController(options)`が担う。`Player.svelte`は表示と操作の受付だけを行い、コントローラーのメソッドを呼ぶ
- 次・前のトラックの決定は`#lib/stores/player.svelte`のキュー操作（`playNextTrack`・`playPreviousTrack`）が担う。キュー操作の結果が再生中と同じトラックだった場合（1曲リピート、3秒以上再生中の「前へ」、1曲だけのキューの全曲リピート）はトラックIDが変わらず再生が始まらないため、コントローラーが頭から再生し直す
- 再生状態の復元と保存は`#lib/stores/playbackState.svelte`が担い、再生コントローラーが作成時に呼ぶ（ADR-028）
  - 復元（`restorePlaybackState`）は、音量・シャッフル・リピートと、キュー・再生していた曲を`player`へ戻す。再生コントローラーは`player.currentTrack`の変化で再生を始めるため、戻す直前に「この曲は再生を始めない」と記録する（復元した曲は、再生ボタンで頭から再生する）
  - 保存（`watchPlaybackState`）は、復元が済んでから始める（先に始めると、復元する前の空の状態で上書きする）。変更から0.3秒待ってまとめて送り、キューは、キューの配列が置き換わった時だけ送る
  - エンジンが曲を持っていない間（復元した直後・再生に失敗した後）は、シークしても位置を動かさない（再生ボタンで頭から再生するため）
- 再生回数・スキップ回数に数えるかどうかは`#lib/stores/playTracker`が決める（ADR-030）。再生コントローラーが、曲の再生の始まり（`begin`）・再生位置の通知（`progress`）・曲の終わり（`finish`）・停止や失敗（`reset`）を伝え、数える時に`#lib/queries/tracks`の`recordPlay`・`recordSkip`が呼ばれる
  - `recordPlay`は、キャッシュにあるその曲の再生回数を書き換え、再生履歴と「よく再生する曲」を取り直す（開いている画面に、そのまま反映される）
- OSのNow Playingへの通知は`#lib/stores/nowPlaying`が担い、再生コントローラーが`player`の状態を渡す（ADR-029）。再生位置は0.25秒ごとに変わるが、伝え直すのは、曲・再生中かどうかが変わったときと、位置がOSの計算と1秒以上ずれたときだけ
- メニューバーの「再生」メニューと、OSのメディアキーなどからの操作（`PlaybackControl`イベント）は、`Player.svelte`が受け取り、ウィンドウの中のキー操作と同じ処理を行う
- 再生キュー（右サイドバー）の曲をダブルクリックした時は、`playQueueIndex(index)`でキューの並びを変えずに再生位置だけを移す（指定した位置より前の曲もキューに残る）
- 再生中かどうか（`isPlaying`）は、エンジンのコマンドの結果（再生を始めた・一時停止した）と通知（失敗）から更新する。再生位置（`currentTime`）は、エンジンの`position`の通知で更新する
- エンジンへ渡す番号（トークン）は、再生する曲・続けて再生する曲ごとに増やす。通知（`PlaybackEvent`）は番号で見分け、前の曲についての通知は捨てる
- ギャップレス再生・クロスフェードでは、キューの次の曲（`#lib/stores/player.svelte`の`peekNextTrack()`。`playNextTrack`の進む先を、状態を変えずに返す）を、先にエンジンへ伝える（`playbackSetNext`）。キュー・リピート・シャッフル・設定が変わると`$effect`で伝え直す
  - エンジンは、再生中の曲の終わりから切れ目なく（クロスフェードでは、終わりと重ねて）次の曲を続け、鳴っている曲が切り替わった時点で`advanced`を送る。コントローラーは、再生し直さずに`playNextTrack()`でキューを進める
  - 伝えてあった曲が、もうキューの次の曲でない場合（伝えた後でキューを変えた場合）は、キューの次の曲を再生し直す
  - 次の曲を伝えていない場合（ギャップレス再生もクロスフェードも無効）・次の曲の準備に失敗した場合は、曲の終わりの`ended`でキューを進め、次の曲を`playbackPlay`で再生する
  - 同じ曲を繰り返す切り替え（1曲リピート）では、再生中の曲の通知（`setCurrentTrack`）はしない（再生回数は、聴いた時間を数え直して、届けばもう1回数える）
  - 重ねる処理・重ねる長さ（設定の秒数を上限に、どちらの曲も長さの半分まで）・手動の操作や1曲リピートではクロスフェードしないことは、エンジンが決める（ADR-026）
- 音量の正規化とクロスフェードの設定は、`save_settings`がエンジンへ伝える（フロントからは送らない）。イコライザの設定は`equalizer.svelte`から読み、起動時と変更のたびに`playbackSetEqualizer`で送る
- シークバーのドラッグ中は、シークを間隔（80ms）を空けて送る（そのたびにシークすると、音が細切れになる）
- コントローラーは設定を直接読まず、`options`の関数（`gapless`・`crossfadeSeconds`）で受け取る（テストで差し替えるため）
- アルバム・プレイリストなどの「シャッフル再生」は`playShuffled(tracks)`を使う（配列を`sort(() => Math.random() - 0.5)`などで独自に並べ替えない）。シャッフルモードを有効にし、元の順序を保持するため、解除すると元の順序に戻る

### 多言語化（i18n）

- 画面に表示する文言は、コンポーネントに直接書かず`src/lib/i18n/messages/ja.ts`（正）と`en.ts`に定義し、`#lib/i18n/i18n.svelte`の`m`から読む（例: `m.common.cancel`、`m.common.trackCount(3)`）。英語は`Messages`型（`typeof ja`）にするため、キーの過不足は型チェックで分かる
  - 引数のある文言は関数にし、語順や単数形・複数形は言語ごとの関数で決める（文字列をつなげて文を作らない）
  - コンソールのログ（`console.*`）・Rustのログは日本語のまま（利用者に表示しない）
- `m`は読むたびに今の言語のメッセージを返す。テンプレート・`$derived`・イベント処理の中で読む。スクリプトの初期化時に文字列として取り出すと、言語を切り替えても変わらない（選択肢は値の配列にして、ラベルをテンプレートで`m`から読む。propsの既定値に文言を書かず、テンプレートで`??`で補う）
- 日付・数値の書式は`i18n.locale`（例: `ja-JP`）を使う（`#lib/utils/format`）
- 言語は設定の`language`。メインウィンドウは`SettingsSync`、設定ウィンドウは設定画面が`applyLanguage`で反映し、設定を読み込むまでの間は`restoreLanguage`で前回の言語を使う
- エラー: `toErrorMessage`はcodeごとの汎用メッセージ（`m.errors.byCode`）を使う。日本語では、`NOT_FOUND`・`VALIDATION`はバックエンドの日本語のメッセージをそのまま表示し、英語では汎用メッセージにする（バックエンドのメッセージは日本語のため）
- Rust側の文言はメニューバーと設定ウィンドウのタイトルだけ（`menu.rs`）。言語を変えて保存すると`save_settings`が作り直す

### 一覧の選択とキーボード操作

- 一覧では、矢印キーを押しても一覧をスクロールさせず、選択している項目から隣の項目へ選択を移す（移動先が画面の外なら、見える位置までスクロールする）。移動先の計算は`#lib/utils/listNavigation`の`navigationTarget`（リストは↑↓、グリッドは上下左右、Home・End）で行う
- 曲の一覧（`TrackList`、アルバム・アーティストの詳細、グループのモーダル、プレイリストの詳細）は、一覧ごとに`#lib/utils/trackSelection.svelte`の`TrackSelection`を作り、行のクリックを`click()`へ、一覧の`onkeydown`を`handleTrackListKeydown`へ渡す
  - 矢印キーで選択を移し、Shift+矢印で範囲選択、Cmd/Ctrl+Aですべて選択、Enterで再生する
  - 一覧の要素を`role="listbox"`・`tabindex="0"`にしてフォーカスを受け、行は`role="option"`・`data-track-id`を付けてフォーカスを受けない（行をクリックすると一覧の要素にフォーカスが移り、行が消えてもキー操作を続けられる）
  - 表示する一覧が別のものに変わった時（別のアルバムを選んだ等）は`reset()`で選択を消す
  - 仮想スクロールの一覧では、移動先の行の要素がないことがあるため、`handleTrackListKeydown`の`scrollTo`に`VirtualList`の`scrollToIndex`を渡す（渡さない場合は、`data-track-id`の付いた行の要素を探してスクロールする）
- 1つだけ選択する一覧（2ペイン表示の左の`AlbumList`・`ArtistList`）は、`listSelectionTarget`で移動先を求めて選択し、その項目へフォーカスも移す（Tabでは選択中の項目だけに止まる）
- グリッド表示（`LibraryGrid`: アルバム・アーティスト・ジャンル）は、現在位置の項目を枠で示し、Enterでクリックと同じ操作（`onOpen`）を行う
- 修飾キーなしの矢印キーは一覧が使う。プレーヤーのショートカット（`Player.svelte`）はCmd/Ctrl+矢印（前へ・次へ・音量）とSpace（再生・一時停止）

### ドラッグ&ドロップ

- 曲の一覧からサイドバーのプレイリストへの追加は、HTML5のドラッグ&ドロップで行う。`#lib/utils/trackDrag`の`startTrackDrag`（`dragstart`）でトラックIDを専用の種類（`TRACK_DRAG_TYPE`）のデータとして運び、`isTrackDrag`（`dragover`）・`readDraggedTrackIds`（`drop`）で受け取る
- 運ぶ曲は`TrackSelection.beginDrag(trackId)`で決める。選択中の曲の上で始めた場合は選択中の曲すべて（一覧の並び順）、そうでなければその曲だけ
- Tauriのファイルドロップ（`tauri.conf.json`の`dragDropEnabled`）は無効にする。有効だとWebViewに`dragover`・`drop`が届かない（ADR-018）

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
- 色は`app.css`の変数（`bg-base-*`・`text-text-*`・`bg-surface-*`・`border-border`など）を使い、テーマで変わるべき色に`white`・`black`・`rgba(255, 255, 255, …)`などを直接書かない（ライトテーマで読めなくなるため）。アクセントカラーを薄くする場合は`bg-primary/15`か`color-mix(in oklab, var(--color-primary) 15%, transparent)`。オーバーレイ（`bg-black/50`）・影・色付きの面の上の白い文字は、どちらのテーマでも同じでよい
- 配色はテーマ（`<html data-theme="dark|light">`）ごとに`app.css`の「配色（ダーク / ライト）」で定義する。`@theme`はユーティリティを作るための登録で、値はダークの配色。DaisyUI 5のテーマが同じ名前の変数を`@layer base`で定義して`@theme`より優先されるため、配色はレイヤーの外で定義し直している。色の変数を追加したら、`@theme`とライトの配色の両方に書く

### VS Codeワークスペース運用

- `muspice.code-workspace` は `root` / `docs` / `tauri` の3ルート構成とする
- `.vscode/settings.json` の `files.exclude.docs = true` は appルート内での重複表示を避けるために維持し、編集は workspace の各ルートから行う

### エラーハンドリング

- Rustコマンドは `AppResult<T>`（`error.rs` の `AppError`）を返す。エラーは `{ code, message }` 形式でシリアライズされ、messageは日本語のユーザー向け文言とする（英語の表示ではcodeごとの汎用メッセージを使う。「多言語化」参照）
- エラーコード: `LOCK` / `DATABASE` / `NOT_FOUND` / `VALIDATION` / `IO` / `METADATA`
- フロントエンドでは `handleError` を必ず経由し、codeでエラーを分類する（部分文字列マッチは行わない）
- DBアクセスはコマンド層で `AppState::with_db` を経由し、ロック取得エラーの処理を一元化する
- ファイルI/O・タグ解析・大量のDB書き込みなど重い同期処理は `commands::run_blocking` で実行する（asyncコマンド内で直接行うと非同期ランタイムのワーカーを占有する）。状態が必要な場合は `AppHandle` を受け取り、クロージャ内で `app.state::<AppState>()` から取得する
- トラック関連のSQLは `repository.rs`、プレイリスト関連のSQLは `playlist.rs`、ライブラリフォルダの記録のSQLは `library_folder.rs`、転送先デバイスの記録のSQLは `device.rs` に集約する（コマンド層に生SQLを書かない）
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
- フロントと共有する定数（設定の値の範囲など）は、`specta_builder()` の `.constant()` で `bindings.ts` にエクスポートし、TS側で同じ値を書かない（例: `MAX_CROSSFADE_SECONDS`・`LIBRARY_SCAN_INTERVALS`）
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
  - 再生エンジン（`playback/`）は、音声デバイスのない環境でも動かせるよう、出力を`OutputBackend`で差し替えてテストする（エンジンをスレッドなしで動かし、出力のコールバックの代わりにテストから音声を取り出す）
  - 非可逆の形式（MP3・AAC・Opus・Vorbis）の、曲の頭・終わりの余分とシークの位置は、`src-tauri/tests/fixtures/playback/`の短いファイルで確かめる（作り直す手順は`decoder.rs`のテストのコメント）
  - 実際の出力デバイスでの再生は、手動のテストで確かめる（音量を0にするため音は出ない）: `PLAYBACK_TEST_FILES=a.flac:b.mp3 cargo test real_output -- --ignored --nocapture`。再生位置・曲の切り替わりの通知と、かかった時間（実時間で再生されたか）、音切れのログを表示する（`PLAYBACK_TEST_CROSSFADE=2`でクロスフェードの秒数、`PLAYBACK_TEST_PLAIN=1`で途中の操作なしを指定できる）
- フロントエンドのロジック（ストア・ユーティリティ）は Vitest で単体テストする（`npm test`）
  - テストは対象と同じディレクトリに `*.test.ts` として置き、Node環境で実行する。テスト内で`$state`・`$effect`などのRunesを使う場合は `*.svelte.test.ts` にする
  - Svelteはアプリと同じクライアント向けにコンパイルする（`vitest.environment.ts`の環境と、Vitest実行時の`resolve.conditions: ['browser']`）。組み込みの`node`環境ではサーバー向けになり、`$effect`が実行されない
  - コンポーネント内の判定ロジックはテストしやすいよう `#lib/utils` の純粋関数へ切り出す（例: `selection.ts`、`listNavigation.ts`）
- 変更前後で最低限以下を確認する
  - 型チェック（`npm run check`）: 警告も失敗扱い。Tailwindの`@apply`/`@reference`をCSS言語サービスが解釈できず誤警告になるため、CSS診断は対象外（`--diagnostic-sources js,svelte`）
  - Lint（`npm run lint`）: 警告も失敗扱い（`--max-warnings 0`）
  - フロントエンドテスト（`npm test`）
  - Rust静的検査（`cargo clippy ... -D warnings`）
  - Rustテスト（`cargo test`）

### 長い一覧の描画（仮想スクロール）

- 件数が多くなる一覧は、`#lib/components/ui`の`VirtualList`で描画する（曲の一覧`TrackList`、プレイリストの詳細、再生キュー、`AlbumList`・`ArtistList`、`LibraryGrid`）。見えている行とその前後の少しの行だけを描画するため、数万件でもDOMの要素は数十個に収まる
- `VirtualList`がスクロールする領域になる（親で高さを決める）。列の見出しは`header`に渡すと上に固定される。`role`・`tabindex`・`onkeydown`などは、項目を並べる要素に渡される
- 行の高さはすべて同じとして扱う。高さは決め打ちにせず、描画した行を実測する（`estimatedRowHeight`は最初の描画だけに使う）。`minColumnWidth`を渡すとグリッドになり、幅に入るだけ列を並べる
  - 高さがそろわない項目（名前の長さで高さが変わるジャンルのカード）は位置を正しく求められないため、すべて描画する（`LibraryGrid`の`virtualized={false}`）
- `row`は、項目ごとに要素を1つだけ描画する（描画した行の数を、要素の数から数えるため）
- 表示範囲・列数・スクロール先の計算は`#lib/utils/virtualList`の純粋関数（`computeVirtualRange`など）で行う
- 描画されていない行があるため、行の要素を探す処理（`querySelector`・`scrollIntoView`）には頼らない。位置は一覧の中の順番（index）で扱い、スクロールは`scrollToIndex`、グリッドの列数は`getColumns`を使う
- 一覧の全件を対象にする処理（並び替え・選択・行ごとの判定）は、数万件でも重くならないようにする
  - 並び替えは`#lib/utils/trackSort`の`createTrackSorter`を使う（評価の変更などで一覧が作り直されても、並び替えに使う値が同じなら比較をやり直さない）
  - 行の中で`indexOf`・`includes`のような全件の走査をしない（行の位置は`row`が受け取るindexを使う）
  - 再生キューへの追加は`player.svelte.ts`の`addToQueue`・`addNextInQueue`を使う（`splice(...tracks)`のような引数への展開は、件数が多いと失敗する）

## ブラウザでの動作確認（IPCモック）

Tauriのウィンドウ（macOSではWKWebView）はブラウザ自動化ツールから操作できないため、Claude Code等でのUI確認はTauri IPCをモックしたブラウザで行う（判断の経緯は`decisions.md`のADR-006）。

- 起動: `npm run dev:mock`（Viteの`mock`モード・ポート1430）。Claude Codeでは`.claude/launch.json`の`web-mock`でプレビューを起動する
- 仕組み: `src/hooks.client.ts`が`mock`モードのときだけ`#lib/mocks/tauri`を読み込み、`@tauri-apps/api/mocks`で`window.__TAURI_INTERNALS__`を差し替える。`tauri dev`・本番ビルドではバンドルに含まれない
- `src/lib/mocks/`の構成:
  - `backend.ts`: `commands`の全コマンドをメモリ上で再現するバックエンド。ハンドラ表の型を`bindings.ts`から導出しているため、Rust側でコマンドを追加・変更したら型エラーに従ってここも更新する
  - `fixtures.ts`: 初期データ。状態はメモリ上のみで、リロードすると初期状態に戻る
  - `media.ts`: アルバムアート（SVGのdata URL）の生成。`albumart`プロトコルの代わりに`convertFileSrc(id, 'albumart')`がdata URLを返す
  - `playbackEngine.ts`: 再生エンジンのコマンドと通知の再現。音は鳴らさず、時計に合わせて再生位置・曲の切り替わり（ギャップレス再生・クロスフェード）・曲の終わりを進める
  - `tauri.ts`: event・dialog・windowプラグインと`convertFileSrc`の差し替え
- ネイティブメニューのイベントや確認ダイアログの回答は、開発者ツールから`window.__MUSPICE_MOCK__`で操作する
  - `window.__MUSPICE_MOCK__.emit('open-import-dialog')`（`toggle-sidebar` / `show-about-dialog`も同様）
  - `window.__MUSPICE_MOCK__.emit('playback-control', { type: 'next' })`で、「再生」メニュー・OSのメディアキーの操作を再現する
  - `window.__MUSPICE_MOCK__.nowPlaying()`で、OSのNow Playingへ伝えた内容を確認する（ブラウザにはOSの表示がないため）
  - `window.__MUSPICE_MOCK__.setM3uImportMode('clean')`で、M3Uの読み込みで選んだことにするファイルを切り替える（`partial`: 対応が付かない行がある（既定）・`clean`: すべて対応が付く・`cancel`: 選ばなかった）。ファイルを選ぶダイアログはRust側が開くため、モックは決まった結果を返す
  - `window.__MUSPICE_MOCK__.setAlbumArtPickMode('cancel')`で、アルバムアートの埋め込みで選んだことにする画像を切り替える（`pick`: 埋め込める画像（既定。選ぶたびに違う色の画像になる）・`cancel`: 選ばなかった・`tooLarge`: 大きすぎる画像）。フィクスチャでは、「Midnight Circuit」がフォルダの画像（`cover.jpg`）、「Quiet Rooms」がアートなし、ほかは埋め込みの画像
  - `window.__MUSPICE_MOCK__.setConfirmResult(false)`で、以降の確認ダイアログを「キャンセル」にする
  - `window.__MUSPICE_MOCK__.setFolderResult('/Volumes/NEW_SD')`で、以降のフォルダ選択ダイアログで選ばれるパスを変える。既定のパスはライブラリフォルダの中のため、転送先デバイスの追加を確認するときはライブラリの外のパスにする
- 音は鳴らない（再生位置と曲の切り替わりだけが進む）。音の確認は、実アプリで行う
- 再生状態（音量・再生キューなど）は、起動時の復元を確認できるよう`sessionStorage`に保存する（再読み込みの後も残り、タブを閉じると消える。ほかの状態は、再読み込みで初期状態に戻る）
- 数万曲のライブラリでの動作は、URLに`?mockTracks=50000`を付けて開くと確認できる（例: `/library/songs?mockTracks=50000`。フィクスチャに加えて、指定した数のトラックを生成する。アプリ内の移動では状態が保たれ、再読み込みすると付けたURLで開き直すまで元の件数に戻る）
- デバイスへの転送は、デバイス上の曲を「コピー済みのトラックの集合」で再現する（配置の決定・リネーム・プレイリストのファイルは再現しない）
- 確認できないもの: Rust側の処理（SQLite・FTS5・ファイルI/O・タグ読み書き）、実ファイルの再生（再生エンジンのデコード・出力）、CSP・capabilityによる制約。これらは`cargo test`と`npm run tauri dev`で確認する

## CI方針

`.github/workflows/ci.yml` で以下を実行:

1. Frontend Check（type-check, lint, format:check, test）
2. Backend Check（fmt --check, clippy, test）
3. Build Test（PR時のみ、Tauri build）

Linuxでは、Tauriの依存に加えて、再生エンジンの出力（`cpal`のALSA）のために`libasound2-dev`を入れる。libopusのビルドにはcmakeが要る（GitHubのランナーには入っている。手元のmacOSでは`brew install cmake`）。

`.github/workflows/audit.yml` で依存関係の脆弱性を検査する（毎週・lockfile変更PR時。詳細は `non-functional.md`）。

## 実装時のドキュメント同期ルール

- データモデル・API仕様変更: `docs/design/detailed-design.md`
- 技術スタック・構成・規約変更: `docs/design/implementation.md`
- 全体構成・データフロー変更: `docs/design/architecture.md`
- 非機能要件（パフォーマンス・セキュリティ・可用性）に関わる変更: `docs/design/non-functional.md`
- 代替案・トレードオフ: `docs/design/decisions.md`
