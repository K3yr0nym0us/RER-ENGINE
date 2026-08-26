# Calidad frontend (Electron / React / TypeScript)

Gate de calidad **estricto** para el editor Electron. El motor Rust queda fuera de este análisis.

## Comandos locales

| Comando | Qué hace |
|---------|----------|
| `yarn lint` | ESLint (cero warnings; con caché) |
| `yarn lint:fix` | Autofix ESLint seguro |
| `yarn typecheck` | `tsc -b` (strict + unused) |
| `yarn test` | Vitest |
| `yarn test:coverage` | Vitest + LCOV (`coverage/`) |
| `yarn quality` | lint + typecheck + test:coverage |
| `yarn sonar` | Scanner Sonar (requiere `SONAR_TOKEN`) |

Si `yarn quality` falla, el cambio **no** está listo.

## ESLint

Configuración: `eslint.config.mjs` (flat config, caché `.eslintcache`).

Incluye:

- `@eslint/js` recommended
- `typescript-eslint` recommended
- Reglas type-aware acotadas: `no-floating-promises`, `no-misused-promises`, `await-thenable` (rápidas vs preset typeChecked completo)
- `eslint-plugin-react` + hooks clásicos de React 18 (`rules-of-hooks`, `exhaustive-deps`)
- `eslint-plugin-sonarjs` recommended (bugs / smells; ruido legacy desactivado)

Reglas clave: `no-explicit-any`, `ban-ts-comment`, `no-non-null-assertion`, type imports, `max-depth`, `eval` / patrones peligrosos.

La **complejidad cognitiva / ciclomática** de handlers legacy se controla en SonarCloud sobre **código nuevo** (Quality Gate), no relajando el gate local.

## TypeScript

- `tsconfig.web.json` — renderer
- `tsconfig.node.json` — main / preload / shared-types / configs

Activo: `strict`, `noImplicitAny`, `strictNullChecks`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`, `forceConsistentCasingInFileNames`.

## SonarCloud / Quality Gate

- Propiedades: `sonar-project.properties` (default `sonar.projectKey=rer-engine`)
- CI: `.github/workflows/quality.yml` (`windows-latest`; SonarScanner CLI + `sonar.qualitygate.wait=true`)

Secrets / variables de GitHub:

| Nombre | Tipo | Uso |
|--------|------|-----|
| `SONAR_TOKEN` | secret | Token SonarCloud |
| `SONAR_ORGANIZATION` | variable | Organización SonarCloud |
| `SONAR_PROJECT_KEY` | variable | Si vacío, usa `sonar-project.properties` |

En SonarCloud, configura un Quality Gate estricto para **New Code**:

- 0 bugs críticos / bloqueantes
- 0 vulnerabilidades críticas / bloqueantes
- Security Hotspots revisados
- Maintainability rating A en código nuevo
- Cobertura en código nuevo ≥ umbral (cuando haya tests aplicables)
- Duplicación y complejidad cognitiva bajo umbral en código nuevo

## Qué se analiza / qué no

**Sí:** `src/main` (TS Electron, excl. crates Rust), `src/preload`, `src/renderer`, `src/shared-types`.

**No:** `src/main/Engine/engine_*`, `target/`, `Models/`, `assets/` del motor.

## Agentes de IA

Ver `.cursor/rules/frontend-quality.mdc`: no dar por terminada una tarea frontend sin `yarn quality` en verde; no debilitar reglas ni ocultar errores con `any` / `@ts-ignore` / `eslint-disable`.
