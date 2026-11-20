/**
 * Mantiene `target/` acotado SIN limpiarlo en cada arranque.
 *
 * Antes `yarn dev` / `yarn start` ejecutaban `cargo clean` siempre, lo que
 * obligaba a recompilar el motor entero (~69 s en debug) en cada prueba. Este
 * guardia no hace nada mientras el directorio esté por debajo del umbral
 * (~0,1 s de medición) y solo interviene cuando crece demasiado:
 *
 *   1. Poda cachés puras: `doc/` y los `incremental/` de debug y release.
 *      No fuerzan recompilar dependencias: los `.rlib` de `deps/` se conservan.
 *   2. Si aún se supera el umbral, `cargo clean` completo (un único rebuild
 *      lento, en vez de uno en cada arranque).
 *
 * Uso:  node ./scripts/prune-rust-target.mjs <debug|release>
 * Env:  RER_TARGET_MAX_GB      tope total de target/          (default 18)
 *       RER_TARGET_CACHE_MAX_GB  tope por caché incremental   (default 2)
 */
import { spawnSync } from 'node:child_process'
import { readdirSync, rmSync, statSync } from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const manifest = path.join(root, 'src', 'main', 'Engine', 'Cargo.toml')
const target = path.join(root, 'src', 'main', 'Engine', 'target')

const gb = (name, fallback) => {
  const value = Number(process.env[name] ?? fallback)
  return Number.isFinite(value) && value > 0 ? value : fallback
}
const maxBytes = gb('RER_TARGET_MAX_GB', 18) * 1_000_000_000
const cacheMaxBytes = gb('RER_TARGET_CACHE_MAX_GB', 2) * 1_000_000_000
const profile = (process.argv[2] ?? 'debug').trim()

function dirBytes(dir) {
  let total = 0
  let entries
  try {
    entries = readdirSync(dir, { withFileTypes: true })
  } catch {
    return 0
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name)
    try {
      if (entry.isDirectory()) total += dirBytes(full)
      else if (entry.isFile()) total += statSync(full).size
    } catch {
      // Archivo desaparecido o sin permisos durante la medición: se ignora.
    }
  }
  return total
}

function remove(dir) {
  try {
    rmSync(dir, { recursive: true, force: true })
    return true
  } catch {
    return false
  }
}

const fmt = (bytes) => `${(bytes / 1_000_000_000).toFixed(2)} GB`
let size = dirBytes(target)

if (size <= maxBytes) {
  // Modo normal: no se toca nada. Solo se poda una caché incremental
  // desproporcionada, que es prescindible por definición.
  for (const name of ['debug', 'release']) {
    const cache = path.join(target, name, 'incremental')
    const cacheSize = dirBytes(cache)
    if (cacheSize > cacheMaxBytes && remove(cache)) {
      console.log(`[prune-target] caché incremental de ${name} podada (${fmt(cacheSize)})`)
    }
  }
  console.log(`[prune-target] target ${fmt(size)} / ${fmt(maxBytes)} → caché conservada`)
  process.exit(0)
}

console.log(
  `[prune-target] target ${fmt(size)} supera el tope de ${fmt(maxBytes)}: podando cachés...`,
)
for (const rel of ['doc', 'debug/incremental', 'release/incremental']) {
  const dir = path.join(target, ...rel.split('/'))
  const bytes = dirBytes(dir)
  if (bytes > 0 && remove(dir)) console.log(`[prune-target] - ${rel} (${fmt(bytes)})`)
}

size = dirBytes(target)
if (size <= maxBytes) {
  console.log(`[prune-target] target reducido a ${fmt(size)}; rebuild incremental disponible`)
  process.exit(0)
}

console.log(`[prune-target] aún ${fmt(size)}: cargo clean completo (perfil ${profile})`)
const result = spawnSync(
  'cargo',
  ['clean', '--manifest-path', manifest, '--profile', profile === 'release' ? 'release' : 'dev'],
  { cwd: root, stdio: 'inherit' },
)
if (result.error || result.status !== 0) {
  console.error('[prune-target] cargo clean falló')
  process.exit(result?.status ?? 1)
}
console.log(`[prune-target] target ${fmt(dirBytes(target))}; se reconstruirá ${profile}`)
