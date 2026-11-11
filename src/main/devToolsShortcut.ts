import type { BrowserWindow } from 'electron'

/**
 * Alterna DevTools con el atajo clásico de Electron (Ctrl+Shift+I / Cmd+Shift+I / F12).
 *
 * La app quita el menú de aplicación, así que registramos el atajo por ventana
 * en `before-input-event` para que funcione solo cuando esa ventana tiene foco.
 */
export function registerDevToolsShortcut(win: BrowserWindow): void {
  win.webContents.on('before-input-event', (event, input) => {
    if (input.type !== 'keyDown') return
    const commandOrControl = input.control || input.meta
    const toggle =
      (input.shift && commandOrControl && input.key.toLowerCase() === 'i')
      || input.key === 'F12'
    if (!toggle) return

    event.preventDefault()
    if (win.webContents.isDevToolsOpened()) {
      win.webContents.closeDevTools()
    } else {
      win.webContents.openDevTools({ mode: 'detach' })
    }
  })
}
