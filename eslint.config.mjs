import eslint from '@eslint/js'
import globals from 'globals'
import reactPlugin from 'eslint-plugin-react'
import reactHooks from 'eslint-plugin-react-hooks'
import sonarjs from 'eslint-plugin-sonarjs'
import tseslint from 'typescript-eslint'

const sharedIgnores = [
  '**/node_modules/**',
  '**/out/**',
  '**/dist/**',
  '**/coverage/**',
  '**/.yarn/**',
  '**/.eslintcache',
  'src/main/Engine/target/**',
  'src/main/Engine/engine_2d/**',
  'src/main/Engine/engine_3d/**',
  'src/main/Engine/engine_shared/**',
  'src/main/Engine/engine_ipc_common/**',
  'src/main/Engine/Models/**',
  'src/main/Engine/assets/**',
  '**/*.save',
  'eslint.config.mjs',
  'scripts/**',
  'eslint-report.json',
  'lint-report.txt',
]

export default tseslint.config(
  { ignores: sharedIgnores },

  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  sonarjs.configs.recommended,

  {
    rules: {
      // --- TypeScript (strict, non-stylistic) ---
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
      '@typescript-eslint/ban-ts-comment': [
        'error',
        {
          'ts-ignore': true,
          'ts-nocheck': true,
          'ts-expect-error': 'allow-with-description',
          minimumDescriptionLength: 10,
        },
      ],
      '@typescript-eslint/consistent-type-imports': [
        'error',
        {
          prefer: 'type-imports',
          fixStyle: 'inline-type-imports',
          // Allow `import('mod').Type` in ambient Window/API typings.
          disallowTypeAnnotations: false,
        },
      ],
      '@typescript-eslint/no-import-type-side-effects': 'error',
      '@typescript-eslint/no-non-null-assertion': 'error',

      // Cyclomatic complexity is enforced on *new code* by SonarCloud Quality Gate.
      // Local ESLint keeps max-depth for readability without blocking legacy IPC handlers.
      'max-depth': ['error', { max: 5 }],

      // --- Dangerous JS ---
      'no-eval': 'error',
      'no-implied-eval': 'error',
      'no-new-func': 'error',
      'no-var': 'error',
      'prefer-const': 'error',
      eqeqeq: ['error', 'always', { null: 'ignore' }],
      // Conflicts with inline/separate type imports from the same module.
      'no-duplicate-imports': 'off',
      'no-console': 'off',

      // SonarJS: keep bug/security-oriented; turn off high-noise legacy/style rules.
      // Cognitive complexity & nested conditionals are enforced on *new code* via SonarCloud QG.
      'sonarjs/cognitive-complexity': 'off',
      'sonarjs/no-nested-conditional': 'off',
      'sonarjs/function-return-type': 'off',
      'sonarjs/deprecation': 'off',
      'sonarjs/todo-tag': 'off',
      'sonarjs/no-commented-code': 'off',
      'sonarjs/redundant-type-aliases': 'off',
      'sonarjs/no-redundant-optional': 'off',
      'sonarjs/argument-type': 'off',
      'sonarjs/null-dereference': 'off',
      'sonarjs/pseudo-random': 'off',
      'sonarjs/super-linear-regex': 'off',
      'sonarjs/nested-control-flow': 'off',
      'sonarjs/no-nested-functions': 'off',
      'sonarjs/no-nested-template-literals': 'off',
      'sonarjs/void-use': 'off',
      'sonarjs/use-type-alias': 'off',
      'sonarjs/reduce-initial-value': 'off',
      'sonarjs/no-alphabetical-sort': 'off',
      'sonarjs/regex-complexity': 'off',
      'sonarjs/different-types-comparison': 'off',
    },
  },

  // Async/promise correctness (type-aware, limited rule set for speed)
  {
    files: ['src/**/*.{ts,tsx}', 'electron.vite.config.ts', 'vitest.config.ts'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': [
        'error',
        { checksVoidReturn: { attributes: false } },
      ],
      '@typescript-eslint/await-thenable': 'error',
    },
  },

  {
    files: [
      'src/main/**/*.{ts,tsx}',
      'src/preload/**/*.{ts,tsx}',
      'src/shared-types/**/*.{ts,tsx}',
      'electron.vite.config.ts',
      'vitest.config.ts',
    ],
    languageOptions: {
      globals: { ...globals.node },
    },
  },

  {
    files: ['src/renderer/**/*.{ts,tsx}'],
    ...reactPlugin.configs.flat.recommended,
    languageOptions: {
      ...reactPlugin.configs.flat.recommended.languageOptions,
      globals: { ...globals.browser },
    },
    settings: { react: { version: 'detect' } },
  },

  {
    files: ['src/renderer/**/*.{ts,tsx}'],
    ...reactPlugin.configs.flat['jsx-runtime'],
  },

  // React 18: classic hooks rules only (not React Compiler / React 19 extras)
  {
    files: ['src/renderer/**/*.{ts,tsx}'],
    plugins: { 'react-hooks': reactHooks },
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'error',
      'react/prop-types': 'off',
      'react/jsx-no-target-blank': 'error',
      'react/jsx-key': 'error',
      'react/no-danger': 'error',
      'react/no-unstable-nested-components': 'error',
    },
  },

  {
    files: ['**/*.{test,spec}.{ts,tsx}', 'src/**/__tests__/**/*.{ts,tsx}'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      complexity: 'off',
    },
  },
)
