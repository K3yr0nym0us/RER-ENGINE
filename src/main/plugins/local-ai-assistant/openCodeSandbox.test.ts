import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import {
  buildWorkspacePlan,
  computeDocsSourceStamp,
  workspaceStampMatches,
  writeWorkspaceStamp,
} from './openCodeSandbox'

describe('workspace stamp', () => {
  const tmpRoots: string[] = []

  afterEach(() => {
    for (const root of tmpRoots.splice(0)) {
      fs.rmSync(root, { recursive: true, force: true })
    }
  })

  it('computeDocsSourceStamp es estable y cambia cuando los docs cambian', () => {
    const docs = fs.mkdtempSync(path.join(os.tmpdir(), 'rer-docs-'))
    tmpRoots.push(docs)
    fs.writeFileSync(path.join(docs, 'guide.txt'), 'v1', 'utf8')

    const stamp1 = computeDocsSourceStamp(docs)
    const stamp2 = computeDocsSourceStamp(docs)
    expect(stamp1).toBe(stamp2)

    fs.writeFileSync(path.join(docs, 'guide.txt'), 'v2 con más contenido', 'utf8')
    expect(computeDocsSourceStamp(docs)).not.toBe(stamp1)
  })

  it('workspaceStampMatches exige versión y hash de docs coincidentes', () => {
    const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'rer-ws-'))
    tmpRoots.push(ws)

    writeWorkspaceStamp(ws, '2.0.0', 'abc123')
    expect(workspaceStampMatches(ws, '2.0.0', 'abc123')).toBe(true)
    expect(workspaceStampMatches(ws, '2.0.0', 'otro')).toBe(false)
    expect(workspaceStampMatches(ws, '2.0.1', 'abc123')).toBe(false)
    expect(workspaceStampMatches(ws, '2.0.0')).toBe(true)
    expect(workspaceStampMatches('C:/fake/workspace', '2.0.0')).toBe(false)
  })
})


describe('buildWorkspacePlan', () => {
  const plan = buildWorkspacePlan({
    workspaceDir: 'C:/fake/workspace',
    docsSourceDir: 'C:/fake/docs/ai',
    uiActionsLogPath: 'C:/fake/workspace/ui-actions.log',
  })

  it('declara el proyecto, los agentes y el plugin rer-ui', () => {
    const paths = plan.files.map((file) => file.relativePath)
    expect(paths).toContain('opencode.jsonc')
    expect(paths).toContain('.opencode/agents/rer-assistant-es.md')
    expect(paths).toContain('.opencode/agents/rer-assistant-en.md')
    expect(paths).toContain('.opencode/plugins/rer-ui/index.ts')
  })

  it('deniega todo por defecto y solo permite leer docs/ai y las tools rer_ui', () => {
    const config = plan.files.find((file) => file.relativePath === 'opencode.jsonc')
    expect(config).toBeDefined()
    const parsed = JSON.parse(config?.content ?? '{}') as {
      permissions: Array<{ action: string; resource: string; effect: string }>
    }
    const rules = parsed.permissions
    expect(rules[0]).toEqual({ action: '*', resource: '*', effect: 'deny' })
    expect(rules).toContainEqual({ action: 'read', resource: 'docs/ai/**', effect: 'allow' })
    expect(rules).toContainEqual({ action: 'rer_ui_*', resource: '*', effect: 'allow' })
    const allows = rules.filter((rule) => rule.effect === 'allow')
    for (const rule of allows) {
      const allowed = ['read', 'glob', 'grep', 'rer_ui_*']
      expect(allowed).toContain(rule.action)
    }
  })

  it('inyecta la ruta del log de acciones en el plugin rer-ui', () => {
    const plugin = plan.files.find(
      (file) => file.relativePath === '.opencode/plugins/rer-ui/index.ts',
    )
    expect(plugin?.content).toContain('C:/fake/workspace/ui-actions.log')
    expect(plugin?.content).toContain("name: 'open_accordion'")
    expect(plugin?.content).toContain("name: 'highlight'")
    expect(plugin?.content).not.toContain('__RER_UI_ACTION_LOG__')
  })

  it('normaliza la ruta del log a forward slashes (los backslashes rompen escapes JS)', () => {
    const windowsPathPlan = buildWorkspacePlan({
      workspaceDir: 'C:\\fake\\workspace',
      docsSourceDir: 'C:\\fake\\docs\\ai',
      uiActionsLogPath: 'C:\\fake\\workspace\\ui-actions.log',
    })
    const plugin = windowsPathPlan.files.find(
      (file) => file.relativePath === '.opencode/plugins/rer-ui/index.ts',
    )
    expect(plugin?.content).toContain('C:/fake/workspace/ui-actions.log')
    expect(plugin?.content).not.toMatch(/\\fake\\workspace/)
  })

  it('copia docs/ai al sandbox', () => {
    expect(plan.docsTargetDir.replace(/\\/g, '/')).toBe('C:/fake/workspace/docs/ai')
    expect(plan.docsSourceDir.replace(/\\/g, '/')).toBe('C:/fake/docs/ai')
  })
})
