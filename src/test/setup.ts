/**
 * Shared Vitest setup for Electron / React / TypeScript unit tests.
 * Keep side effects minimal; prefer per-file mocks for Electron APIs.
 */
import { afterEach, vi } from 'vitest'

afterEach(() => {
  vi.clearAllMocks()
  vi.restoreAllMocks()
})
