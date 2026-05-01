import { resolve } from 'path'
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
  },
  renderer: {
    resolve: {
      alias: {
        '@renderer': resolve('src/renderer/src'),
        '@shared-types': resolve('src/shared-types/types.ts'),
        '@modal': resolve('src/renderer/src/context/ModalContext.tsx'),
        '@engine': resolve('src/renderer/src/context/useContextEngine.tsx'),
      },
    },
    plugins: [react()],
  },
})
