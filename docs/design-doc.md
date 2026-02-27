# Design Doc

MuspiceのDesign Docは、実装判断の根拠と制約を一元管理するための設計ドキュメント群です。

## 使い方（参照順）

実装タスクでは以下の順に参照します。

1. [詳細設計](design/detailed-design.md) - データモデル・Tauriコマンド・バリデーション仕様
2. [実装方針](design/implementation.md) - ディレクトリ構成・規約・開発/テスト運用
3. [設計概要](design/architecture.md) - 全体アーキテクチャとデータフローの確認が必要な場合
4. [概要](design/overview.md) - 背景やスコープの文脈確認が必要な場合

## ドキュメント構成

| ファイル                                               | 内容                                                            | 参照優先度 |
| ------------------------------------------------------ | --------------------------------------------------------------- | ---------- |
| [design/detailed-design.md](design/detailed-design.md) | データモデル・DBスキーマ・Tauriコマンド仕様・エラーハンドリング | 高         |
| [design/implementation.md](design/implementation.md)   | 技術スタック・ファイル配置・コーディング規約・テスト方針        | 高         |
| [design/architecture.md](design/architecture.md)       | コンポーネント構成・責務分離・データフロー                      | 中         |
| [design/non-functional.md](design/non-functional.md)   | パフォーマンス・セキュリティ・可用性                            | 中         |
| [design/overview.md](design/overview.md)               | 背景・目的・スコープ・機能一覧                                  | 低         |
| [design/decisions.md](design/decisions.md)             | 設計判断・不採用案・トレードオフ・移行履歴                      | 低         |

## 更新ルール

設計の意図・判断・制約に変更が入る場合は、実装と同時に以下を更新します。

- データモデル・APIの変更: `docs/design/detailed-design.md`
- ディレクトリ構成・技術スタック・規約の変更: `docs/design/implementation.md`
- コンポーネント構成・データフローの変更: `docs/design/architecture.md`
- 採用しなかった代替案・トレードオフの追加: `docs/design/decisions.md`

実装とドキュメントの乖離を見つけた場合は、ドキュメントを実装の実態に合わせて修正します。
