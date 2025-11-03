# Muspice

PC向けデスクトップ音楽管理アプリケーション

## 技術スタック

- **フロントエンド**: SvelteKit 2.x + TypeScript
- **バックエンド**: Tauri 2.x + Rust
- **データベース**: SQLite

## 開発環境のセットアップ

### 必要なツール

- Node.js 24.x
- Rust (最新安定版)
- npm

### インストール

```bash
# 依存関係のインストール
npm install

# Rustの依存関係のインストール
cd src-tauri && cargo build
```

## 開発コマンド

### フロントエンド開発

```bash
# 開発サーバー起動
npm run dev

# ビルド
npm run build

# プレビュー
npm run preview
```

### Tauriアプリ開発

```bash
# Tauriアプリ開発モード
npm run tauri dev

# Tauriアプリビルド
npm run tauri build
```

### コード品質

```bash
# TypeScript型チェック
npm run check

# ESLint実行
npm run lint

# ESLint自動修正
npm run lint:fix

# Prettier実行
npm run format

# Prettierチェック
npm run format:check
```

### Rustテスト

```bash
cd src-tauri && cargo test
```

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) + [ESLint](https://marketplace.visualstudio.com/items?itemName=dbaeumer.vscode-eslint) + [Prettier](https://marketplace.visualstudio.com/items?itemName=esbenp.prettier-vscode)
