# Engine branding

Shared runtime assets that belong to RER-ENGINE, not to a project `.save`.

## `RER_ENGINE_Intro_4K.mp4`

Canonical 4K intro for a **shipped game** (standalone `rer_engine_2d` / `rer_engine_3d`).

When the **final game build** is configured, copy or embed this file with the executable and play it at process start so the player sees that the title was made with RER-ENGINE.

| | |
|---|---|
| Status | Asset only — playback is **not** implemented |
| When | Game export / final build pipeline (not the editor installer) |
| Scope | 2D and 3D shipped runtimes |
| Not for | `rer_engine_editor`, `yarn dev`, `yarn start`, Electron `yarn dist` |

Do not replace this with a per-project splash from the `.save`. Project intros, if any, come later and stay separate from this engine credit.
