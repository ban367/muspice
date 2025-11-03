# 技術スタック

## アーキテクチャ

Tauri + SvelteKit ハイブリッドデスクトップアプリケーション

## フロントエンド

- **フレームワーク**: SvelteKit 2.x (Svelte 5.x)
- **言語**: TypeScript 5.6.x
- **ビルドツール**: Vite 6.x
- **アダプター**: @sveltejs/adapter-static (SPA mode)
- **UI**: Tailwind CSS + DaisyUI (予定)
- **状態管理**: Svelte stores

## バックエンド (Tauri)

- **フレームワーク**: Tauri 2.x
- **言語**: Rust (edition 2021)
- **データベース**: SQLite (rusqlite)
- **メタデータ処理**: lofty crate
- **シリアライゼーション**: serde, serde_json

## 開発環境

- **Node.js**: 24.x (mise管理)
- **パッケージマネージャー**: npm
- **Rust**: cargo

## 共通コマンド

### 開発

```bash
# フロントエンド開発サーバー起動
npm run dev

# Tauriアプリ開発モード
npm run tauri dev
```

### ビルド

```bash
# フロントエンドビルド
npm run build

# Tauriアプリビルド
npm run tauri build
```

### 型チェック

```bash
# Svelte型チェック
npm run check

# Svelte型チェック (watch mode)
npm run check:watch
```

### テスト

```bash
# Rustユニットテスト
cd src-tauri && cargo test

# フロントエンドテスト (予定)
npm test
```

## プロジェクト設定

### Vite設定

- 開発サーバーポート: 1420 (固定)
- HMRポート: 1421
- src-tauriディレクトリは監視対象外

### TypeScript設定

- strict mode有効
- moduleResolution: bundler
- .svelte-kit/tsconfig.jsonを継承

### Tauri設定

- 開発URL: http://localhost:1420
- ビルド出力: ../build
- 開発前コマンド: npm run dev
- ビルド前コマンド: npm run build
