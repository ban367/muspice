# プロジェクト構造

## ディレクトリ構成

```
muspice/
├── src/                    # SvelteKitフロントエンドソース
│   ├── routes/            # SvelteKitルート (ページ)
│   └── app.html           # HTMLテンプレート
├── src-tauri/             # Tauriバックエンド (Rust)
│   ├── src/
│   │   ├── main.rs       # Tauriエントリーポイント
│   │   └── lib.rs        # ライブラリコード
│   ├── capabilities/      # Tauriセキュリティ設定
│   ├── icons/            # アプリケーションアイコン
│   ├── Cargo.toml        # Rust依存関係
│   ├── tauri.conf.json   # Tauri設定
│   └── build.rs          # ビルドスクリプト
├── static/                # 静的アセット
├── .kiro/                 # Kiro設定・仕様書
│   ├── specs/            # 機能仕様書
│   └── steering/         # ステアリングルール
├── target/                # Rustビルド出力 (gitignore)
├── package.json           # Node.js依存関係
├── vite.config.js         # Vite設定
├── svelte.config.js       # Svelte設定
├── tsconfig.json          # TypeScript設定
└── mise.toml              # 開発環境ツール設定
```

## コード配置規則

### フロントエンド (src/)

- **routes/**: SvelteKitのファイルベースルーティング
  - `+page.svelte`: ページコンポーネント
  - `+layout.svelte`: レイアウトコンポーネント
  - `+page.ts/+page.server.ts`: ページロード関数
- **lib/** (予定): 共有コンポーネント、ユーティリティ、stores
  - `components/`: 再利用可能なSvelteコンポーネント
  - `stores/`: Svelte stores (状態管理)
  - `utils/`: ヘルパー関数
  - `types/`: TypeScript型定義

### バックエンド (src-tauri/src/)

- **main.rs**: Tauriアプリケーションのエントリーポイント
- **lib.rs**: ライブラリコード、Tauriコマンド定義
- **modules/** (予定): 機能別モジュール
  - `db/`: データベース操作
  - `metadata/`: メタデータ抽出・編集
  - `playlist/`: プレイリスト管理
  - `library/`: 音楽ライブラリ管理
  - `models/`: データモデル定義

## 命名規則

### ファイル名

- **Svelte**: PascalCase (例: `Library.svelte`, `Player.svelte`)
- **TypeScript**: camelCase (例: `audioPlayer.ts`, `metadataUtils.ts`)
- **Rust**: snake_case (例: `music_library.rs`, `playlist_manager.rs`)

### コード

- **変数・関数**: camelCase (TypeScript), snake_case (Rust)
- **クラス・型**: PascalCase (両言語共通)
- **定数**: UPPER_SNAKE_CASE (両言語共通)

## 設定ファイル

- **package.json**: npm依存関係、スクリプト定義
- **Cargo.toml**: Rust依存関係、パッケージメタデータ
- **tauri.conf.json**: Tauriアプリケーション設定
- **vite.config.js**: Vite/SvelteKitビルド設定
- **svelte.config.js**: Svelteコンパイラ設定
- **tsconfig.json**: TypeScriptコンパイラ設定
- **mise.toml**: 開発ツールバージョン管理

## ビルド出力

- **build/**: フロントエンドビルド出力 (Tauriが参照)
- **target/**: Rustビルド出力
- **.svelte-kit/**: SvelteKit内部ファイル (gitignore)
