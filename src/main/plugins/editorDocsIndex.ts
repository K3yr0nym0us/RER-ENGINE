import fs from 'fs'
import path from 'path'
import { app } from 'electron'

export interface DocChunk {
  id: string
  source: string
  keywords: string[]
  content: string
}

const BUNDLED_CHUNKS: DocChunk[] = [
  {
    id: 'sidebar-scenes',
    source: 'editor-ui',
    keywords: ['scene', 'escena', 'programming', 'programación', 'blueprint'],
    content:
      'Scenes accordion: create, rename, switch scenes. Programming sub-accordion opens visual scripting or Rhai scene scripts.',
  },
  {
    id: 'sidebar-resources',
    source: 'editor-ui',
    keywords: ['model', 'modelo', 'sprite', 'font', 'sound', 'resource', 'recurso', 'import'],
    content:
      'Resources accordion: import models (3D), sprites (2D), fonts, HUD images, sounds, backgrounds. Each category has its own sub-accordion.',
  },
  {
    id: 'sidebar-entities',
    source: 'editor-ui',
    keywords: ['entity', 'entidad', 'create', 'crear', 'character', 'player'],
    content:
      'Create entity accordion: spawn entities from sprites (2D) or models (3D). Select category (environment, character, object, etc.).',
  },
  {
    id: 'sidebar-ui',
    source: 'editor-ui',
    keywords: ['hud', 'ui', 'player', 'screen', 'pantalla'],
    content:
      'Player HUD accordion: create UI screens, add text/images/buttons, edit in viewport. Scope is player HUD for 3D projects.',
  },
  {
    id: 'sidebar-tools',
    source: 'editor-ui',
    keywords: ['tool', 'herramienta', 'blueprint', 'quick build', 'draw', 'dibujar'],
    content:
      'Tools accordion: Quick Build (blueprints), drawing tools for colliders and execution areas (2D), plane tool (3D).',
  },
  {
    id: 'rhai-scripting',
    source: 'docs/RHAI_API.yaml',
    keywords: ['script', 'rhai', 'code', 'código', 'on_start', 'update'],
    content:
      'Entity scripts use Rhai callbacks: on_start, update, on_press, on_keep. Scene scripts use on_scene_start, on_scene_tick. API: engine.move_to, engine.log, etc.',
  },
]

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

function loadYamlExcerpt(filePath: string, maxChars: number): string {
  try {
    const raw = fs.readFileSync(filePath, 'utf8')
    return raw.slice(0, maxChars)
  } catch {
    return ''
  }
}

export function buildDocChunks(): DocChunk[] {
  const chunks = [...BUNDLED_CHUNKS]
  const docsRoot = getDocsRoot()
  if (!docsRoot) return chunks

  const yamlFiles = [
    { file: 'RHAI_API.yaml', keywords: ['rhai', 'script', 'engine.', 'callback'] },
    { file: 'Programing_Model.yaml', keywords: ['visual', 'node', 'nodo', 'graph'] },
    { file: 'MODAL_ELECTRON.yaml', keywords: ['modal', 'window', 'ventana'] },
    { file: 'Save_Proyect_Model.yaml', keywords: ['save', 'guardar', 'project', 'manifest'] },
  ]

  for (const { file, keywords } of yamlFiles) {
    const full = path.join(docsRoot, file)
    const content = loadYamlExcerpt(full, 4_000)
    if (content) {
      chunks.push({
        id: `doc-${file}`,
        source: `docs/${file}`,
        keywords,
        content,
      })
    }
  }

  return chunks
}

export function searchDocChunks(query: string, limit = 4): DocChunk[] {
  const tokens = query
    .toLowerCase()
    .split(/\s+/)
    .filter((t) => t.length > 2)

  if (tokens.length === 0) return BUNDLED_CHUNKS.slice(0, limit)

  const chunks = buildDocChunks()
  const scored = chunks.map((chunk) => {
    const hay = `${chunk.id} ${chunk.keywords.join(' ')} ${chunk.content}`.toLowerCase()
    let score = 0
    for (const token of tokens) {
      if (hay.includes(token)) score += 1
    }
    return { chunk, score }
  })

  return scored
    .filter((s) => s.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((s) => s.chunk)
}

export function buildSystemContext(userQuery: string): string {
  const hits = searchDocChunks(userQuery, 4)
  if (hits.length === 0) {
    return 'You help users navigate the RER-ENGINE editor. Keep answers to 2-3 short sentences with one concrete UI step.'
  }
  const body = hits.map((h) => `[${h.source}]\n${h.content}`).join('\n\n---\n\n')
  return `You help users navigate the RER-ENGINE editor. Use only the context below. Keep answers to 2-3 short sentences with one concrete UI step.\n\n${body}`
}
