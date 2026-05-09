# Code Review Report

Resumen de hallazgos detectados durante la revisión del motor Rust y el frontend React/TypeScript.

MEDIO | Rust | apply_redo() deja RemoveEntity como no-op silencioso, rompiendo la expectativa de rehacer. |
MEDIO | Rust | Los scripts Lua corren también en modo editor, con riesgo de efectos laterales inesperados. |
BAJO | Rust | new_entity_id() tiene coste O(n) en ciertos escenarios de crecimiento. |
BAJO | Rust | autosave_last_tick tiene semántica poco clara entre inicialización y activación. |
BAJO | Rust | query_ctrl_held_x11 tiene nombre engañoso en Windows. |
MEDIO | React | createEngineEventHandler instancia un Set nuevo en cada evento IPC. |
BAJO | React | La restauración inicial del proyecto está demasiado acoplada al handler de eventos. |
BAJO | React | window.engine.off() puede limpiar más listeners de los necesarios según implementación. |


PENDIENTES:
MEDIO | Rust | El atlas de texturas no libera ni reutiliza entradas y puede agotarse de forma silenciosa.