import type { MsvcLocale } from './msvcRedistributable'
import { resolveMsvcMessageLocale } from './msvcRedistributable'

/** Locale for MSVC user messages (editor OS locale when Electron is available). */
export function resolvePluginUiLocale(): MsvcLocale {
  try {
    // Dynamic import keeps unit tests from requiring a ready Electron app.
    // eslint-disable-next-line @typescript-eslint/no-require-imports -- Electron main optional in tests
    const electron = require('electron') as typeof import('electron')
    return resolveMsvcMessageLocale(electron.app.getLocale())
  } catch {
    return resolveMsvcMessageLocale(process.env.LANG ?? process.env.LC_ALL)
  }
}
