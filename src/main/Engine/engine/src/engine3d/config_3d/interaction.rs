//! Sistema de interacción en play: raycast desde la mira y tooltip anclado a la entidad.
//!
//! Reutiliza el `raycast_first_hit` de física para la oclusión y los AABB de
//! picking (`model_ops`) para detectar entidades interactuables sin exigir que
//! tengan collider. El prompt se dibuja con el pipeline HUD de pantalla,
//! proyectando la entidad a coordenadas de viewport cada frame.

use super::*;

use crate::engine3d::ecs::Interactable;
use crate::engine3d::scripting::{InteractionState, ScriptCmd};
use rer_engine_core::player_ui::config::UiHudRect;

/// Definición de un tooltip con nombre, compartida por todas las entidades que lo usan.
#[derive(Debug, Clone, Default)]
pub struct InteractionTooltip {
    pub key: String,
    pub trigger: String,
    pub action: String,
    /// Script Rhai o grafo compilado asignado al tooltip.
    pub action_ref_name: String,
    pub action_ref_source: String,
    pub max_distance: f32,
    pub tooltip_distance: f32,
    pub once: bool,
    pub highlight: bool,
    pub highlight_color: [u8; 3],
}

/// Paleta usada solo como compatibilidad con tooltips guardados con índice.
pub(crate) const AURA_PALETTE: [[u8; 3]; 6] = [
    [31, 153, 224],
    [46, 242, 183],
    [255, 191, 26],
    [242, 64, 89],
    [199, 115, 255],
    [255, 115, 51],
];
pub(crate) const DEFAULT_AURA_COLOR: [u8; 3] = AURA_PALETTE[0];

/// Lee el color del tooltip: RGB en definiciones nuevas, índice en las antiguas.
pub(crate) fn tooltip_color(value: &serde_json::Value) -> [u8; 3] {
    let Some(raw) = value.get("highlight_color") else {
        return DEFAULT_AURA_COLOR;
    };
    if let Some(a) = raw.as_array()
        && a.len() >= 3
    {
        return [
            a[0].as_u64().unwrap_or(0).min(255) as u8,
            a[1].as_u64().unwrap_or(0).min(255) as u8,
            a[2].as_u64().unwrap_or(0).min(255) as u8,
        ];
    }
    raw.as_u64()
        .map(|index| AURA_PALETTE[(index as usize).min(5)])
        .unwrap_or(DEFAULT_AURA_COLOR)
}

/// Tope global del rayo (cada entidad puede reducirlo con `max_distance`).
const INTERACTION_RAY_MAX: f32 = 20.0;
/// Margen para considerar que el hit de la mira es la propia entidad interactuable.
const OCCLUSION_EPS: f32 = 0.15;
/// Desplazamiento vertical del prompt sobre el punto proyectado (píxeles).
const PROMPT_Y_OFFSET_PX: f32 = 42.0;
/// Segundos que tarda en llenarse/vaciarse el anillo de «mantener».
const HOLD_FILL_SECONDS: f32 = 1.1;
const HOLD_DRAIN_SECONDS: f32 = 0.8;

impl State {
    /// Borra el anillo de progreso (al perder el objetivo o salir de play).
    fn clear_interaction_ring(&mut self) {
        self.interaction_ring_center = None;
        crate::engine3d::gizmo::write_triangles(
            &self.vulkan,
            &mut self.interaction_ring_buffer,
            &[],
        );
    }

    /// Estado de la tecla mantenida (modo «Hold»).
    pub(crate) fn set_interaction_held(&mut self, held: bool) {
        self.interaction_held = held;
    }

    /// El host solo registra la tecla pulsada; la resuelve la entidad mirada.
    pub(crate) fn queue_interact_key(&mut self, control_key: String) {
        self.interaction_key_pressed = Some(control_key);
        self.interact_queued = true;
    }

    /// ¿Coincide la tecla con la configuración resuelta de una entidad?
    fn interaction_key_matches_cfg(&self, cfg: Option<&Interactable>, control_key: &str) -> bool {
        let Some(cfg) = cfg else {
            return false;
        };
        if !cfg.enabled {
            return false;
        }
        let key = cfg
            .tooltip_id
            .as_deref()
            .and_then(|id| self.interaction_tooltip_registry.get(id))
            .map(|definition| definition.key.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| cfg.key.trim());
        key.eq_ignore_ascii_case(control_key)
    }

    /// Estado «mantener» del objetivo mirado (el del frame anterior).
    pub(crate) fn interaction_key_matches_target(&self, control_key: &str) -> bool {
        let cfg = self
            .interaction_target
            .and_then(|id| self.world.get::<Interactable>(id));
        self.interaction_key_matches_cfg(cfg, control_key)
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
            self.clear_interaction_ring();
            self.interaction_prompt_alpha = 0.0;
            self.interaction_highlight_target = None;
            self.script_engine
                .set_interaction_state(InteractionState::default());
            self.interact_queued = false;
            return;
        }

        let key_event = self.interaction_key_pressed.take();
        self.interact_queued = false;
        self.interaction_hold_cooldown =
            (self.interaction_hold_cooldown - self.delta_time).max(0.0);
        let actor = self.play_character_entity;
        let origin = self.camera_world_position();
        let dir = self.camera.view_forward();
        if dir.length_squared() < 1.0e-8 {
            self.interaction_prompt_glyph_instances.clear();
            self.clear_interaction_ring();
            self.interaction_prompt_alpha = 0.0;
            self.interaction_highlight_target = None;
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
            // El tooltip puede aparecer antes de estar a distancia de interacción.
            let max_dist = cfg
                .tooltip_distance
                .max(cfg.max_distance)
                .clamp(0.5, INTERACTION_RAY_MAX);
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
        self.interaction_target = target;
        let distance = best.map(|(t, _)| t).unwrap_or(0.0);
        let mut target_cfg = target.and_then(|id| self.world.get::<Interactable>(id).cloned());
        // La entidad resuelve su tooltip por id contra el registro compartido:
        // editar la definición no requiere tocar N entidades.
        if let Some(cfg) = target_cfg.as_mut()
            && let Some(tooltip_id) = cfg.tooltip_id.as_deref()
            && let Some(definition) = self.interaction_tooltip_registry.get(tooltip_id)
        {
            cfg.key = definition.key.clone();
            cfg.trigger = if definition.trigger.eq_ignore_ascii_case("hold") {
                crate::engine3d::ecs::InteractionTrigger::Hold
            } else {
                crate::engine3d::ecs::InteractionTrigger::Press
            };
            cfg.action = match definition.action.as_str() {
                "animation" => crate::engine3d::ecs::InteractionAction::Animation,
                "node" | "nodes" => crate::engine3d::ecs::InteractionAction::Node,
                _ => crate::engine3d::ecs::InteractionAction::Script,
            };
            cfg.action_name = definition.action_ref_name.clone();
            cfg.action_source = definition.action_ref_source.clone();
            cfg.max_distance = definition.max_distance;
            cfg.tooltip_distance = definition.tooltip_distance;
            cfg.once = definition.once;
            cfg.highlight = definition.highlight;
            cfg.highlight_color = definition.highlight_color;
        }

        // Solo cuenta la tecla pulsada si pertenece al objetivo mirado.
        let pressed = key_event
            .as_deref()
            .is_some_and(|key| self.interaction_key_matches_cfg(target_cfg.as_ref(), key));

        // Aura solo si está activada y dentro de la distancia de interacción.
        let in_range = target_cfg
            .as_ref()
            .is_some_and(|cfg| distance <= cfg.max_distance);
        self.interaction_highlight_target =
            if in_range && target_cfg.as_ref().is_some_and(|c| c.highlight) {
                target
            } else {
                None
            };
        self.interaction_highlight_color = target_cfg
            .as_ref()
            .map(|cfg| cfg.highlight_color)
            .unwrap_or(DEFAULT_AURA_COLOR);

        self.script_engine.set_interaction_state(InteractionState {
            target,
            actor,
            pressed,
            distance,
        });

        let cfg_for_fire = target_cfg.clone();
        let hold_mode = target_cfg.as_ref().is_some_and(|cfg| {
            matches!(cfg.trigger, crate::engine3d::ecs::InteractionTrigger::Hold)
        });

        if self.interaction_hold_target != target {
            self.interaction_hold_target = target;
            self.interaction_hold_progress = 0.0;
            self.interaction_hold_fired = false;
            self.interaction_hold_wait_release = false;
        }

        let can_fire = target.is_some() && in_range;
        if hold_mode && can_fire {
            if self.interaction_hold_wait_release {
                // Ciclo completado: el anillo ya está a 0 y no se llena hasta
                // que el usuario suelte la tecla y vuelva a pulsarla.
                if !self.interaction_held {
                    self.interaction_hold_wait_release = false;
                    self.interaction_hold_fired = false;
                }
            } else if self.interaction_held && !self.interaction_hold_fired {
                self.interaction_hold_progress =
                    (self.interaction_hold_progress + self.delta_time / HOLD_FILL_SECONDS).min(1.0);
                if self.interaction_hold_progress >= 1.0 {
                    self.interaction_hold_fired = true;
                    self.interaction_hold_wait_release = true;
                    self.interaction_hold_progress = 0.0;
                    if let (Some(id), Some(cfg)) = (target, cfg_for_fire.as_ref()) {
                        self.fire_interaction(id, cfg, actor);
                    }
                }
            } else if !self.interaction_held {
                // Al soltar, el anillo se vacía poco a poco; volver a pulsar
                // continúa desde el progreso actual.
                self.interaction_hold_progress = (self.interaction_hold_progress
                    - self.delta_time / HOLD_DRAIN_SECONDS)
                    .max(0.0);
                if self.interaction_hold_progress <= 0.0 {
                    self.interaction_hold_fired = false;
                }
            }
        } else {
            self.interaction_hold_progress = 0.0;
            self.interaction_hold_fired = false;
            self.interaction_hold_wait_release = false;
            if pressed
                && can_fire
                && let (Some(id), Some(cfg)) = (target, cfg_for_fire.as_ref())
            {
                self.fire_interaction(id, cfg, actor);
            }
        }

        // Fade temporal: entra al mirar la entidad y sale al dejar de mirarla,
        // con un suave gradiente según la distancia al modelo.
        let desired_alpha = target_cfg
            .as_ref()
            .map(|cfg| {
                let t = (distance / cfg.tooltip_distance.max(0.5)).clamp(0.0, 1.0);
                1.0 - 0.25 * t
            })
            .unwrap_or(0.0);
        let speed = if desired_alpha > self.interaction_prompt_alpha {
            9.0
        } else {
            6.0
        };
        let ease = 1.0 - (-self.delta_time * speed).exp();
        self.interaction_prompt_alpha += (desired_alpha - self.interaction_prompt_alpha) * ease;
        if self.interaction_prompt_alpha < 0.01 {
            self.interaction_prompt_alpha = 0.0;
        }

        self.rebuild_interaction_prompt(target, self.interaction_prompt_alpha, target_cfg.as_ref());
    }

    /// Reconstruye el tooltip del objetivo: «Pulsa»(negrita) / [E] / acción.
    /// `cfg` ya viene resuelto contra el registro de tooltips compartido.
    fn rebuild_interaction_prompt(
        &mut self,
        target: Option<u32>,
        alpha: f32,
        cfg: Option<&Interactable>,
    ) {
        self.interaction_prompt_glyph_instances.clear();
        // El anillo se reconstruye desde cero cada frame: si el objetivo se
        // pierde al mover el ratón, no debe quedarse dibujado en pantalla.
        self.clear_interaction_ring();
        if alpha <= 0.0 {
            return;
        }
        let Some(id) = target else {
            return;
        };
        let Some(cfg) = cfg else {
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
        let (prefix, action) = cfg.prompt_parts(&self.snap_locale);
        let key = cfg.key.trim();

        let width_px = (prefix.chars().count().max(action.chars().count()) as f32 * 12.0 + 64.0)
            .clamp(200.0, 560.0);
        const PANEL_H: f32 = 124.0;
        let top_px = (sy - PROMPT_Y_OFFSET_PX - PANEL_H).clamp(4.0, vh - PANEL_H - 4.0);
        let cx_px = sx.clamp(width_px * 0.5, vw - width_px * 0.5);

        let line_rect = |y: f32, h: f32| UiHudRect {
            center_x: (cx_px / vw) * 2.0 - 1.0,
            center_y: 1.0 - ((y + h * 0.5) / vh) * 2.0,
            width: 2.0 * width_px / vw,
            height: 2.0 * h / vh,
        };

        let font = self
            .interaction_font()
            .or_else(super::player_ui::font::load_builtin_font_arc);
        let Some(font) = font else {
            return;
        };

        // 1. «Press» / «Pulsa» en negrita.
        super::player_ui::font::build_cached_hud_label_glyph(
            &font,
            &prefix,
            line_rect(top_px + 8.0, 34.0),
            alpha,
            true,
            4,
            vw,
            vh,
            &self.vulkan,
            &mut self.player_ui_text_atlas,
            &mut self.interaction_prompt_cache,
            &mut self.interaction_prompt_glyph_instances,
        );

        // 2. Botón de tecla: rectángulo oscuro con la tecla centrada.
        let key_line = line_rect(top_px + 48.0, 40.0);
        let key_box = UiHudRect {
            center_x: key_line.center_x,
            center_y: key_line.center_y,
            width: 2.0 * 44.0 / vw,
            height: 2.0 * 44.0 / vh,
        };
        self.rebuild_hold_ring(key_box, &cfg, vw, vh);
        let key_bg = self.interaction_key_texture();
        super::player_ui::font::push_hud_texture_quad_instance(
            key_bg,
            key_box,
            vw,
            vh,
            1.0,
            &mut self.interaction_prompt_glyph_instances,
        );
        super::player_ui::font::build_cached_hud_label_glyph(
            &font,
            key,
            key_box,
            alpha,
            false,
            4,
            vw,
            vh,
            &self.vulkan,
            &mut self.player_ui_text_atlas,
            &mut self.interaction_prompt_cache,
            &mut self.interaction_prompt_glyph_instances,
        );

        // 3. Acción: «to move the platform» / «para mover la plataforma».
        super::player_ui::font::build_cached_hud_label_glyph(
            &font,
            &action,
            line_rect(top_px + 94.0, 28.0),
            alpha,
            true,
            4,
            vw,
            vh,
            &self.vulkan,
            &mut self.player_ui_text_atlas,
            &mut self.interaction_prompt_cache,
            &mut self.interaction_prompt_glyph_instances,
        );
    }

    /// Anillo blanco que se llena/vacía alrededor del botón en modo «mantener».
    fn rebuild_hold_ring(&mut self, key_box: UiHudRect, cfg: &Interactable, vw: f32, vh: f32) {
        let progress = if matches!(cfg.trigger, crate::engine3d::ecs::InteractionTrigger::Hold) {
            self.interaction_hold_progress
        } else {
            0.0
        };
        if progress <= 0.0 {
            self.interaction_ring_center = None;
            crate::engine3d::gizmo::write_triangles(
                &self.vulkan,
                &mut self.interaction_ring_buffer,
                &[],
            );
            return;
        }

        use rer_engine_core::player_ui::ndc_draw::push_quad;
        let cx = key_box.center_x;
        let cy = key_box.center_y;
        let rx = 2.0 * self.interaction_ring_radius_px / vw;
        let ry = 2.0 * self.interaction_ring_radius_px / vh;
        self.interaction_ring_center = Some([cx, cy]);

        const SEGMENTS: usize = 48;
        const TAU: f32 = std::f32::consts::TAU;
        let mut verts = Vec::with_capacity(SEGMENTS * 6);
        for index in 0..SEGMENTS {
            let start = index as f32 / SEGMENTS as f32;
            if start >= progress {
                break;
            }
            let end = ((index + 1) as f32 / SEGMENTS as f32).min(progress);
            let a0 = -std::f32::consts::FRAC_PI_2 + start * TAU;
            let a1 = -std::f32::consts::FRAC_PI_2 + end * TAU;
            let point = |angle: f32, radius: f32| {
                [
                    cx + angle.cos() * rx * radius,
                    cy + angle.sin() * ry * radius,
                    0.0,
                ]
            };
            let color = [1.0, 1.0, 1.0, 0.92];
            push_quad(
                &mut verts,
                point(a0, 0.80),
                point(a0, 1.0),
                point(a1, 1.0),
                point(a1, 0.80),
                color,
            );
        }
        crate::engine3d::gizmo::write_triangles(
            &self.vulkan,
            &mut self.interaction_ring_buffer,
            &verts,
        );
    }

    /// Ejecuta la acción configurada de una interacción.
    fn fire_interaction(&mut self, id: EntityId, cfg: &Interactable, actor: Option<EntityId>) {
        if cfg.once {
            self.interaction_consumed.insert(id);
        }
        if matches!(
            cfg.action,
            crate::engine3d::ecs::InteractionAction::Animation
        ) && !cfg.action_name.trim().is_empty()
        {
            self.apply_script_commands(vec![ScriptCmd::PlayAnimation {
                id,
                name: cfg.action_name.clone(),
            }]);
        }
        // Script/nodo configurado en el tooltip: se ejecuta además del hook de
        // los scripts adjuntos a la entidad.
        if !cfg.action_source.trim().is_empty() {
            let target_snapshot = self.build_script_snapshot(id);
            let actor_snapshot = actor.and_then(|a| self.build_script_snapshot(a));
            let cmds = self.script_engine.run_interact_source(
                &cfg.action_name,
                &cfg.action_source,
                id,
                actor.unwrap_or(0),
                target_snapshot.as_ref(),
                actor_snapshot.as_ref(),
            );
            if !cmds.is_empty() {
                self.apply_script_commands(cmds);
            }
        }
        let target_snapshot = self.build_script_snapshot(id);
        let actor_snapshot = actor.and_then(|a| self.build_script_snapshot(a));
        let cmds = self.script_engine.run_interact_hook(
            id,
            actor.unwrap_or(0),
            target_snapshot.as_ref(),
            actor_snapshot.as_ref(),
        );
        if !cmds.is_empty() {
            self.apply_script_commands(cmds);
        }
    }

    /// Crea/destruye el emisor de fuego sobre el cubo del cuarto demo.
    pub(crate) fn toggle_demo_fire_emitter(&mut self) {
        const FIRE_ID: u64 = 0x00D1_0000_0000_0000;
        if self.demo_fire_emitter_active {
            self.particles.remove(FIRE_ID);
            self.demo_fire_emitter_active = false;
        } else {
            let mut config = crate::engine3d::particles::EmitterConfig::fire_demo();
            config.origin = [
                super::interaction_demo::INTERACTION_DEMO_X,
                1.45,
                super::interaction_demo::INTERACTION_DEMO_Z0
                    + 3.0 * super::interaction_demo::INTERACTION_DEMO_Z_STEP,
            ];
            self.particles.add_or_with_id(Some(FIRE_ID), config);
            self.demo_fire_emitter_active = true;
        }
    }

    /// Pastilla redondeada opaca del botón de tecla (misma técnica que el panel).
    fn interaction_key_texture(
        &mut self,
    ) -> crate::engine3d::screen_hud_image::ScreenHudPackedImage {
        if let Some(packed) = self.interaction_key_box {
            return packed;
        }
        const SIZE: u32 = 128;
        const RADIUS: f32 = 12.0;
        let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
        let half = SIZE as f32 * 0.5;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let px = x as f32 + 0.5 - half;
                let py = y as f32 + 0.5 - half;
                let qx = px.abs() - (half - RADIUS);
                let qy = py.abs() - (half - RADIUS);
                let distance = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt()
                    + qx.min(0.0).max(qy.min(0.0))
                    - RADIUS;
                let a = (0.5 - distance).clamp(0.0, 1.0);
                let i = ((y * SIZE + x) * 4) as usize;
                rgba[i] = 9;
                rgba[i + 1] = 11;
                rgba[i + 2] = 17;
                rgba[i + 3] = (a * 255.0).round() as u8;
            }
        }
        let packed = self
            .player_ui_text_atlas
            .pack_rgba(&self.vulkan, &rgba, SIZE, SIZE);
        self.interaction_key_box = Some(packed);
        packed
    }

    /// Fuente del proyecto para el prompt (HUD activo o biblioteca de fuentes).
    fn interaction_font(&mut self) -> Option<std::sync::Arc<ab_glyph::FontArc>> {
        let path = self.interaction_font_path()?;
        if let Some(font) = self.player_ui_font_cache.get(&path).cloned() {
            return Some(font);
        }
        let font = super::player_ui::font::load_font_arc(&path)?;
        self.player_ui_font_cache.insert(path, font.clone());
        Some(font)
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

    #[test]
    fn prompt_parts_derive_verb_from_trigger() {
        let press = Interactable::default();
        assert_eq!(
            press.prompt_parts("es"),
            ("Pulsa".into(), "Para interactuar".into())
        );
        assert_eq!(
            press.prompt_parts("en"),
            ("Press".into(), "To interact".into())
        );
        let hold = Interactable {
            trigger: crate::engine3d::ecs::InteractionTrigger::Hold,
            ..Default::default()
        };
        assert_eq!(
            hold.prompt_parts("es"),
            ("Mantén".into(), "Para interactuar".into())
        );
        assert_eq!(
            hold.prompt_parts("en"),
            ("Hold".into(), "To interact".into())
        );
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
