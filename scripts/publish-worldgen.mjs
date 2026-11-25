/**
 * Empaqueta el plugin world-gen sin publicarlo.
 *
 * Construye `rer-worldgen` en release y crea:
 * - Windows: rer-worldgen-<version>-win-x64.zip con rer_worldgen.dll.
 * - Linux:   rer-worldgen-<version>-linux-x64.zip con librer_worldgen.so.
 *
 * Ambos ZIP incluyen terrain/*.jpg. El SHA-256 se calcula del ZIP final y se
 * imprime para configurarlo manualmente en el catálogo del editor.
 *
 * Uso:
 *   node ./scripts/publish-worldgen.mjs [version] (por defecto: 1.0.0)
 */
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'
import { deflateRawSync } from 'node:zlib'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const engineDir = path.join(root, 'src', 'main', 'Engine')
const manifest = path.join(engineDir, 'Cargo.toml')
const version = (process.argv[2] ?? '1.0.0').trim()

if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  fail(`versión no válida: ${version}. Usa un formato como 1.0.0`)
}
if (process.platform !== 'win32' && process.platform !== 'linux') {
  fail(`plataforma no soportada: ${process.platform}`)
}

const platform = process.platform === 'win32' ? 'win' : 'linux'
const libraryName =
  process.platform === 'win32' ? 'rer_worldgen.dll' : 'librer_worldgen.so'
const outDir = path.join(engineDir, 'dist', 'worldgen')
const zipName = `rer-worldgen-${version}-${platform}-x64.zip`
const zipPath = path.join(outDir, zipName)

// ZIP usa CRC-32 (no el CRC-32C de algunas APIs modernas).
const CRC32_TABLE = Uint32Array.from({ length: 256 }, (_, index) => {
  let value = index
  for (let bit = 0; bit < 8; bit += 1) {
    value = (value & 1) !== 0 ? 0xedb88320 ^ (value >>> 1) : value >>> 1
  }
  return value >>> 0
})

console.log('\n==> Build release del crate worldgen')
const cargoArgs = [
  'build',
  '--manifest-path',
  manifest,
  '-p',
  'rer-worldgen',
  '--release',
]
console.log(`==> cargo ${cargoArgs.join(' ')}`)
const built = spawnSync('cargo', cargoArgs, {
  cwd: root,
  stdio: 'inherit',
  env: process.env,
})
if (built.error) fail(`fallo al iniciar cargo: ${built.error.message}`)
if (built.status !== 0) {
  fail(`cargo build rer-worldgen falló con exit ${built.status ?? 1}`)
}

const targetRoot = process.env.CARGO_TARGET_DIR
  ? path.resolve(root, process.env.CARGO_TARGET_DIR)
  : path.join(engineDir, 'target')
const libraryPath = path.join(targetRoot, 'release', libraryName)
if (!fs.existsSync(libraryPath)) {
  fail(`no se encontró la biblioteca nativa: ${libraryPath}`)
}

const terrainSrc = path.join(engineDir, 'worldgen', 'assets', 'terrain')
if (!fs.existsSync(terrainSrc)) {
  fail(`no se encontró el directorio de texturas: ${terrainSrc}`)
}
const terrainFiles = fs
  .readdirSync(terrainSrc, { withFileTypes: true })
  .filter(
    (entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.jpg'),
  )
  .map((entry) => entry.name)
  .sort()
if (terrainFiles.length === 0) {
  fail(`no se encontraron archivos terrain/*.jpg en ${terrainSrc}`)
}

const entries = [
  zipEntry(libraryName, libraryPath),
  ...terrainFiles.map((name) =>
    zipEntry(`terrain/${name}`, path.join(terrainSrc, name)),
  ),
]

try {
  writeZip(entries, zipPath)
} catch (error) {
  fail(`no se pudo crear ${zipPath}: ${error.message}`)
}

const sha256 = createHash('sha256').update(fs.readFileSync(zipPath)).digest('hex')
const sizeBytes = fs.statSync(zipPath).size

console.log('\nArtefacto listo:')
console.log(`  ZIP      : ${zipPath}`)
console.log(`  Tamaño   : ${sizeBytes} bytes`)
console.log(`  SHA-256  : ${sha256}`)
console.log('\nNo se publicó ningún release ni asset.')

function fail(message) {
  console.error(`[worldgen-publish] ${message}`)
  process.exit(1)
}

function zipEntry(name, sourcePath) {
  return {
    name,
    data: fs.readFileSync(sourcePath),
    modified: fs.statSync(sourcePath).mtime,
  }
}

function crc32(data) {
  let crc = 0xffffffff
  for (const byte of data) {
    crc = CRC32_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8)
  }
  return (crc ^ 0xffffffff) >>> 0
}

function dosDateTime(date) {
  const year = Math.max(1980, Math.min(2107, date.getFullYear()))
  const time =
    (date.getHours() << 11) |
    (date.getMinutes() << 5) |
    Math.floor(date.getSeconds() / 2)
  const day =
    ((year - 1980) << 9) | ((date.getMonth() + 1) << 5) | date.getDate()
  return { time, day }
}

function writeZip(entries, destination) {
  const localParts = []
  const centralParts = []
  let offset = 0

  for (const entry of entries) {
    const name = Buffer.from(entry.name, 'utf8')
    const deflated = deflateRawSync(entry.data, { level: 9 })
    const store = deflated.length >= entry.data.length
    const payload = store ? entry.data : deflated
    const method = store ? 0 : 8
    const checksum = crc32(entry.data)
    const { time, day } = dosDateTime(entry.modified)
    const flags = 0x0800 // Nombres de archivo UTF-8.

    if (
      entry.data.length > 0xffffffff ||
      payload.length > 0xffffffff ||
      offset > 0xffffffff
    ) {
      throw new Error('el artefacto supera el límite ZIP de 4 GiB')
    }

    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(20, 4)
    local.writeUInt16LE(flags, 6)
    local.writeUInt16LE(method, 8)
    local.writeUInt16LE(time, 10)
    local.writeUInt16LE(day, 12)
    local.writeUInt32LE(checksum, 14)
    local.writeUInt32LE(payload.length, 18)
    local.writeUInt32LE(entry.data.length, 22)
    local.writeUInt16LE(name.length, 26)
    local.writeUInt16LE(0, 28)
    localParts.push(local, name, payload)

    const central = Buffer.alloc(46)
    central.writeUInt32LE(0x02014b50, 0)
    central.writeUInt16LE(0x0314, 4) // UNIX, ZIP 2.0.
    central.writeUInt16LE(20, 6)
    central.writeUInt16LE(flags, 8)
    central.writeUInt16LE(method, 10)
    central.writeUInt16LE(time, 12)
    central.writeUInt16LE(day, 14)
    central.writeUInt32LE(checksum, 16)
    central.writeUInt32LE(payload.length, 20)
    central.writeUInt32LE(entry.data.length, 24)
    central.writeUInt16LE(name.length, 28)
    central.writeUInt16LE(0, 30)
    central.writeUInt16LE(0, 32)
    central.writeUInt16LE(0, 34)
    central.writeUInt16LE(0, 36)
    central.writeUInt32LE(0o100644 * 0x10000, 38)
    central.writeUInt32LE(offset, 42)
    centralParts.push(central, name)

    offset += local.length + name.length + payload.length
  }

  const centralDirectory = Buffer.concat(centralParts)
  if (centralDirectory.length > 0xffffffff || offset > 0xffffffff) {
    throw new Error('el directorio central supera el límite ZIP de 4 GiB')
  }
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(0, 4)
  end.writeUInt16LE(0, 6)
  end.writeUInt16LE(entries.length, 8)
  end.writeUInt16LE(entries.length, 10)
  end.writeUInt32LE(centralDirectory.length, 12)
  end.writeUInt32LE(offset, 16)
  end.writeUInt16LE(0, 20)

  fs.mkdirSync(path.dirname(destination), { recursive: true })
  const temporary = `${destination}.tmp`
  try {
    fs.writeFileSync(
      temporary,
      Buffer.concat([...localParts, centralDirectory, end]),
    )
    fs.rmSync(destination, { force: true })
    fs.renameSync(temporary, destination)
  } finally {
    fs.rmSync(temporary, { force: true })
  }
}
