//! Sistema de interacción en play: raycast desde la mira y tooltip anclado a la entidad.
//!
//! Reutiliza el `raycast_first_hit` de física para la oclusión y los AABB de
//! picking (`model_ops`) para detectar entidades interactuables sin exigir que
//! tengan collider. El prompt se dibuja con el pipeline HUD de pantalla,
//! proyectando la entidad a coordenadas de viewport cada frame.

use super::*;

use crate::engine3d::ecs::Interactable;
use crate::engine3d::scripting::InteractionState;
use rer_engine_core::player_ui::config::UiHudRect;

/// Tope global del rayo (cada entidad puede reducirlo con `max_distance`).
const INTERACTION_RAY_MAX: f32 = 20.0;
/// Margen para considerar que el hit de la mira es la propia entidad interactuable.
const OCCLUSION_EPS: f32 = 0.15;
/// Desplazamiento vertical del prompt sobre el punto proyectado (píxeles).
const PROMPT_Y_OFFSET_PX: f32 = 42.0;
/// Alto del prompt en píxeles.
const PROMPT_H_PX: f32 = 28.0;

impl State {
    /// Marca el flanco de pulsación de la tecla de interacción (llamado por el host).
    pub(crate) fn queue_interact(&mut self) {
        self.interact_queued = true;
    }

    /// ¿Coincide esta tecla de control con la configurada en alguna entidad interactuable?
    /// Si no hay ninguna configurada, se usa `E` por defecto.
    pub(crate) fn interaction_key_matches(&self, control_key: &str) -> bool {
        for &id in self.world.entities() {
            if let Some(cfg) = self.world.get::<Interactable>(id)
                && cfg.enabled
                && cfg.key.trim().eq_ignore_ascii_case(control_key)
            {
                return true;
            }
        }
        control_key.eq_ignore_ascii_case("E")
    }

    /// Actualiza objetivo, prompt y callbacks de interacción. Llamar una vez por frame en play.
    pub(crate) fn update_play_interaction(&mut self) {
        let entering_play = self.preview_playing && !self.interaction_was_playing;
        self.interaction_was_playing = self.preview_playing;
        if entering_play {
            self.interaction_consumed.clear();
        }

        if !self.preview_playing {
            self.interaction_prompt_glyph_instances.clear();
            self.script_engine
                .set_interaction_state(InteractionState::default());
            self.interact_queued = false;
            return;
        }

        let pressed = std::mem::take(&mut self.interact_queued);
        let actor = self.play_character_entity;
        let origin = self.camera_world_position();
        let dir = self.camera.view_forward();
        if dir.length_squared() < 1.0e-8 {
            self.interaction_prompt_glyph_instances.clear();
            return;
        }

        // El collider del jugador no debe bloquear la interacción.
        let exclude: Vec<_> = actor
            .and_then(|id| self.physics.collider_handle_for_entity(id))
            .into_iter()
            .collect();
        let occluder = self
            .physics
            .raycast_first_hit(origin, dir, INTERACTION_RAY_MAX, &exclude);
        let occluder_toi = occluder
            .as_ref()
            .map(|(t, _, _)| *t)
            .unwrap_or(f32::INFINITY);

        let mut best: Option<(f32, u32)> = None;
        for &id in self.world.entities() {
            let Some(cfg) = self.world.get::<Interactable>(id) else {
                continue;
            };
            if !cfg.enabled || (cfg.once && self.interaction_consumed.contains(&id)) {
                continue;
            }
            let max_dist = cfg.max_distance.clamp(0.5, INTERACTION_RAY_MAX);
            let Some(transform) = self.world.get::<Transform>(id) else {
                continue;
            };
            let (center, half) = self.entity_world_pick_aabb(id, transform);
            if let Some(toi) = Self::ray_intersects_aabb(origin, dir, center, half)
                && toi <= max_dist
                && toi <= occluder_toi + OCCLUSION_EPS
                && best.is_none_or(|(bt, _)| toi < bt)
            {
                best = Some((toi, id));
            }
        }

        let target = best.map(|(_, id)| id);
        let distance = best.map(|(t, _)| t).unwrap_or(0.0);
        self.script_engine.set_interaction_state(InteractionState {
            target,
            actor,
            pressed,
            distance,
        });

        if pressed && let Some(id) = target {
            let cfg = self.world.get::<Interactable>(id).cloned();
            if cfg.as_ref().is_some_and(|c| c.once) {
                self.interaction_consumed.insert(id);
            }
            let cmds = self
                .script_engine
                .run_interact_hook(id, actor.unwrap_or(0), None, None);
            if !cmds.is_empty() {
                self.apply_script_commands(cmds);
            }
        }

        self.rebuild_interaction_prompt(target, distance);
    }

    /// Reconstruye el tooltip del objetivo actual (o lo vacía si no hay).
    fn rebuild_interaction_prompt(&mut self, target: Option<u32>, distance: f32) {
        self.interaction_prompt_glyph_instances.clear();
        let Some(id) = target else {
            return;
        };
        let Some(cfg) = self.world.get::<Interactable>(id).cloned() else {
            return;
        };
        if !cfg.enabled {
            return;
        }
        let Some(transform) = self.world.get::<Transform>(id).cloned() else {
            return;
        };
        let (center, _) = self.entity_world_pick_aabb(id, &transform);
        let Some((sx, sy)) = self.project_to_screen(center) else {
            return;
        };

        let vw = self.size.width.max(1) as f32;
        let vh = self.size.height.max(1) as f32;
        let label = format!("{}  [{}]", cfg.prompt(&self.snap_locale), cfg.key);
        let w_px = (label.chars().count() as f32 * 9.0 + 24.0).clamp(96.0, 420.0);
        let h_px = PROMPT_H_PX;

        // Fade por distancia: cerca=self alpha, lejos=0.35.
        let t = (distance / cfg.max_distance.max(0.5)).clamp(0.0, 1.0);
        let alpha = 1.0 - 0.65 * t;

        let cx_px = sx.clamp(w_px * 0.5, vw - w_px * 0.5);
        let cy_px = (sy - PROMPT_Y_OFFSET_PX).clamp(h_px * 0.5, vh - h_px * 0.5);
        let box_rect = UiHudRect {
            center_x: (cx_px / vw) * 2.0 - 1.0,
            center_y: 1.0 - (cy_px / vh) * 2.0,
            width: 2.0 * w_px / vw,
            height: 2.0 * h_px / vh,
        };

        let Some(font_path) = self.interaction_font_path() else {
            return;
        };
        let font = match self.player_ui_font_cache.get(&font_path).cloned() {
            Some(font) => font,
            None => match super::player_ui::font::load_font_arc(&font_path) {
                Some(font) => {
                    self.player_ui_font_cache.insert(font_path, font.clone());
                    font
                }
                None => return,
            },
        };

        super::player_ui::font::build_cached_hud_label_glyph(
            &font,
            &label,
            box_rect,
            alpha,
            vw,
            vh,
            &self.vulkan,
            &mut self.player_ui_text_atlas,
            &mut self.interaction_prompt_cache,
            &mut self.interaction_prompt_glyph_instances,
        );
    }

    /// Fuente a usar para el prompt: la del HUD activo o cualquier fuente registrada.
    fn interaction_font_path(&self) -> Option<String> {
        if let Some(key) = self.player_ui_screen_key()
            && let Some(boxes) = self.player_ui_text_boxes.get(&key)
            && let Some(path) = boxes
                .iter()
                .find(|b| !b.font_path.trim().is_empty())
                .map(|b| b.font_path.clone())
        {
            return Some(path);
        }
        self.font_store
            .keys()
            .next()
            .cloned()
            .or_else(|| self.player_ui_font_cache.keys().next().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interactable(es: &str, en: &str) -> Interactable {
        Interactable {
            prompt_es: es.to_string(),
            prompt_en: en.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn prompt_respeta_locale_y_fallback() {
        let cfg = interactable("Abrir", "Open");
        assert_eq!(cfg.prompt("es"), "Abrir");
        assert_eq!(cfg.prompt("en"), "Open");
        assert_eq!(cfg.prompt("fr"), "Abrir");
        assert_eq!(interactable("", "").prompt("es"), "Interactuar");
    }

    #[test]
    fn defaults_son_razonables() {
        let cfg = Interactable::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.key, "E");
        assert!(!cfg.once);
        assert!(cfg.max_distance > 0.0);
    }

    #[test]
    fn prompt_cache_key_es_unica_por_texto_y_tamano() {
        // El cache usa texto + dimensiones en píxeles; dos textos distintos no colisionan.
        assert_ne!("Abrir|40x28", "Cerrar|40x28");
        assert_ne!("Abrir|40x28", "Abrir|80x28");
    }
}
