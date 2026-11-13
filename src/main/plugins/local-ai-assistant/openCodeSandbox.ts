import crypto from 'node:crypto'
import fs from 'fs'
import path from 'path'

export interface WorkspacePlanFile {
  relativePath: string
  content: string
}

export interface WorkspacePlan {
  workspaceDir: string
  files: WorkspacePlanFile[]
  /** Directorio a copiar recursivamente dentro de workspace/docs/ai. */
  docsSourceDir: string
  docsTargetDir: string
}

const OPENCODE_GLOBAL_CONFIG = `${JSON.stringify(
  {
    $schema: 'https://opencode.ai/config.json',
    update: 'disable',
    snapshots: false,
    formatter: false,
  },
  null,
  2,
)}\n`

const PROJECT_CONFIG = `${JSON.stringify(
  {
    $schema: 'https://opencode.ai/config.json',
    default_agent: 'rer-assistant-es',
    update: 'disable',
    snapshots: false,
    formatter: false,
    plugins: ['./.opencode/plugins/rer-ui'],
    permissions: [
      { action: '*', resource: '*', effect: 'deny' },
      { action: 'read', resource: 'docs/ai/**', effect: 'allow' },
      { action: 'glob', resource: '*', effect: 'allow' },
      { action: 'grep', resource: '*', effect: 'allow' },
      { action: 'rer_ui_*', resource: '*', effect: 'allow' },
    ],
  },
  null,
  2,
)}\n`

const AGENT_FRONTMATTER = [
  '---',
  'description: Asistente RER-AI del editor RER-ENGINE (solo docs/ai)',
  'mode: primary',
  'steps: 10',
  '---',
  '',
].join('\n')

const AGENT_SYSTEM_ES = `${AGENT_FRONTMATTER}
Eres RER-AI, el asistente oficial del editor RER-ENGINE.

REGLAS:
- Responde SIEMPRE en español.
- Tu única fuente de conocimiento es la documentación en \`docs/ai/\`. Lee el archivo de guía correspondiente antes de responder y NUNCA inventes UI ni nombres de botones.
- Lista SIEMPRE los pasos completos, numerados (1. 2. 3.), sin truncar, usando las etiquetas exactas de la documentación.
- Para acompañar tu respuesta, usa las herramientas:
  - \`rer_ui_open_accordion\` para abrir un acordeón del panel izquierdo (clave corta: scenes, resources, entities, ui, herramientas, controles, mundo, camera).
  - \`rer_ui_highlight\` para resaltar un control (dom-id, p. ej. accordion-resources).
- No editas archivos ni ejecutas comandos; solo lees \`docs/ai/\` y llamas a esas dos herramientas.
`

const AGENT_SYSTEM_EN = `${AGENT_FRONTMATTER}
You are RER-AI, the official assistant of the RER-ENGINE editor.

RULES:
- Always reply in English.
- Your only source of knowledge is the documentation in \`docs/ai/\`. Read the matching guide file before answering and NEVER invent UI or button names.
- Always list the complete steps, numbered (1. 2. 3.), without truncation, using the exact labels from the documentation.
- To accompany your answer, use the tools:
  - \`rer_ui_open_accordion\` to open a left-sidebar accordion (short key: scenes, resources, entities, ui, herramientas, controles, mundo, camera).
  - \`rer_ui_highlight\` to highlight a control (dom-id, e.g. accordion-resources).
- You do not edit files or run commands; you only read \`docs/ai/\` and call those two tools.
`

const UI_ACTION_LOG_PLACEHOLDER = '__RER_UI_ACTION_LOG__'

const RER_UI_PLUGIN_TEMPLATE = `import { appendFile } from 'node:fs/promises'
import { Plugin } from '@opencode/plugin'

const LOG_FILE = '${UI_ACTION_LOG_PLACEHOLDER}'

async function record(entry: Record<string, unknown>): Promise<string> {
  await appendFile(LOG_FILE, JSON.stringify({ at: new Date().toISOString(), ...entry }) + '\\n', 'utf8')
  return 'OK'
}

export default Plugin.define({
  id: 'rer-ui',
  async setup(ctx) {
    await ctx.tool.transform((editor) => {
      editor.namespace({ name: 'rer_ui', description: 'Acciones de UI del editor RER-ENGINE' })
      editor.add({
        name: 'open_accordion',
        description: 'Abre un acordeón del panel izquierdo del editor RER-ENGINE',
        input: {
          type: 'object',
          properties: {
            accordionKey: {
              type: 'string',
              description: 'scenes | resources | entities | ui | herramientas | controles | mundo | camera',
            },
          },
          required: ['accordionKey'],
          additionalProperties: false,
        },
        options: { namespace: 'rer_ui', codemode: false },
        execute: async (input: { accordionKey: string }) => {
          return { content: await record({ tool: 'open_accordion', accordionKey: input.accordionKey }) }
        },
      })
      editor.add({
        name: 'highlight',
        description: 'Resalta un control de la UI del editor RER-ENGINE',
        input: {
          type: 'object',
          properties: {
            targetId: {
              type: 'string',
              description: 'dom-id del control, p. ej. accordion-resources',
            },
          },
          required: ['targetId'],
          additionalProperties: false,
        },
        options: { namespace: 'rer_ui', codemode: false },
        execute: async (input: { targetId: string }) => {
          return { content: await record({ tool: 'highlight', targetId: input.targetId }) }
        },
      })
    })
  },
})
`

/** Genera el plan determinista del sandbox (archivos + copia de docs/ai). */
export function buildWorkspacePlan(input: {
  workspaceDir: string
  docsSourceDir: string
  uiActionsLogPath: string
}): WorkspacePlan {
  const { workspaceDir, docsSourceDir } = input
  // El plugin usa la ruta dentro de un string JS: los backslashes de Windows
  // romperían escapes (\u, \r, \t...) y provocarían "Syntax Error" al transpilar.
  const uiActionsLogPath = input.uiActionsLogPath.replace(/\\/g, '/')
  const pluginSource = RER_UI_PLUGIN_TEMPLATE.replace(UI_ACTION_LOG_PLACEHOLDER, uiActionsLogPath)

  return {
    workspaceDir,
    docsSourceDir,
    docsTargetDir: path.join(workspaceDir, 'docs', 'ai'),
    files: [
      { relativePath: 'opencode.jsonc', content: PROJECT_CONFIG },
      { relativePath: '.opencode/agents/rer-assistant-es.md', content: AGENT_SYSTEM_ES },
      { relativePath: '.opencode/agents/rer-assistant-en.md', content: AGENT_SYSTEM_EN },
      { relativePath: '.opencode/plugins/rer-ui/index.ts', content: pluginSource },
    ],
  }
}

function copyDirRecursively(source: string, target: string): void {
  fs.mkdirSync(target, { recursive: true })
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    const from = path.join(source, entry.name)
    const to = path.join(target, entry.name)
    if (entry.isDirectory()) {
      copyDirRecursively(from, to)
    } else if (entry.isFile()) {
      fs.copyFileSync(from, to)
    }
  }
}

/** Escribe el plan sobre disco (idempotente: sobrescribe siempre). */
export function applyWorkspacePlan(plan: WorkspacePlan, uiActionsLogPath: string): void {
  fs.mkdirSync(plan.workspaceDir, { recursive: true })
  for (const file of plan.files) {
    const target = path.join(plan.workspaceDir, file.relativePath)
    fs.mkdirSync(path.dirname(target), { recursive: true })
    fs.writeFileSync(target, file.content, 'utf8')
  }
  fs.rmSync(plan.docsTargetDir, { recursive: true, force: true })
  copyDirRecursively(plan.docsSourceDir, plan.docsTargetDir)
  // Elimina el log anterior para que cada instalación empiece limpia.
  fs.rmSync(uiActionsLogPath, { force: true })
}

/** Config global del perfil XDG aislado (sin auto-update). */
export function applyOpenCodeGlobalConfig(globalConfigPath: string): void {
  fs.mkdirSync(path.dirname(globalConfigPath), { recursive: true })
  fs.writeFileSync(globalConfigPath, OPENCODE_GLOBAL_CONFIG, 'utf8')
}

/** Stamp para saber si el sandbox corresponde a la versión instalada y a los docs actuales. */
export function computeDocsSourceStamp(docsSourceDir: string): string {
  const hash = crypto.createHash('sha256')
  const walk = (dir: string, prefix: string): void => {
    let entries: fs.Dirent[]
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))
    } catch {
      return
    }
    for (const entry of entries) {
      const full = path.join(dir, entry.name)
      const rel = prefix ? `${prefix}/${entry.name}` : entry.name
      if (entry.isDirectory()) {
        walk(full, rel)
      } else if (entry.isFile()) {
        const stat = fs.statSync(full)
        hash.update(`${rel}:${stat.size}:${stat.mtimeMs}\n`)
      }
    }
  }
  walk(docsSourceDir, '')
  return hash.digest('hex')
}

export function workspaceStampMatches(
  workspaceDir: string,
  pluginVersion: string,
  docsSourceStamp?: string,
): boolean {
  const stampPath = path.join(workspaceDir, '.rer-stamp.json')
  try {
    const parsed = JSON.parse(fs.readFileSync(stampPath, 'utf8')) as {
      version?: string
      docsHash?: string
    }
    if (parsed.version !== pluginVersion) return false
    if (docsSourceStamp && parsed.docsHash !== docsSourceStamp) return false
    return true
  } catch {
    return false
  }
}

export function writeWorkspaceStamp(
  workspaceDir: string,
  pluginVersion: string,
  docsSourceStamp?: string,
): void {
  const stampPath = path.join(workspaceDir, '.rer-stamp.json')
  fs.writeFileSync(
    stampPath,
    `${JSON.stringify({ version: pluginVersion, docsHash: docsSourceStamp ?? null }, null, 2)}\n`,
    'utf8',
  )
}
