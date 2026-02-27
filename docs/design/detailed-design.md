<!-- このファイルは docs/design-doc.md の一部です -->

# 詳細設計: データモデル・API仕様・エラーハンドリング

## 5. 詳細設計

<!-- AI実装時の主要参照章。
     型定義・スキーマ・APIインターフェースをコードブロックで明記することで、
     AIがそのまま実装の入力として活用できる。
     曖昧さを排除し、実装の根拠を提供する。 -->

### データモデル

```typescript
// 例: TypeScriptの型定義
interface User {
  id: string;
  name: string;
  email: string;
  createdAt: Date;
}

interface CreateUserInput {
  name: string;
  email: string;
}
```

```sql
-- 例: テーブル定義
CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(255) NOT NULL,
  email VARCHAR(255) UNIQUE NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
```

### API設計

#### エンドポイント一覧

| メソッド | パス              | 説明             | 認証 |
| -------- | ----------------- | ---------------- | ---- |
| GET      | /api/v1/users     | ユーザー一覧取得 | 必要 |
| POST     | /api/v1/users     | ユーザー作成     | 必要 |
| GET      | /api/v1/users/:id | ユーザー詳細取得 | 必要 |

#### リクエスト/レスポンス例

```json
// POST /api/v1/users
// Request
{
  "name": "山田太郎",
  "email": "yamada@example.com"
}

// Response 201 Created
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "山田太郎",
  "email": "yamada@example.com",
  "createdAt": "2024-01-01T00:00:00Z"
}
```

### 処理ロジック

（主要な処理フローや業務ロジックを記述する）

### エラーハンドリング

| エラーコード    | HTTPステータス | 原因                         | 対処                                       |
| --------------- | -------------- | ---------------------------- | ------------------------------------------ |
| USER_NOT_FOUND  | 404            | 指定IDのユーザーが存在しない | クライアントに適切なエラーメッセージを返す |
| EMAIL_DUPLICATE | 409            | メールアドレスが重複         | エラーメッセージでメール重複を通知         |
