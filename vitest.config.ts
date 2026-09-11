import { resolve } from 'node:path'
import { defineConfig } from 'vitest/config'

const engineRustGlobs = [
  'src/main/Engine/target/**',
  'src/main/Engine/engine_2d/**',
  'src/main/Engine/engine_3d/**',
  'src/main/Engine/engine_shared/**',
  'src/main/Engine/engine_ipc_common/**',
  'src/main/Engine/Models/**',
  'src/main/Engine/assets/**',
] as const

export default defineConfig({
  test: {
    // Default: Node (main / preload / shared-types / pure helpers).
    // Renderer DOM tests: name files `*.test.tsx` under src/renderer (jsdom).
    environment: 'node',
    environmentMatchGlobs: [['src/renderer/**/*.{test,spec}.tsx', 'jsdom']],
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    exclude: [
      '**/node_modules/**',
      '**/out/**',
      '**/dist/**',
      '**/coverage/**',
      ...engineRustGlobs,
    ],
    clearMocks: true,
    restoreMocks: true,
    mockReset: true,
    passWithNoTests: false,
    reporters: ['default'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov', 'html'],
      reportsDirectory: './coverage',
      include: [
        'src/main/**/*.{ts,tsx}',
        'src/preload/**/*.{ts,tsx}',
        'src/renderer/src/**/*.{ts,tsx}',
        'src/shared-types/**/*.{ts,tsx}',
      ],
      exclude: [
        ...engineRustGlobs,
        'src/test/**',
        '**/*.{test,spec}.{ts,tsx}',
        '**/env.d.ts',
        '**/*.d.ts',
      ],
    },
  },
  resolve: {
    alias: {
      '@renderer': resolve('src/renderer/src'),
      '@shared-types': resolve('src/shared-types/types.ts'),
      '@modal': resolve('src/renderer/src/context/ModalContext.tsx'),
      '@engine': resolve('src/renderer/src/context/useContextEngine.tsx'),
      '@hooks': resolve('src/renderer/src/hooks/index.ts'),
      '@components': resolve('src/renderer/src/components/index.ts'),
      '@context': resolve('src/renderer/src/context/index.ts'),
    },
  },
})
