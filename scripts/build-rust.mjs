/**
 * Build de los binarios Rust del editor con conjuntos de features aislados.
 *
 * La app Electron lanza SOLO `rer_engine_editor` (ver `startEditorFrame` en
 * src/main/index.ts). Los binarios standalone `rer_engine_2d` / `rer_engine_3d`
 * se usan para depuración y para el instalador, no para `yarn dev` / `yarn start`.
 *
 * `rer-engine-core` NO se compila aquí: es dependencia de `rer-engine` y cargo
 * lo construye ya con el conjunto de features correcto en cada invocación.
 * Compilarlo aparte (sin `vulkan`) era un build redundante que además se
 * recompilaba de nuevo al cambiar de features.
 *
 * Uso:
 *   node ./scripts/build-rust.mjs <debug|release> <editor|all>
 */
import { spawnSync } from 'node:child_process'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const manifest = path.join(root, 'src', 'main', 'Engine', 'Cargo.toml')

const profile = (process.argv[2] ?? 'debug').trim()
const scope = (process.argv[3] ?? 'editor').trim()

if (profile !== 'debug' && profile !== 'release') {
  console.error(`[build-rust] perfil desconocido: ${profile}. Usa: debug | release`)
  process.exit(1)
}
if (scope !== 'editor' && scope !== 'all') {
  console.error(`[build-rust] alcance desconocido: ${scope}. Usa: editor | all`)
  process.exit(1)
}

// Cada binario necesita su propio feature set: cargo unifica features entre
// `-p` de una misma invocación, así que hay que compilarlos por separado.
const targets =
  scope === 'editor'
    ? [['rer_engine_editor', 'editor']]
    : [
        ['rer_engine_2d', 'engine2d'],
        ['rer_engine_3d', 'engine3d'],
        ['rer_engine_editor', 'editor'],
      ]

for (const [binary, feature] of targets) {
  const args = [
    'build',
    '--manifest-path',
    manifest,
    '-p',
    'rer-engine',
    '--bin',
    binary,
    '--no-default-features',
    '--features',
    feature,
  ]
  if (profile === 'release') args.splice(1, 0, '--release')

  console.log(`\n==> cargo ${args.join(' ')}`)
  const result = spawnSync('cargo', args, {
    cwd: root,
    stdio: 'inherit',
    env: process.env,
  })
  if (result.error) {
    console.error(`[build-rust] fallo al iniciar cargo: ${result.error.message}`)
    process.exit(1)
  }
  if (result.status !== 0) {
    console.error(`[build-rust] ${binary} falló con exit ${result.status ?? 1}`)
    process.exit(result.status ?? 1)
  }
}

console.log(`\n[build-rust] ${scope} (${profile}) OK`)
