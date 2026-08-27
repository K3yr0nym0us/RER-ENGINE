/**
 * Monaco offline for Electron: bundle `monaco-editor` from npm (no CDN).
 * Import once before any `@monaco-editor/react` Editor mounts.
 */
import * as monaco from 'monaco-editor'
import { loader } from '@monaco-editor/react'
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker'

import 'monaco-editor/min/vs/editor/editor.main.css'

declare global {
  interface Window {
    MonacoEnvironment?: {
      getWorker: (workerId: string, label: string) => Worker
    }
  }
}

self.MonacoEnvironment = {
  getWorker() {
    // Rhai is a custom language; only the core editor worker is required.
    return new editorWorker()
  },
}

loader.config({ monaco })
