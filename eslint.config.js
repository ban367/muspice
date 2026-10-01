import js from '@eslint/js';
import ts from '@typescript-eslint/eslint-plugin';
import tsParser from '@typescript-eslint/parser';
import svelte from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import prettier from 'eslint-config-prettier';
import globals from 'globals';

export default [
  js.configs.recommended,
  {
    files: ['**/*.ts', '**/*.tsx'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module'
      },
      globals: {
        ...globals.browser,
        ...globals.es2022
      }
    },
    plugins: {
      '@typescript-eslint': ts
    },
    rules: {
      ...ts.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_'
        }
      ],
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/explicit-function-return-type': 'off',
      '@typescript-eslint/explicit-module-boundary-types': 'off'
    }
  },
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tsParser,
        ecmaVersion: 2022,
        sourceType: 'module'
      },
      globals: {
        ...globals.browser,
        ...globals.es2022
      }
    },
    plugins: {
      svelte
    },
    rules: {
      ...svelte.configs.recommended.rules,
      'svelte/no-unused-svelte-ignore': 'warn',
      'svelte/valid-compile': 'error',
      'no-undef': 'off',
      'no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_'
        }
      ]
    }
  },
  {
    files: ['**/*.ts', '**/*.svelte'],
    rules: {
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
  prettier
];
