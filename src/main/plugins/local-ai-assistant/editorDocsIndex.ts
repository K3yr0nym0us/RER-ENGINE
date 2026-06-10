import fs from 'fs'
import path from 'path'
import { app } from 'electron'

const GUIDE_FILES: Record<'es' | 'en', string> = {
  es: 'AI_Assistant_Editor_Guide.es.prompt.txt',
  en: 'AI_Assistant_Editor_Guide.en.prompt.txt',
}

const FALLBACK_CONTEXT: Record<'es' | 'en', string> = {
  es: `RER-ENGINE: editor 2D/3D. Panel izquierdo = acordeones. Propiedades = modal al clic en entidad. Guardar en barra superior.`,
  en: `RER-ENGINE editor: Scenes, World, Camera, Resources, Create entity, UI, Tools, Controls in left sidebar.
Entity properties open when clicking an entity. Save via top bar.`,
}

function getDocsRoot(): string | null {
  const candidates = [
    path.join(process.cwd(), 'docs'),
    path.join(app.getAppPath(), '..', '..', 'docs'),
    path.join(app.getAppPath(), 'docs'),
  ]
  for (const dir of candidates) {
    if (fs.existsSync(dir)) return dir
  }
  return null
}

const cachedByLocale: Partial<Record<'es' | 'en', string>> = {}

function loadGuideForLocale(locale: 'es' | 'en'): string {
  const docsRoot = getDocsRoot()
  if (!docsRoot) return FALLBACK_CONTEXT[locale]

  const filePath = path.join(docsRoot, GUIDE_FILES[locale])
  try {
    const raw = fs.readFileSync(filePath, 'utf8').trim()
    if (raw) return raw
  } catch {
    // fallback below
  }

  return FALLBACK_CONTEXT[locale]
}

/** Recarga el prompt desde disco (p. ej. tras cambios en desarrollo). */
export function refreshAiEditorGuideCache(): void {
  for (const key of Object.keys(cachedByLocale) as Array<'es' | 'en'>) {
    delete cachedByLocale[key]
  }
}

/** Solo el archivo de guía del idioma activo (menos tokens → respuesta más rápida). */
export function buildSystemContext(_userQuery?: string, locale: 'en' | 'es' = 'en'): string {
  if (cachedByLocale[locale] == null) {
    cachedByLocale[locale] = loadGuideForLocale(locale)
  }
  return cachedByLocale[locale]!
}
