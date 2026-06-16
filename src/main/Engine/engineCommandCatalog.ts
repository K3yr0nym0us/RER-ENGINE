/**
 * Reexporta el catálogo IPC para validación en main (stdin JSON).
 * Fuente única: src/shared-types/engineCommandCatalog.ts
 */
export {
  ENGINE_COMMANDS_SHARED,
  ENGINE_COMMANDS_2D_ONLY,
  ENGINE_COMMANDS_3D_ONLY,
  ENGINE_COMMAND_SET_2D,
  ENGINE_COMMAND_SET_3D,
  engineCommandSetFor,
  isCommandAllowedForMotor,
} from '../../shared-types/engineCommandCatalog'
