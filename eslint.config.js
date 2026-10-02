import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import { defineConfig } from 'eslint/config';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';
import svelteConfig from './svelte.config.js';

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
          message: '$lib/utils/dialog の confirmDestructive を await して使用してください。'
        }
      ]
    }
  },
  {
    // コンポーネント・ページからコマンドを直接呼ばない。キャッシュの無効化とエラー通知を
    // $lib/queries に集約するため（型・eventsのimportは許可する）
    files: ['src/lib/components/**/*.svelte', 'src/routes/**/*.svelte', 'src/routes/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          paths: [
            {
              name: '$lib/bindings',
              importNames: ['commands'],
              message: '$lib/queries のクエリ・ミューテーションを経由して呼び出してください。'
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
        extraFileExtensions: ['.svelte'],
        svelteConfig
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
