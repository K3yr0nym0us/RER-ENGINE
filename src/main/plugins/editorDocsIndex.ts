import fs from 'fs'
import path from 'path'
import { app } from 'electron'

/** Contexto compacto para el asistente IA local. */
const AI_EDITOR_GUIDE_FILE = 'AI_Assistant_Editor_Guide.prompt.txt'

const LOCALE_MARKERS = {
  es: '---LOCALE:es---',
  en: '---LOCALE:en---',
  rules: '---LOCALE:rules---',
} as const

const FALLBACK_CONTEXT = `RER-ENGINE editor: Scenes, World, Camera, Resources, Create entity, UI, Tools, Controls in left sidebar.
Entity properties open when clicking an entity. Save via top bar.`

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

function extractSection(raw: string, marker: string, nextMarkers: string[]): string {
  const start = raw.indexOf(marker)
  if (start === -1) return ''

  const contentStart = start + marker.length
  let end = raw.length
  for (const next of nextMarkers) {
    const idx = raw.indexOf(next, contentStart)
    if (idx !== -1 && idx < end) end = idx
  }

  return raw.slice(contentStart, end).trim()
}

function parsePromptFile(raw: string): { es: string; en: string; rules: string } {
  return {
    es: extractSection(raw, LOCALE_MARKERS.es, [LOCALE_MARKERS.en, LOCALE_MARKERS.rules]),
    en: extractSection(raw, LOCALE_MARKERS.en, [LOCALE_MARKERS.rules]),
    rules: extractSection(raw, LOCALE_MARKERS.rules, []),
  }
}

let cachedSections: { es: string; en: string; rules: string } | null = null

function loadPromptSections(): { es: string; en: string; rules: string } {
  const docsRoot = getDocsRoot()
  if (!docsRoot) {
    return { es: FALLBACK_CONTEXT, en: FALLBACK_CONTEXT, rules: '' }
  }

  const full = path.join(docsRoot, AI_EDITOR_GUIDE_FILE)
  try {
    const raw = fs.readFileSync(full, 'utf8').trim()
    if (!raw) return { es: FALLBACK_CONTEXT, en: FALLBACK_CONTEXT, rules: '' }
    return parsePromptFile(raw)
  } catch {
    return { es: FALLBACK_CONTEXT, en: FALLBACK_CONTEXT, rules: '' }
  }
}

function getPromptSections(): { es: string; en: string; rules: string } {
  if (cachedSections == null) {
    cachedSections = loadPromptSections()
  }
  return cachedSections
}

/** Recarga el prompt desde disco (p. ej. tras cambios en desarrollo). */
export function refreshAiEditorGuideCache(): void {
  cachedSections = null
}

export function buildSystemContext(_userQuery?: string, locale: 'en' | 'es' = 'en'): string {
  const sections = getPromptSections()
  const localeBlock = locale === 'es' ? sections.es : sections.en
  const rules = sections.rules

  return [localeBlock, rules].filter(Boolean).join('\n\n')
}
