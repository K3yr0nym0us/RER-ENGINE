# Calidad Rust (engine)

Gate estricto para el workspace Cargo en `src/main/Engine/`.

Crates: `rer-engine-2d`, `rer-engine-3d`, `rer-engine-shared`, `rer-engine-ipc-common`.

## Comandos

| Comando | Qué hace |
|---------|----------|
| `yarn rust:fmt` | `cargo fmt --all -- --check` |
| `yarn rust:fmt:fix` | Aplica rustfmt |
| `yarn rust:check` | `cargo check --workspace --all-targets` |
| `yarn rust:clippy` | Clippy con `-D warnings` |
| `yarn rust:test` | `cargo test --workspace --all-features` |
| `yarn rust:audit` | `cargo audit` (requiere `cargo install cargo-audit --locked`) |
| `yarn quality:rust` | fmt + check + clippy + test + audit |
| `yarn quality:frontend` | ESLint + tsc + Vitest coverage |
| `yarn quality` | frontend + rust |

Config: `src/main/Engine/rustfmt.toml`, `src/main/Engine/clippy.toml`, lints en `Cargo.toml` del workspace.

## Clippy

Warnings = errores. No uses `#[allow(...)]` para ocultar problemas sin justificación técnica real.

## Cargo audit

Integrado en `yarn quality:rust` y en CI. Instala la herramienta una vez:

```bash
cargo install cargo-audit --locked
```

`cargo audit` reporta vulnerabilidades (falla) y avisos unmaintained/unsound (warning; no bloquean por defecto).

## Agentes de IA

Ver `.cursor/rules/rust-quality.mdc`.
