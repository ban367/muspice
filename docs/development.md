# 開発ガイド

## 開発コマンド

### セットアップ

```bash
npm install                    # フロントエンド依存関係をインストール
cd src-tauri && cargo build   # Rustバックエンドをビルド
```

### 開発

```bash
npm run tauri dev             # Tauriアプリを開発モードで実行（推奨）
npm run dev                   # フロントエンドのみ実行（ポート1420）
```

### テスト

```bash
cd src-tauri && cargo test    # Rustユニットテストを実行
```

### コード品質

```bash
npm run check                 # TypeScript型チェック
npm run lint                  # ESLintを実行
npm run lint:fix              # ESLint自動修正
npm run format                # Prettierでフォーマット
npm run format:check          # フォーマットチェック
```

### ビルド

```bash
npm run build                 # フロントエンドをビルド
npm run tauri build           # 本番用Tauriアプリをビルド
```

## テスト戦略

### 手動テスト項目

**音楽インポート**:

- フォルダ選択とインポート進行状況
- サポートされているファイル形式の確認
- サブディレクトリの再帰的スキャン
- メタデータ抽出とデフォルト値
- 重複ファイル処理（スキップ/置換）

**プレイリスト**:

- 新規作成、トラック追加、並び替え、削除
- ドラッグ&ドロップ機能
- プレイリスト永続化

**音楽再生**:

- 基本的な再生コントロール
- シーク機能、音量調整
- プレイリスト連続再生
- トラック情報表示

**検索・フィルタリング**:

- リアルタイム検索とハイライト
- フィルター適用と組み合わせ
- 検索結果なしメッセージ

**メタデータ編集**:

- 単一/複数トラック編集
- バリデーションエラー
- 即座のライブラリ更新

**エラーハンドリング**:

- ファイルが見つからない場合
- サポートされていない形式
- データベースエラー

**パフォーマンス**:

- 1000曲以上のライブラリでの動作
- スクロールの滑らかさ
- 検索速度

### 自動テスト

- **Rustユニットテスト**: `cargo test`でサービス層とユーティリティをテスト
- カバレッジ目標: Rustサービス層90%以上

## CI/CD

### GitHub Actions ワークフロー

CIは`.github/workflows/ci.yml`で定義されており、以下の3つのジョブで構成されます:

1. **Frontend Check**: TypeScript型チェック、ESLint、Prettierフォーマットチェック
2. **Backend Check**: Rustフォーマットチェック、Clippy、ユニットテスト
3. **Build Test**: PRのみで実行される完全ビルドテスト

### プッシュ前の必須チェック

```bash
# フロントエンド
npm run check        # TypeScript型チェック
npm run lint         # ESLint
npm run format       # Prettierフォーマット

# バックエンド
cargo fmt --manifest-path src-tauri/Cargo.toml           # Rustフォーマット
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings  # Clippy
cargo test --manifest-path src-tauri/Cargo.toml          # テスト
```

### システム依存関係（Ubuntu/CI環境）

```bash
# 必須パッケージ（ci.ymlに定義済み）
libwebkit2gtk-4.1-dev
librsvg2-dev
patchelf
libgtk-3-dev
libayatana-appindicator3-dev  # 注意: libappindicator3-devとは競合するため使用不可
```

### Node.jsバージョン

- **Node.js 24**を使用（ci.ymlで指定）

## セキュリティ考慮事項

### Tauri固有のセキュリティ設定

- allowlistでファイルシステムアクセスを制限
- ファイルスコープ設定（`$AUDIO`、`$DATA`）
- ユーザーが選択したディレクトリのみアクセス許可
- パストラバーサル攻撃の防止

### データ保護

- ローカルデータのみ使用
- 外部サーバーへのデータ送信なし
- すべてのデータはプラットフォーム固有のアプリデータディレクトリに保存

## Gitワークフロー

### ブランチ戦略

- `main`: 安定版
- `feature/*`: 機能開発ブランチ

### コミット規約

```bash
# 形式
<type>: <description>

# type一覧
feat:     新機能
fix:      バグ修正
refactor: リファクタリング
style:    フォーマット変更（コード動作に影響なし）
docs:     ドキュメント更新
test:     テスト追加・修正
chore:    ビルド設定等の変更

# 例
feat: add list view to album grid
fix: resolve Clippy warnings
refactor: extract shared LibraryHeader component
```

### PRチェックリスト

1. `npm run format` でコードフォーマット
2. `npm run check` でTypeScript型チェック
3. `npm run lint` でESLintチェック
4. `cargo fmt` でRustコードフォーマット
5. `cargo clippy -- -D warnings` でClippyチェック
6. `cargo test` でテスト実行
7. 全てのCIチェックがパスすることを確認

## 開発のベストプラクティス

- 新機能追加時は既存の状態管理パターンに従うこと
- エラーメッセージは常に日本語でユーザーフレンドリーに
- データベース操作は必ずトランザクション内で実行
- パフォーマンスに影響する変更はバッチ処理を検討
- セキュリティ関連の変更は慎重に review
- コミット前に`npm run check`と`cargo test`を実行

## トラブルシューティング

### よくあるCI失敗パターン

| エラー                               | 原因                         | 解決方法                             |
| ------------------------------------ | ---------------------------- | ------------------------------------ |
| `Cannot apply unknown utility class` | カスタムクラスを@applyで使用 | Tailwind組み込みクラスに変更         |
| `manual_range_contains`              | 手動範囲チェック             | `!(range).contains(&value)`に変更    |
| `unwrap_or_default`                  | `or_insert_with(Vec::new)`   | `.or_default()`に変更                |
| `collapsible_str_replace`            | 連続replace呼び出し          | 配列で一括置換                       |
| パッケージ競合                       | libappindicator3-dev         | libayatana-appindicator3-devのみ使用 |

### ローカルでClippy実行不可の場合

ローカル環境にpkg-config等がない場合、Clippyをローカルで実行できないことがあります。
その場合は変更をコミット・プッシュしてCIで確認してください。
