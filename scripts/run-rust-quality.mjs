/**
 * Cross-platform Rust quality runner for the engine Cargo workspace.
 * Exit ≠ 0 on any failure. Requires cargo, rustfmt, clippy; cargo-audit for the audit step.
 */
import { spawnSync } from 'node:child_process'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const manifest = path.join(root, 'src', 'main', 'Engine', 'Cargo.toml')
const lockfile = path.join(root, 'src', 'main', 'Engine', 'Cargo.lock')

function run(label, command, args) {
  console.log(`\n==> ${label}`)
  console.log(`$ ${command} ${args.join(' ')}`)
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: 'inherit',
    shell: true,
    env: process.env,
  })
  if (result.error) {
    console.error(`[rust-quality] failed to start ${command}:`, result.error.message)
    process.exit(1)
  }
  if (result.status !== 0) {
    console.error(`[rust-quality] ${label} failed with exit ${result.status ?? 1}`)
    process.exit(result.status ?? 1)
  }
}

const steps = process.argv.slice(2)
const all = steps.length === 0

if (all || steps.includes('fmt')) {
  run('cargo fmt --check', 'cargo', [
    'fmt',
    '--manifest-path',
    manifest,
    '--all',
    '--',
    '--check',
  ])
}

if (all || steps.includes('check')) {
  run('cargo check', 'cargo', [
    'check',
    '--manifest-path',
    manifest,
    '--workspace',
    '--all-targets',
  ])
}

if (all || steps.includes('clippy')) {
  run('cargo clippy', 'cargo', [
    'clippy',
    '--manifest-path',
    manifest,
    '--workspace',
    '--all-targets',
    '--all-features',
    '--',
    '-D',
    'warnings',
  ])
}

if (all || steps.includes('test')) {
  run('cargo test', 'cargo', [
    'test',
    '--manifest-path',
    manifest,
    '--workspace',
    '--all-features',
  ])
}

if (all || steps.includes('audit')) {
  const auditCheck = spawnSync('cargo', ['audit', '--version'], {
    cwd: root,
    stdio: 'pipe',
    shell: true,
  })
  if (auditCheck.status !== 0) {
    console.error(
      '[rust-quality] cargo-audit is not installed. Install with: cargo install cargo-audit --locked',
    )
    process.exit(1)
  }
  run('cargo audit', 'cargo', ['audit', '--file', lockfile])
}

console.log('\n[rust-quality] OK')
