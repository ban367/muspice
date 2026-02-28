# 設計判断: 代替案・トレードオフ

## ADR-001: 状態管理はハイブリッド構成を採用

### 背景

UIの即時反映、サーバー状態キャッシュ、バックエンド永続状態を単一手段で扱うと責務が混在しやすい。

### 採用

- UI状態: Svelte Stores
- データ取得/キャッシュ: TanStack Query
- 永続状態: Tauri `AppState`

### トレードオフ

- 利点: 責務分離が明確で再取得戦略を制御しやすい
- 欠点: 状態の置き場所を誤ると実装が複雑化する

## ADR-002: メタデータ更新は「DBのみ」と「DB+ファイル」を分離

### 背景

タグ書き込みはファイルI/O失敗やフォーマット差異の影響を受けるため、常に同一フローだと運用が硬直する。

### 採用

- `update_track_metadata`: DBのみ更新
- `update_track_metadata_with_file`: DB+ファイルタグ更新

### トレードオフ

- 利点: ユースケースに応じて安全側/同期側を選べる
- 欠点: 呼び分けの理解が必要で、仕様説明が不足すると誤使用の余地がある

## ADR-003: 検索はFTS5優先 + LIKEフォールバック

### 背景

全文検索の高速性を活かしつつ、FTSテーブル未整備や例外時にも検索不能にしない必要がある。

### 採用

- `tracks_fts MATCH` を優先
- 失敗時は `LIKE` に自動フォールバック

### トレードオフ

- 利点: 平常時の高速性と障害時の可用性を両立
- 欠点: 実装が単純な単一検索方式より複雑になる

## ADR-004: ドキュメントを`docs/design-doc.md`起点へ一本化

### 背景

従来の `docs/tech-stack.md` など分散構成は、実装更新時に同期漏れが発生しやすかった。

### 採用

- エントリポイントを `docs/design-doc.md` に統一
- 実装時の優先参照を `detailed-design.md` / `implementation.md` に固定
- 旧ドキュメント（`docs/tech-stack.md`, `docs/architecture.md`, `docs/features.md`, `docs/conventions.md`, `docs/development.md`）は削除

### トレードオフ

- 利点: 参照経路が単純化し、実装時の探索コストと乖離リスクを低減
- 欠点: 旧パスへの直接参照は移行時に修正が必要

## 参考資料

- `src-tauri/src/lib.rs` - コマンド登録とアプリ初期化
- `src-tauri/src/db.rs` - DBスキーマ/FTS5/マイグレーション
- `src-tauri/src/commands/*` - コマンドI/Fの実装
- `src/lib/queries/*.ts` - フロントエンドからの呼び出し仕様
