# コーディング規約

## エラーハンドリング

### エラータイプ

1. `FileNotFoundError`: 音楽ファイルが見つからない
2. `UnsupportedFormatError`: サポートされていないファイル形式
3. `MetadataExtractionError`: メタデータ抽出失敗
4. `DatabaseError`: データベース操作エラー
5. `PlaybackError`: 音楽再生エラー
6. `ValidationError`: データバリデーションエラー

### エラーハンドリング戦略

- Rustコマンドは日本語のユーザーフレンドリーなエラーメッセージを持つ`Result<T, String>`を返す
- フロントエンドは一貫したエラー表示のため`handleError`ユーティリティを使用
- すべてのデータベース操作は適切な場合トランザクションでラップ
- グローバルエラーハンドラーで未処理エラーをキャッチしてログ出力
- エラーリカバリー機能（可能な場合は自動復旧）
- すべてのエラーをログファイルに記録
- エラートースト通知でユーザーに通知

## バリデーション

- 入力バリデーションはデータベース操作の前にRustで実行
- `validation.rs`のバリデーションユーティリティ: パス検証、ID検証、文字列長制限
- データ整合性のためのメタデータバリデーション
- フロントエンドでも入力サニタイゼーションを実施
- SQLインジェクション対策

## コードスタイルと命名規則

### ファイル命名

- **Svelte**: PascalCase（例: `Library.svelte`, `Player.svelte`）
- **TypeScript**: camelCase（例: `audioPlayer.ts`, `metadataUtils.ts`）
- **Rust**: snake_case（例: `music_library.rs`, `playlist_manager.rs`）

### コード命名

- **変数・関数**: camelCase (TypeScript), snake_case (Rust)
- **クラス・型**: PascalCase（両言語共通）
- **定数**: UPPER_SNAKE_CASE（両言語共通）

### コメントとドキュメント

- **コードコメント**: 日本語で記述（Rust、TypeScriptともに）
- **エラーメッセージ**: ユーザー向けメッセージは日本語必須
- **ログ出力**: デバッグ情報は英語可、ユーザー向けメッセージは日本語必須
- **技術用語**: 不自然な日本語訳は英語併記

### コード規約

- Rust: 標準的なRust規約、日本語での詳細なコメント
- TypeScript: Strictモード有効、型安全なTauri呼び出し
- 利用するファイルに合わせてフォーマットを必ず適用

## 状態管理

- Mutex-wrappedされたConnectionを持つ`AppState`経由での共有データベース状態
- 現在再生中のトラックは`AppState.current_track_id`に保存
- バックエンド呼び出しを減らすためのTanStack Queryによるフロントエンドキャッシング

## Rustコーディング規約（Clippy準拠）

### 範囲チェック

```rust
// NG: 手動での範囲チェック
if rating < 0 || rating > 5 { ... }

// OK: RangeInclusive::containsを使用
if !(0..=5).contains(&rating) { ... }
```

### HashMap/Vecのデフォルト値挿入

```rust
// NG: or_insert_withでコンストラクタを渡す
map.entry(key).or_insert_with(Vec::new).push(value);
map.entry(key).or_insert_with(HashMap::new);

// OK: or_defaultを使用
map.entry(key).or_default().push(value);
map.entry(key).or_default();
```

### 文字列置換

```rust
// NG: 同じ置換値で連続するreplace
text.replace(';', "").replace('\'', "").replace('"', "")

// OK: 配列で一括置換
text.replace([';', '\'', '"'], "")
```

## TailwindCSS規約

### @applyディレクティブでの制限

コンポーネントの`<style>`ブロック内では、`app.css`で定義されたカスタムクラスは`@apply`で使用できません。

```css
/* NG: カスタムクラスは@applyで使用不可 */
.my-class {
  @apply text-truncate; /* app.cssのカスタムクラス → エラー */
}

/* OK: Tailwind組み込みクラスを使用 */
.my-class {
  @apply truncate; /* Tailwind組み込み */
}
```

### @referenceディレクティブ

コンポーネントでTailwindクラスを使用する場合、スタイルブロックの先頭に`@reference`を追加:

```svelte
<style>
  @reference "../../../app.css";
  .my-class {
    @apply flex items-center;
  }
</style>
```

## UIコンポーネント規約

### ライブラリヘッダー（LibraryHeader）

全てのライブラリビュー（曲、アルバム、アーティスト、ジャンル）で統一されたヘッダーコンポーネントを使用:

```svelte
<LibraryHeader
  title="タイトル"
  count={itemCount}
  countUnit="曲"
  searchPlaceholder="検索..."
  {searchTerm}
  onSearchInput={handleSearchInput}
  onSearchClear={clearSearch}
  {displayMode}
  onDisplayModeChange={handleDisplayModeChange}
  showGridMode={true}
  showListMode={true}
  showCardSizeSlider={true}
/>
```

**レイアウト構成**:

- 左側: タイトル、カウント、表示切り替えボタン、カードサイズスライダー
- 右側: 検索バー（固定位置）

**表示切り替えボタンの動作**:

- グリッド/リストボタンは常に表示
- 利用不可の場合は`disabled`状態にする（レイアウトのズレを防止）

### 表示モード（displayMode）

```typescript
type DisplayMode = 'grid' | 'list';
```

- 全てのライブラリビューでグリッド/リスト表示を切り替え可能
- `displayMode`は親コンポーネントで管理し、子コンポーネントにpropsとして渡す
- 子コンポーネント内で独自のヘッダーを持たない（重複を避ける）

## Svelte 5 パターン

### Runes構文

```svelte
<script lang="ts">
  // Props定義
  interface Props {
    title: string;
    count?: number;
    onAction?: (value: string) => void;
  }
  let { title, count = 0, onAction }: Props = $props();

  // リアクティブ状態
  let searchTerm = $state('');
  let items = $state<Item[]>([]);

  // 派生値
  const filteredItems = $derived(items.filter((item) => item.name.includes(searchTerm)));

  // 派生値（複雑なロジック）
  const processedData = $derived.by(() => {
    if (!items.length) return null;
    return items.map((item) => ({ ...item, processed: true }));
  });

  // エフェクト
  $effect(() => {
    console.log('searchTerm changed:', searchTerm);
  });
</script>
```
