/**
 * Compila `rer-worldgen` y copia DLL + texturas al directorio de plugins
 * que usa el editor (`%APPDATA%\rer-engine\plugins\world-gen`).
 *
 * Lo invocan `yarn rust:build:editor` / `yarn rust:build:editor:release`
 * (predev y yarn start). Uso directo:
 *   node ./scripts/install-worldgen-dev.mjs [debug|release]
 *
 * Omite la copia con `RER_SKIP_WORLDGEN_INSTALL=1`.
 */
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const PLUGIN_ID = 'world-gen'
const DLL_NAME = 'rer_worldgen.dll'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const engineDir = path.join(root, 'src', 'main', 'Engine')
const manifest = path.join(engineDir, 'Cargo.toml')
const profile = (process.argv[2] ?? 'debug').trim()

if (profile !== 'debug' && profile !== 'release') {
  console.error(`[worldgen-dev] perfil desconocido: ${profile}. Usa: debug | release`)
  process.exit(1)
}

if (process.env.RER_SKIP_WORLDGEN_INSTALL === '1') {
  console.log('[worldgen-dev] omitido (RER_SKIP_WORLDGEN_INSTALL=1)')
  process.exit(0)
}

function fail(message) {
  console.error(`[worldgen-dev] ${message}`)
  process.exit(1)
}

function userDataDir() {
  const appdata = process.env.APPDATA
  if (!appdata) fail('APPDATA no está definido')
  for (const name of ['RER-ENGINE', 'rer-engine']) {
    const dir = path.join(appdata, name)
    if (fs.existsSync(dir)) return dir
  }
  return path.join(appdata, 'RER-ENGINE')
}

function copyReplace(src, dest) {
  try {
    fs.copyFileSync(src, dest)
    return
  } catch {
    const tmp = `${dest}.tmp`
    try {
      fs.copyFileSync(src, tmp)
      fs.rmSync(dest, { force: true })
      fs.renameSync(tmp, dest)
    } catch {
      try {
        fs.rmSync(tmp, { force: true })
      } catch {
        // ignore cleanup
      }
      fail(
        `no se pudo copiar ${dest}. Cierra el editor (yarn dev / yarn start) y vuelve a lanzarlo.`,
      )
    }
  }
}

function ensurePluginState(libraryPath) {
  const statePath = path.join(userDataDir(), 'plugins', 'state.json')
  let state = { installed: {}, enabled: [] }
  try {
    state = JSON.parse(fs.readFileSync(statePath, 'utf8'))
  } catch {
    // first local install
  }
  if (!state.installed || typeof state.installed !== 'object') state.installed = {}
  if (!Array.isArray(state.enabled)) state.enabled = []

  const previous = state.installed[PLUGIN_ID]
  state.installed[PLUGIN_ID] = {
    version: previous?.version ?? '1.0.0',
    libraryPath,
    installedAt: previous?.installedAt ?? new Date().toISOString(),
    source: 'local-dev',
  }
  if (!state.enabled.includes(PLUGIN_ID)) state.enabled.push(PLUGIN_ID)

  fs.mkdirSync(path.dirname(statePath), { recursive: true })
  fs.writeFileSync(statePath, `${JSON.stringify(state, null, 2)}\n`)
}

const cargoArgs = ['build', '--manifest-path', manifest, '-p', 'rer-worldgen']
if (profile === 'release') cargoArgs.splice(1, 0, '--release')

console.log(`\n==> cargo ${cargoArgs.join(' ')}`)
const built = spawnSync('cargo', cargoArgs, {
  cwd: root,
  stdio: 'inherit',
  env: process.env,
})
if (built.error) fail(`fallo al iniciar cargo: ${built.error.message}`)
if (built.status !== 0) fail(`cargo build rer-worldgen falló con exit ${built.status ?? 1}`)

const targetRoot = process.env.CARGO_TARGET_DIR
  ? path.resolve(process.env.CARGO_TARGET_DIR)
  : path.join(engineDir, 'target')
const dllSrc = path.join(targetRoot, profile, DLL_NAME)
if (!fs.existsSync(dllSrc)) fail(`no se encontró la DLL: ${dllSrc}`)

const destDir = path.join(userDataDir(), 'plugins', PLUGIN_ID)
const dllDest = path.join(destDir, DLL_NAME)
fs.mkdirSync(destDir, { recursive: true })
copyReplace(dllSrc, dllDest)

const terrainSrc = path.join(engineDir, 'worldgen', 'assets', 'terrain')
const terrainDest = path.join(destDir, 'terrain')
if (fs.existsSync(terrainSrc)) {
  fs.mkdirSync(terrainDest, { recursive: true })
  for (const name of fs.readdirSync(terrainSrc)) {
    if (!name.toLowerCase().endsWith('.jpg')) continue
    copyReplace(path.join(terrainSrc, name), path.join(terrainDest, name))
  }
}

ensurePluginState(dllDest)
console.log(`[worldgen-dev] ${profile} → ${dllDest}`)
