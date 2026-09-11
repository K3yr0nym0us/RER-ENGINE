# Calidad frontend (Electron / React / TypeScript)

Gate de calidad **estricto** para el editor Electron. El motor Rust queda fuera de este análisis.

## Comandos locales

| Comando | Qué hace |
|---------|----------|
| `yarn lint` | ESLint (cero warnings; con caché) |
| `yarn lint:fix` | Autofix ESLint seguro |
| `yarn typecheck` | `tsc -b` (strict + unused) |
| `yarn test` | Vitest (una pasada) |
| `yarn test:watch` | Vitest en watch |
| `yarn test:coverage` | Vitest + cobertura (`coverage/`: text, LCOV, HTML) |
| `yarn quality` | lint + typecheck + test:coverage (+ Rust vía `quality:rust`) |

Si `yarn quality` falla, el cambio **no** está listo.

## Vitest

Config: `vitest.config.ts`. Setup compartido: `src/test/setup.ts` (limpia mocks tras cada test).

| Ámbito | Entorno | Convención de archivos |
|--------|---------|------------------------|
| main / preload / shared-types / helpers | `node` | `*.test.ts` / `*.spec.ts` junto al módulo |
| React (renderer) | `jsdom` | `*.test.tsx` / `*.spec.tsx` bajo `src/renderer/` |

Aliases iguales que `electron.vite.config.ts` (`@renderer`, `@shared-types`, `@hooks`, …).

**Fuera de alcance:** crates Rust bajo `src/main/Engine/engine_*` y `target/` (calidad aparte con Cargo).

Ejemplos existentes: `src/shared-types/engineCommands.test.ts`, `src/main/plugins/local-ai-assistant/msvcRedistributable.test.ts`.

## ESLint

Configuración: `eslint.config.mjs` (flat config, caché `.eslintcache`).

Incluye:

- `@eslint/js` recommended
- `typescript-eslint` recommended
- Reglas type-aware acotadas: `no-floating-promises`, `no-misused-promises`, `await-thenable` (rápidas vs preset typeChecked completo)
- `eslint-plugin-react` + hooks clásicos de React 18 (`rules-of-hooks`, `exhaustive-deps`)
- `eslint-plugin-sonarjs` recommended (bugs / smells en CI/local sin token; ruido legacy desactivado)

Reglas clave: `no-explicit-any`, `ban-ts-comment`, `no-non-null-assertion`, type imports, `max-depth`, `eval` / patrones peligrosos.

## TypeScript

- `tsconfig.web.json` — renderer
- `tsconfig.node.json` — main / preload / shared-types / configs

Activo: `strict`, `noImplicitAny`, `strictNullChecks`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`, `forceConsistentCasingInFileNames`.

## SonarLint (IDE, sin token)

Usamos **SonarLint** en el editor (Cursor / VS Code), **no** SonarCloud ni scanners que pidan `SONAR_TOKEN`.

- Extensión: `SonarSource.sonarlint-vscode` (instalada en el IDE)
- En CLI/CI las reglas SonarJS van por `eslint-plugin-sonarjs` dentro de `yarn lint`

No hace falta Connected Mode, organización ni secretos de SonarCloud. No versionamos `.vscode/` para esto.

## CI

`.github/workflows/quality.yml` (`windows-latest`):

- Frontend: `yarn lint`, `yarn typecheck`, `yarn test:coverage`
- Rust: fmt, check, clippy (`-D warnings`), test, audit

## Qué se analiza / qué no

**Sí:** `src/main` (TS Electron, excl. crates Rust), `src/preload`, `src/renderer`, `src/shared-types`.

**No:** `src/main/Engine/engine_*`, `target/`, `Models/`, `assets/` del motor.

## Agentes de IA

Ver `.cursor/rules/frontend-quality.mdc`: no dar por terminada una tarea frontend sin `yarn quality` en verde; no debilitar reglas ni ocultar errores con `any` / `@ts-ignore` / `eslint-disable`.
