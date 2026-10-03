import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import { defineConfig } from 'eslint/config';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

// 共有する状態はRunes（*.svelte.tsの$state）で書く。svelte/storeは使わない
// （規約はdocs/design/implementation.mdの「共有する状態」）
const NO_SVELTE_STORE = {
  name: 'svelte/store',
  message: '共有する状態は *.svelte.ts の $state で実装してください（svelte/store は使わない）。'
};

export default defineConfig(
  {
    ignores: [
      'build/**',
      '.svelte-kit/**',
      'dist/**',
      'node_modules/**',
      'src-tauri/target/**',
      // tauri-spectaによる生成物
      'src/lib/bindings.ts',
      '*.config.js',
      '*.config.ts'
    ]
  },
  js.configs.recommended,
  ts.configs.recommended,
  svelte.configs.recommended,
  prettier,
  svelte.configs.prettier,
  {
    languageOptions: {
      globals: {
        ...globals.browser
      }
    },
    rules: {
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_'
        }
      ],
      '@typescript-eslint/no-explicit-any': 'warn',
      // window.confirmはdialogプラグインにより非同期関数へ置き換えられており、
      // 同期的に呼ぶと戻り値のPromiseが常にtrue扱いになる
      'no-restricted-globals': [
        'error',
        {
          name: 'confirm',
          message: '#lib/utils/dialog.svelte.js の confirmDestructive を await して使用してください。'
        },
        {
          // macOSのWebView（wry）はwindow.promptを実装しておらず、常にnullを返す
          name: 'prompt',
          message: '#lib/utils/dialog.svelte.js の promptText を await して使用してください。'
        }
      ],
      'no-restricted-imports': ['error', { paths: [NO_SVELTE_STORE] }]
    }
  },
  {
    // コンポーネント・ページからコマンドを直接呼ばない。キャッシュの無効化とエラー通知を
    // #lib/queries に集約するため（型・eventsのimportは許可する）
    files: ['src/lib/components/**/*.svelte', 'src/routes/**/*.svelte', 'src/routes/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          paths: [
            // ルールの設定は上書きされるため、全体の制限もここに含める
            NO_SVELTE_STORE,
            {
              name: '#lib/bindings.js',
              importNames: ['commands'],
              message: '#lib/queries のクエリ・ミューテーションを経由して呼び出してください。'
            }
          ]
        }
      ]
    }
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: {
      parserOptions: {
        parser: ts.parser,
        extraFileExtensions: ['.svelte']
      }
    },
    rules: {
      // TypeScriptの型検査と重複するコアルールを無効化する（typescript-eslint推奨）
      // no-undefは型名を、no-unused-varsはコールバック型の引数名を誤検出する
      'no-undef': 'off',
      'no-unused-vars': 'off',
      'svelte/no-unused-svelte-ignore': 'warn'
    }
  }
);
