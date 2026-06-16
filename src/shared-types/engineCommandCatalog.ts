import type { ProjectType } from './types'

/**
 * Nombres `cmd` IPC alineados con Rust:
 * - `engine_ipc_common::EngineCommandCommon`
 * - `engine_2d/src/engine_command.rs` (Only2d)
 * - `engine_3d/src/engine_command.rs` (Only3d)
 *
 * Fuente única para main (`engineCommandCatalog.ts`) y tipos TS (`engineCommands.ts`).
 */

export const ENGINE_COMMANDS_SHARED = [
  'ping',
  'shutdown',
  'set_clear_color',
  'resize',
  'set_bounds',
  'load_model',
  'replace_entity_model',
  'set_transform',
  'set_entity_name',
  'set_scene',
  'remove_entity',
  'deselect_entity',
  'set_world_size',
  'set_gravity',
  'set_grid_visible',
  'set_grid_cell_size',
  'set_target_fps',
  'set_ctrl_held',
  'set_physics',
  'set_active_tool',
  'play_audio',
  'stop_audio',
  'set_animation',
  'remove_animation',
  'set_default_animation',
  'play_animation',
  'stop_animation',
  'load_script',
  'run_control_script',
  'set_control_bindings',
  'unload_script',
  'load_scene_visual_script',
  'load_sprite',
  'remove_sprite',
  'get_sprites_list',
  'load_sound',
  'remove_sound',
  'get_sounds_list',
  'load_font',
  'remove_font',
  'get_fonts_list',
  'load_hud_image',
  'remove_hud_image',
  'get_hud_images_list',
  'load_background_asset',
  'remove_background_asset',
  'get_backgrounds_list',
  'set_debug_mode',
  'set_preview_playing',
  'set_player_ui_edit_mode',
  'add_player_ui_text_box',
  'remove_player_ui_text_box',
  'add_player_ui_button',
  'remove_player_ui_button',
  'add_player_ui_image',
  'remove_player_ui_image',
  'set_player_ui_object_draw',
  'remove_player_ui_object',
  'set_player_ui_hud_element_props',
  'set_player_ui_object_style',
  'sync_player_ui_screens',
  'set_active_player_ui_screen',
  'undo',
  'redo',
  'reload_asset',
  'set_locale',
  'set_autosave',
  'export_save_snapshot',
  'get_default_scene_name',
  'resend_all_model_clips',
  'apply_entity_restore',
] as const

/** Solo `rer_engine_2d` (`EngineCommand2dOnly`). */
export const ENGINE_COMMANDS_2D_ONLY = [
  'load_scenario',
  'set_scenario_scale',
  'load_character',
  'set_character_scale',
  'clear_background',
  'play_animation_frame',
  'restore_animation_frame',
  'load_background',
  'set_camera_2d',
  'set_pivot_edit_mode',
  'cancel_pivot_edit_mode',
  'set_logical_area_mode',
  'cancel_logical_area_mode',
  'create_collider_from_points',
  'create_execution_area_from_points',
  'set_vsync',
  'import_scene',
] as const

/** Solo `rer_engine_3d` (`EngineCommand3dOnly`). */
export const ENGINE_COMMANDS_3D_ONLY = [
  'spawn_cached_model',
  'spawn_quick_build_instance',
  'place_quick_build_at_cursor',
  'register_blueprint',
  'load_model_asset',
  'remove_model_asset',
  'get_models_list',
  'spawn_editor_box',
  'spawn_sun',
  'spawn_ground',
  'set_directional_light',
  'load_character',
  'set_play_character_spawn',
  'set_play_character_view',
  'set_camera_fov',
  'set_play_editor_frustum_distance',
  'set_entity_colision',
  'set_graphics_texture_tier',
  'set_texture_detail_distance',
  'create_editor_scene',
  'switch_editor_scene',
  'delete_editor_scene',
  'notify_project_saved',
  'clear_editor_undo_redo',
] as const

function toSet<T extends string>(items: readonly T[]): ReadonlySet<string> {
  return new Set(items)
}

export const ENGINE_COMMAND_SET_2D = toSet([
  ...ENGINE_COMMANDS_SHARED,
  ...ENGINE_COMMANDS_2D_ONLY,
])

export const ENGINE_COMMAND_SET_3D = toSet([
  ...ENGINE_COMMANDS_SHARED,
  ...ENGINE_COMMANDS_3D_ONLY,
])

export function engineCommandSetFor(projectType: ProjectType): ReadonlySet<string> {
  return projectType === '3D' ? ENGINE_COMMAND_SET_3D : ENGINE_COMMAND_SET_2D
}

export function isCommandAllowedForMotor(cmd: string, projectType: ProjectType | null): boolean {
  if (!projectType) return cmd === 'set_locale'
  return engineCommandSetFor(projectType).has(cmd)
}
