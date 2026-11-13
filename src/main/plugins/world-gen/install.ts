//! Instalación del plugin `world-gen` (DLL nativa de generación de mundos).
//!
//! Flujo: descarga del ZIP desde GitHub Releases (`downloadFile`), verificación
//! opcional de SHA-256, extracción de `rer_worldgen.dll` a
//! `userData/plugins/world-gen/` y escritura del record en `plugins/state.json`.
//! La lógica de pago se engancha aquí: validar la licencia **antes** de descargar.

import AdmZip from 'adm-zip'
import crypto from 'crypto'
import fs from 'fs'
import path from 'path'

import type { PluginId, PluginInstallResult, PluginArtifactManifest } from '../../../shared-types/plugins'
import { cancelActiveDownload, DownloadCancelledError, downloadFile } from '../fileDownload'
import { getPluginCatalogEntry } from '../pluginCatalog'
import { broadcastPluginsProgress, resetProgressBroadcastThrottle } from '../pluginProgressBroadcast'
import { getPluginDir, readPluginsState, writePluginsState } from '../pluginState'

const WORLDGEN_PLUGIN_ID: PluginId = 'world-gen'

let installInProgress = false

export function isWorldGenInstallInProgress(): boolean {
  return installInProgress
}

export function cancelWorldGenInstall(): void {
  cancelActiveDownload()
}

export async function installWorldGenPlugin(_pluginId: PluginId): Promise<PluginInstallResult> {
  const entry = getPluginCatalogEntry(WORLDGEN_PLUGIN_ID)
  if (!entry?.artifact) {
    return { ok: false, error: 'Catalog entry world-gen is missing its artifact manifest' }
  }
  if (installInProgress) {
    return { ok: false, error: 'Install already in progress' }
  }

  installInProgress = true
  resetProgressBroadcastThrottle()
  try {
    // La URL debe ser la del ASSET (`/releases/download/...`), no la página del
    // release (`/releases/tag/...`): la página es HTML y la descarga "funciona"
    // pero con unos KB de HTML que fallarían la verificación de tamaño/SHA.
    if (!/\/releases\/download\//.test(entry.artifact.downloadUrl)) {
      throw new Error(
        `artifact.downloadUrl must point to the release asset (/releases/download/...), got: ${entry.artifact.downloadUrl}`,
      )
    }
    const dir = getPluginDir(WORLDGEN_PLUGIN_ID)
    fs.mkdirSync(dir, { recursive: true })
    const zipPath = path.join(dir, `${entry.id}-${entry.version}.zip`)
    const total = entry.artifact.sizeBytes

    // Gancho de pago: aquí irá la validación de licencia antes de descargar.
    await downloadFile(entry.artifact.downloadUrl, zipPath, (progress) => {
      const overall = total > 0 ? Math.min(99, Math.round((progress.bytesReceived / total) * 100)) : 0
      broadcastPluginsProgress({
        pluginId: WORLDGEN_PLUGIN_ID,
        phase: 'artifact',
        step: 1,
        stepsTotal: 2,
        percent: progress.percent,
        overallPercent: overall,
        bytesReceived: progress.bytesReceived,
        bytesTotal: total,
        bytesOverallReceived: progress.bytesReceived,
        bytesOverallTotal: total,
      })
    }, Math.max(1, Math.round(total * 0.9)))

    // Verificación de integridad (opcional: solo si el catálogo declara SHA-256).
    const actualSha256 = sha256File(zipPath)
    if (entry.artifact.sha256 && actualSha256 !== entry.artifact.sha256.toLowerCase()) {
      throw new Error(`SHA-256 mismatch: expected ${entry.artifact.sha256}, got ${actualSha256}`)
    }

    // Fase de extracción.
    broadcastPluginsProgress({
      pluginId: WORLDGEN_PLUGIN_ID,
      phase: 'artifact',
      step: 2,
      stepsTotal: 2,
      percent: 100,
      overallPercent: 100,
      bytesReceived: total,
      bytesTotal: total,
      bytesOverallReceived: total,
      bytesOverallTotal: total,
    })
    const libraryPath = extractArtifactZip(zipPath, dir, entry.artifact)
    fs.rmSync(zipPath, { force: true })

    // Record de instalación (reutiliza el mismo state.json del sistema de plugins).
    const state = readPluginsState()
    state.installed[WORLDGEN_PLUGIN_ID] = {
      version: entry.version,
      libraryPath,
      installedAt: new Date().toISOString(),
    }
    writePluginsState(state)
    return { ok: true }
  } catch (err) {
    if (err instanceof DownloadCancelledError) {
      return { ok: false, cancelled: true }
    }
    const message = err instanceof Error ? err.message : String(err)
    return { ok: false, error: message }
  } finally {
    installInProgress = false
  }
}

export async function uninstallWorldGenPlugin(): Promise<PluginInstallResult> {
  try {
    const dir = getPluginDir(WORLDGEN_PLUGIN_ID)
    fs.rmSync(dir, { recursive: true, force: true })
    const state = readPluginsState()
    delete state.installed[WORLDGEN_PLUGIN_ID]
    state.enabled = state.enabled.filter((id) => id !== WORLDGEN_PLUGIN_ID)
    writePluginsState(state)
    return { ok: true }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    return { ok: false, error: message }
  }
}

/** Ruta del binario instalado, o null si no está.
 *
 * Se exporta para que el editor pueda construir el comando
 * `world_gen_set_library_path` al habilitar el plugin.
 */
export function getWorldGenLibraryPath(): string | null {
  const state = readPluginsState()
  const record = state.installed[WORLDGEN_PLUGIN_ID]
  if (!record?.libraryPath || !fs.existsSync(record.libraryPath)) return null
  return record.libraryPath
}

function extractArtifactZip(zipPath: string, destDir: string, artifact: PluginArtifactManifest): string {
  const zip = new AdmZip(zipPath)
  const inner = zip.getEntry(artifact.archiveInnerPath)
  if (!inner) {
    throw new Error(`${artifact.archiveInnerPath} was not found inside the artifact archive`)
  }
  const dest = path.join(destDir, artifact.executableName)
  const data = inner.getData()
  fs.writeFileSync(dest, data)
  return dest
}

function sha256File(filePath: string): string {
  const hash = crypto.createHash('sha256')
  hash.update(fs.readFileSync(filePath))
  return hash.digest('hex')
}
