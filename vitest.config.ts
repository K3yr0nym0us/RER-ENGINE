import { resolve } from 'path'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    exclude: [
      '**/node_modules/**',
      '**/out/**',
      '**/dist/**',
      'src/main/Engine/target/**',
      'src/main/Engine/engine_2d/**',
      'src/main/Engine/engine_3d/**',
      'src/main/Engine/engine_shared/**',
      'src/main/Engine/engine_ipc_common/**',
    ],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov'],
      reportsDirectory: './coverage',
      include: [
        'src/main/**/*.{ts,tsx}',
        'src/preload/**/*.{ts,tsx}',
        'src/renderer/src/**/*.{ts,tsx}',
        'src/shared-types/**/*.{ts,tsx}',
      ],
      exclude: [
        'src/main/Engine/target/**',
        'src/main/Engine/engine_2d/**',
        'src/main/Engine/engine_3d/**',
        'src/main/Engine/engine_shared/**',
        'src/main/Engine/engine_ipc_common/**',
        'src/main/Engine/Models/**',
        'src/main/Engine/assets/**',
        '**/*.{test,spec}.{ts,tsx}',
        '**/env.d.ts',
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
