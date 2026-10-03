# 設計概要: アーキテクチャ・データフロー

## 全体アーキテクチャ

```mermaid
graph TD
    U[User] --> FE[SvelteKit UI]
    FE --> STORES[共有状態 stores]
    FE --> QUERY[TanStack Query]
    QUERY --> INVOKE[@tauri-apps/api/core invoke]
    INVOKE --> CMD[Rust Commands]
    CMD --> REPO[repository / playlist / library / metadata]
    REPO --> DB[(SQLite + FTS5)]
    REPO --> FS[(ローカル音楽ファイル)]
    CMD --> EVT[Tauri Event Emitter]
    EVT --> FE
```

## レイヤー責務

| レイヤー                  | 主な実装                                                    | 責務                                               |
| ------------------------- | ----------------------------------------------------------- | -------------------------------------------------- |
| UI                        | `src/routes`, `src/lib/components`                          | 画面描画、ユーザー操作の受付                       |
| クライアント状態          | `src/lib/stores`                                            | 再生状態・UI状態など即時反映が必要な状態管理       |
| サーバー状態キャッシュ    | `src/lib/queries`                                           | `invoke`呼び出し、キャッシュ、再取得制御           |
| コマンド層                | `src-tauri/src/commands/*`                                  | 入力バリデーション、ユースケース単位の操作公開     |
| ドメイン/データアクセス層 | `repository.rs`, `library.rs`, `playlist.rs`, `metadata.rs` | SQL実行、ファイル走査、タグ読み書き                |
| 永続化                    | SQLite, ローカルファイル                                    | トラック/プレイリスト/統計の保存、音楽ファイル実体 |

## フロントエンド構成

- ルート: `src/routes/(app)` 配下にライブラリ・プレイリスト、`src/routes/settings` に設定画面（メニューから開く別ウィンドウ。独自のQueryClientを持つ）
- 設定: Rust側（`settings.rs`）がアプリデータ配下の`settings.json`に保存する。設定ウィンドウで保存するとRustが`SettingsChanged`イベントを送り、メインウィンドウの`SettingsSync`がキャッシュとアクセントカラー（CSS変数`--color-primary`）を更新する
- 起動時の画面: ルート（`/`）が設定に応じて、前回開いていた画面（`localStorage`に記録）か曲一覧へ移動する
- UI部品: `src/lib/components` と `src/lib/components/ui`
- 型: `src/lib/types/models.ts` をRustモデルと対応させる
- データ取得: `src/lib/queries/*.ts` のクエリ・ミューテーションがコマンド呼び出し・キャッシュ無効化・エラー通知を担い、コンポーネントからは直接コマンドを呼ばない
- アルバムアート: `#lib/utils/albumArt` の `albumArtUrl(trackId)` が返す `albumart://` のURLを `<img>` に指定し、Rust側のカスタムプロトコルから直接読み込む（フロントエンドに画像データを保持しない）
- 再生: `src/lib/stores/playback.svelte.ts` の再生コントローラーがaudio要素と再生状態（`player.svelte.ts` の `player`）を `$effect` で同期し、`Player.svelte` は表示と操作の受付に専念する
- 音声の経路: audio要素 → 音量の正規化（GainNode、ReplayGainから求めた倍率） → イコライザ（10バンドのBiquadFilterNode） → 全体ゲイン → 出力。Web Audioのグラフは`equalizer.svelte.ts`が作る
- ダイアログ: `src/lib/components/ui/Modal.svelte`（ネイティブの`<dialog>`）に統一。テキスト入力は`promptText()`の要求を、レイアウトに置いた`TextPromptDialog`が表示する
- ブラウザ確認用モック: `npm run dev:mock` のときだけ `src/hooks.client.ts` が `src/lib/mocks` のインメモリバックエンドへIPCを差し替える（`implementation.md` 参照）

## バックエンド構成

- エントリーポイント: `src-tauri/src/lib.rs`
- アプリ状態: `AppState { db: Mutex<Connection>, current_track_id: Mutex<Option<String>>, album_art_limiter: Semaphore, album_art_cache: Mutex<AlbumArtCache> }`
- アルバムアートの配信: `album_art.rs` が `albumart` カスタムプロトコルを処理する。トラックIDからDB上のファイルを引いて埋め込み画像をバイト列のまま返し、抽出結果（アートがないことを含む）は容量上限付きのLRUキャッシュ（64MiB）に保持する。WebViewには`no-store`でキャッシュさせず、インポート後はキャッシュを消去する
- 重い同期処理（インポート、メタデータ再読込、ファイルへのタグ書き込み、アルバムアート抽出、ファイル削除）は `run_blocking`（`spawn_blocking`）でブロッキング処理用スレッドへ逃がし、非同期ランタイムのワーカーを占有しない。アルバムアート抽出はセマフォで同時実行数を4に制限する
- バックエンド→フロントエンドの通知は `events.rs` の型付きイベント（tauri-specta）で行い、フロントは `bindings.ts` の `events.xxx.listen()` で受け取る
- コマンド登録: `tauri::generate_handler!` でインポート/検索/編集/再生/統計/システム操作を公開
- DB初期化: `db.rs` のマイグレーションでテーブル・インデックス・FTS5・トリガーを作成

## 主要データフロー

### インポート

```mermaid
sequenceDiagram
    participant User
    participant Frontend
    participant TauriCmd as import_folder
    participant DB as SQLite

    User->>Frontend: フォルダ選択
    Frontend->>TauriCmd: invoke(import_folder)
    TauriCmd->>TauriCmd: ディレクトリ再帰走査 / 重複判定
    TauriCmd->>DB: 50件単位でトランザクション保存
    TauriCmd-->>Frontend: ImportProgressイベント送信（import-progress）
    TauriCmd-->>Frontend: ImportResult返却
```

インポートしたフォルダは `library_folders` に記録する（ライブラリフォルダ）。

### ライブラリフォルダの再スキャン

1. 設定ウィンドウの「ライブラリ」で再スキャンを実行する（`rescan_library_folder`）
2. フォルダを走査し、ライブラリのトラックとサイズ・更新日時を比べる（`library_folder::plan_rescan`）
   - ライブラリにないファイルは追加、サイズか更新日時が変わったファイルは読み直す（変わっていないファイルは読まないため、DBだけで編集したメタデータは保たれる）
   - 見つからなくなったファイルの曲はライブラリから外す。ただしフォルダ自体が見つからない、または音楽ファイルが1件も見つからない場合は外さない
3. 読み込みはインポートと同じく、DBロックの外でメタデータを抽出し、50件単位で書き込む（`LibraryScanProgress`イベントで進捗を送る）
4. 曲が変わったら`LibraryChanged`イベントを送り、メインウィンドウ（`(app)/+layout.svelte`）がトラック一覧とプレイリストのキャッシュを無効化する

### 検索

1. フロントエンドで300msデバウンス
2. `search_tracks` 実行
3. バックエンドはFTS5 `MATCH` を優先し、失敗時は `LIKE` にフォールバック
4. 結果をTanStack QueryでキャッシュしてUIへ反映

### メタデータ編集

1. 入力値バリデーション（ID形式、文字数、年・トラック番号）
2. `update_track_metadata`（DBのみ）または`update_track_metadata_with_file`（DB+ファイル）を実行
3. 成功後に関連クエリをinvalidateして一覧表示を同期
