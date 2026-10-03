# 非機能要件: パフォーマンス・セキュリティ・可用性

## パフォーマンス

| 項目           | 方針 / 目標                             | 実装上の根拠                           |
| -------------- | --------------------------------------- | -------------------------------------- |
| インポート処理 | 大量ファイル時もメモリ/失敗影響を抑える | 50件単位トランザクションコミット       |
| 検索応答性     | 入力体験の遅延を抑える                  | フロント300msデバウンス + FTS5検索     |
| 一覧取得負荷   | 過剰取得を防ぐ                          | `DEFAULT_QUERY_LIMIT = 1000`           |
| 描画性能       | 長いリストでもUIを維持                  | 仮想スクロール運用（100曲以上）        |
| 再取得頻度     | 無駄な通信/処理を削減                   | TanStack QueryでstaleTime/gcTimeを設定 |

## セキュリティ

- データはローカル保存のみ（外部サーバー送信なし）
- ファイル/ディレクトリパスは `validation.rs` で検証（Null文字・親ディレクトリ遡りを拒否）
- ID・入力値はコマンド入口でバリデーション
- WebViewの権限は最小限（ADR-005）
  - ファイルの読み書き・削除・フォルダ表示はすべてRust側のコマンドで行い、WebViewからは任意のパスを扱えない（パスはトラックIDからDBで解決する）
  - capability（`src-tauri/capabilities/default.json`）は `core:default` と `dialog:allow-open` / `dialog:allow-message` のみ。fs / opener プラグインはWebViewに公開しない
  - 設定ウィンドウ（`settings`）は別のcapability（`settings.json`）で `core:default` と `core:window:allow-close`（キャンセルボタンでウィンドウを閉じる）のみ
  - `assetProtocol.scope` は空にし、`get_track_file_path` が返すトラックファイルだけを実行時に許可する
- CSPを明示（`tauri.conf.json`）
  - 画像は`img-src`で`data:`と`albumart`プロトコルのみ許可する。`albumart`はDBに登録済みのトラックのアートだけを配信し、任意のファイルは読めない
  - 本番は `script-src 'self'`（SvelteKitの起動用インラインscriptはTauriがビルド時にハッシュを付与）
  - Vite開発サーバー（HMR）向けの許可は `devCsp` に分離
- `freezePrototype` を有効化
- `window.confirm` / `window.prompt` は使わず、`#lib/utils/dialog` の `confirmDestructive` / `promptText` を `await` する（ESLintで禁止。前者はdialogプラグインにより非同期化され、後者はmacOSのWebViewで動作しない）
- 外部リンクはWebViewから開けないため、プロジェクトのページは固定URLを開く `open_project_page` コマンドで開く（任意のURLを開くコマンドは公開しない）

### 依存関係の脆弱性監視

- `.github/workflows/audit.yml` が毎週と、lockfileを変更するPRで `npm audit`（moderate以上で失敗）と `cargo-audit`（脆弱性で失敗、unmaintained・unsoundは警告）を実行する
- Dependabotはminor/patchをエコシステムごとに1つのPRへまとめる（メジャー更新は個別PR）
- 上流の対応待ちで受容しているアドバイザリ（解消条件を満たしたら再確認する）

| 対象                    | アドバイザリ                      | 経路                        | 受容理由・解消条件  |
| ----------------------- | --------------------------------- | --------------------------- | ------------------- |
| Rust `glib 0.18`        | RUSTSEC-2024-0429（unsound）      | Tauri → muda → gtk（Linux） | TauriのGTK4移行待ち |
| Rust `proc-macro-error` | RUSTSEC-2024-0370（unmaintained） | glib-macros（同上）         | 同上                |
| Rust `paste`            | RUSTSEC-2024-0436（unmaintained） | lofty / specta のproc-macro | 上流の置き換え待ち  |

## 可用性・運用

- ローカルアプリ前提のため、外部サービス障害の影響を受けない
- DB初期化時にマイグレーションを実行し、起動時に必要テーブルを自動準備
- 主要更新処理はトランザクション化し、不整合を抑制
- ログは `tauri-plugin-log` が標準出力とOS標準のログフォルダに出力する（ファイル名は `muspice.log`、レベルはInfo以上、時刻はローカル時刻）
  - macOS: `~/Library/Logs/com.ban367.muspice`、Windows: `%LOCALAPPDATA%\com.ban367.muspice\logs`、Linux: `~/.local/share/com.ban367.muspice/logs`
  - 5MiBを超えたら日時付きの名前に変えて新しいファイルに切り替え、古いファイルは4つまで残す（最大で約25MiB）
  - アプリデータ配下（以前の出力先）ではなくOS標準の場所にしているのは、macOSのコンソールアプリで参照でき、WindowsではRoamingプロファイルに含まれないため

## 品質維持の運用

- PR前に `npm run check`, `npm run lint`, `cargo clippy`, `cargo test` を通す
- CI失敗時は再現条件と環境依存差（特にLinux依存パッケージ）を確認する
- 実装と設計に乖離が見つかった場合はドキュメントを即時更新する
