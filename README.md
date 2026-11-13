# Muspice

macOS向けのデスクトップ音楽管理アプリケーション。手元の音楽ファイルを、インポート・整理・再生します。データはMacの中だけに保存し、外部とは通信しません。

## 主な機能

- **ライブラリ**: フォルダ・ファイルのインポート（MP3・FLAC・WAV・M4A（AAC・ALAC）・AIFF・Ogg Vorbis・Opus・WavPack・APEなど）、フォルダの監視と再スキャン、曲・アルバム・アーティスト・ジャンル・フォルダ・年代での一覧、検索と絞り込み
- **再生**: ギャップレス再生・クロスフェード・音量の正規化（ReplayGain）・10バンドのイコライザ・再生速度の変更、再生キュー、メディアキーとコントロールセンターでの操作、ミニプレーヤー、歌詞の表示
- **タグとファイル**: タグの単体・一括の編集、アルバムアートの埋め込み、評価（半分の星）、タグに基づくファイルの整理、形式の変換
- **プレイリスト**: 通常のプレイリストと自動プレイリスト（評価・再生回数・追加日などの条件）、M3Uの読み込み・書き出し
- **そのほか**: 再生回数・再生履歴・お気に入り、SDカードなどへの転送、ライブラリのデータのバックアップと復元、日本語と英語の表示

機能の範囲は、[概要](docs/design/overview.md)にあります。

## 対応するOS

macOSだけに対応しています。Linuxは、CIでビルドとテストを通していますが、動作は保証しません。Windowsには対応していません（[設計判断](docs/design/decisions.md)のADR-090）。

## 技術スタック

- **フロントエンド**: SvelteKit 3.x + TypeScript
- **バックエンド**: Tauri 2.x + Rust
- **データベース**: SQLite

## 開発環境のセットアップ

### 必要なツール

- Node.js 24.15以上（`.node-version`の24.22.2を使う）
- npm 11以上（Node.js 24に同梱）
- Rust（`rust-toolchain.toml`の版。rustupが自動で入れる）
- CコンパイラとCMake（Opusのデコーダーの`symphonia-adapter-libopus`が、同梱のlibopusをビルドするため。macOSではXcodeのCommand Line Toolsと、Homebrewなどで入れたCMake）
- Linux（対応の対象外。ビルドとテストだけを通す）では、Tauri（WebKitGTK・GTK・トレイ）と音声の出力（ALSA）に要るシステムのパッケージ（CIの`.github/workflows/ci.yml`で入れているものと同じ）。Ubuntuの場合（`build-essential`・`cmake`は、上のCコンパイラとCMake）:

```bash
sudo apt-get install -y build-essential cmake \
  libwebkit2gtk-4.1-dev librsvg2-dev patchelf libgtk-3-dev \
  libayatana-appindicator3-dev libasound2-dev
```

Node.js・npmの版は`package.json`の`engines`に書いてあり、満たさない版では`npm install`が失敗する（`.npmrc`の`engine-strict=true`）。miseを使う場合は、`mise install`で`.node-version`・`rust-toolchain.toml`の版が入る。

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

# ブラウザ確認用（Tauri IPCをモック、ポート1430）
npm run dev:mock

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

# Markdownのlint（markdownlint）
npm run lint:md
```

### テスト

```bash
# フロントエンドの単体テスト（Vitest）
npm test

# Rustのテスト
cd src-tauri && cargo test
```

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) + [ESLint](https://marketplace.visualstudio.com/items?itemName=dbaeumer.vscode-eslint) + [Prettier](https://marketplace.visualstudio.com/items?itemName=esbenp.prettier-vscode) + [markdownlint](https://marketplace.visualstudio.com/items?itemName=DavidAnson.vscode-markdownlint)

## 詳細ドキュメント

- [Design Doc](docs/design-doc.md) - 設計ドキュメントのエントリポイント
- [詳細設計](docs/design/detailed-design.md) - データモデル・Tauriコマンド仕様
- [実装方針](docs/design/implementation.md) - ファイル配置・コーディング規約・開発運用
- [設計概要](docs/design/architecture.md) - コンポーネント構成・データフロー
- [概要](docs/design/overview.md) - 背景・目的・スコープ
