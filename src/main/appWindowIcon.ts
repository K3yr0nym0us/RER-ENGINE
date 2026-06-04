import { app } from 'electron'
import fs from 'fs'
import path from 'path'

/** Icono de ventana (barra de título / taskbar), alineado con la ventana principal. */
export function resolveAppWindowIcon(): string | undefined {
  const iconPath = path.join(app.getAppPath(), 'src/resources/RER-ENGINE-LOGO.png')
  return fs.existsSync(iconPath) ? iconPath : undefined
}
