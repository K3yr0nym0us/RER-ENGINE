/**
 * Feature-aware test suites for the Rust engine workspace.
 *
 * The engine is compiled with mutually restrictive feature sets in production:
 * `rer_engine_3d` (engine3d), `rer_engine_2d` (engine2d) and `rer_engine_editor`
 * (editor, which pulls BOTH engines). Running only `--all-features` hides
 * `#[cfg(feature = "editor")]` mistakes, so each suite builds and tests the
 * feature set that the shipped binaries actually use.
 *
 * Usage: node ./scripts/run-rust-suite.mjs <engine3d|engine2d|worldgen|all>
 */
import { spawnSync } from 'node:child_process'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const manifest = path.join(root, 'src', 'main', 'Engine', 'Cargo.toml')

const cargo = ([subcommand, ...rest]) => ({
  cmd: 'cargo',
  args: [subcommand, '--manifest-path', manifest, ...rest],
})

const steps = {
  fmt: { label: 'rustfmt --check', ...cargo(['fmt', '--all', '--', '--check']) },

  clippy3d: {
    label: 'clippy engine3d',
    ...cargo([
      'clippy',
      '-p',
      'rer-engine',
      '--no-default-features',
      '--features',
      'engine3d',
      '--all-targets',
      '--',
      '-D',
      'warnings',
    ]),
  },
  test3d: {
    label: 'test engine3d',
    ...cargo([
      'test',
      '-p',
      'rer-engine',
      '--no-default-features',
      '--features',
      'engine3d',
    ]),
  },
  build3d: {
    label: 'build rer_engine_3d',
    ...cargo([
      'build',
      '-p',
      'rer-engine',
      '--bin',
      'rer_engine_3d',
      '--no-default-features',
      '--features',
      'engine3d',
    ]),
  },

  clippy2d: {
    label: 'clippy engine2d',
    ...cargo([
      'clippy',
      '-p',
      'rer-engine',
      '--no-default-features',
      '--features',
      'engine2d',
      '--all-targets',
      '--',
      '-D',
      'warnings',
    ]),
  },
  test2d: {
    label: 'test engine2d',
    ...cargo([
      'test',
      '-p',
      'rer-engine',
      '--no-default-features',
      '--features',
      'engine2d',
    ]),
  },
  build2d: {
    label: 'build rer_engine_2d',
    ...cargo([
      'build',
      '-p',
      'rer-engine',
      '--bin',
      'rer_engine_2d',
      '--no-default-features',
      '--features',
      'engine2d',
    ]),
  },

  clippyEditor: {
    label: 'clippy editor (incluye ambos motores)',
    ...cargo([
      'clippy',
      '-p',
      'rer-engine',
      '--no-default-features',
      '--features',
      'editor',
      '--all-targets',
      '--',
      '-D',
      'warnings',
    ]),
  },
  testEditor: {
    label: 'test editor + ambos motores',
    ...cargo([
      'test',
      '-p',
      'rer-engine',
      '--no-default-features',
      '--features',
      'editor',
    ]),
  },
  buildEditor: {
    label: 'build rer_engine_editor',
    ...cargo([
      'build',
      '-p',
      'rer-engine',
      '--bin',
      'rer_engine_editor',
      '--no-default-features',
      '--features',
      'editor',
    ]),
  },

  clippyWorldgen: {
    label: 'clippy rer-worldgen',
    ...cargo(['clippy', '-p', 'rer-worldgen', '--all-targets', '--', '-D', 'warnings']),
  },
  testWorldgen: { label: 'test rer-worldgen', ...cargo(['test', '-p', 'rer-worldgen']) },
  buildWorldgen: { label: 'build rer-worldgen', ...cargo(['build', '-p', 'rer-worldgen']) },
}

const suites = {
  engine3d: [
    'fmt',
    'clippy3d',
    'test3d',
    'build3d',
    // El binario del editor hospeda el motor 3D in-process.
    'clippyEditor',
    'testEditor',
    'buildEditor',
  ],
  engine2d: [
    'fmt',
    'clippy2d',
    'test2d',
    'build2d',
    // El binario del editor hospeda el motor 2D in-process.
    'clippyEditor',
    'testEditor',
    'buildEditor',
  ],
  worldgen: ['fmt', 'clippyWorldgen', 'testWorldgen', 'buildWorldgen'],
}

suites.all = [
  'fmt',
  'clippy3d',
  'test3d',
  'build3d',
  'clippy2d',
  'test2d',
  'build2d',
  'clippyEditor',
  'testEditor',
  'buildEditor',
  'clippyWorldgen',
  'testWorldgen',
  'buildWorldgen',
]

function run(step) {
  console.log(`\n==> ${step.label}`)
  console.log(`$ ${step.cmd} ${step.args.join(' ')}`)
  const result = spawnSync(step.cmd, step.args, {
    cwd: root,
    stdio: 'inherit',
    env: process.env,
  })
  if (result.error) {
    console.error(`[rust-suite] failed to start ${step.cmd}:`, result.error.message)
    process.exit(1)
  }
  if (result.status !== 0) {
    console.error(`[rust-suite] ${step.label} failed with exit ${result.status ?? 1}`)
    process.exit(result.status ?? 1)
  }
}

const name = (process.argv[2] ?? '').trim()
const selected = suites[name]
if (!selected) {
  console.error(
    `[rust-suite] suite desconocida: ${name || '(vacío)'}. Usa: ${Object.keys(suites).join(' | ')}`,
  )
  process.exit(1)
}

for (const id of selected) {
  run(steps[id])
}

console.log(`\n[rust-suite] ${name} OK`)
