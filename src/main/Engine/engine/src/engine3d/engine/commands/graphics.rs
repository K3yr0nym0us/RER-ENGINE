use super::*;

impl State {
    pub(super) fn cmd_graphics(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetGraphicsTextureTier { tier }) => {
                if let Some(t) =
                    crate::engine3d::config_3d::texture_graphics::TextureGraphicsTier::from_wire(
                        &tier,
                    )
                {
                    self.set_graphics_texture_tier(t);
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetTextureDetailDistance { distance_m }) => {
                self.set_texture_detail_near_m(distance_m);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionTier { tier }) => {
                log::info!("[reflexiones] IPC set_reflection_tier: {tier}");
                if let Some(t) =
                    crate::engine3d::config_3d::reflection_graphics::ReflectionTier::from_wire(
                        &tier,
                    )
                {
                    self.set_reflection_tier(t);
                } else {
                    log::warn!("[reflexiones] tier IPC no reconocido: {tier}");
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionRaytracing { enabled }) => {
                log::info!("[reflexiones] IPC set_reflection_raytracing: {enabled}");
                self.set_reflection_raytracing(enabled);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionProbes { enabled }) => {
                log::info!("[reflexiones] IPC set_reflection_probes: {enabled}");
                self.set_reflection_probes(enabled);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SpawnReflectionProbe { position }) => {
                log::info!("[reflexiones] IPC spawn_reflection_probe");
                let _ = self.spawn_reflection_probe(position);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionDebugView { view }) => {
                if let Some(v) =
                    crate::engine3d::config_3d::reflection_graphics::ReflectionDebugView::from_wire(
                        &view,
                    )
                {
                    self.set_reflection_debug_view(v);
                } else {
                    log::warn!("[reflexiones] vista debug IPC no reconocida: {view}");
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetSsrDebugMode { enabled }) => {
                self.set_reflection_debug_view(if enabled {
                    crate::engine3d::config_3d::reflection_graphics::ReflectionDebugView::SsrDebug
                } else {
                    crate::engine3d::config_3d::reflection_graphics::ReflectionDebugView::Final
                });
                send_event(&EngineEvent::SsrDebugModeChanged { enabled });
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetShadowTier { tier }) => {
                log::info!("[sombras] IPC set_shadow_tier: {tier}");
                if let Some(t) =
                    crate::engine3d::config_3d::shadow_graphics::ShadowTier::from_wire(&tier)
                {
                    self.set_shadow_tier(t);
                } else {
                    log::warn!("[sombras] tier IPC no reconocido: {tier}");
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetMsaaTier { tier }) => {
                log::info!("[msaa] IPC set_msaa_tier: {tier}");
                if let Some(t) =
                    crate::engine3d::config_3d::msaa_graphics::MsaaTier::from_wire(&tier)
                {
                    self.set_msaa_tier(t);
                } else {
                    log::warn!("[msaa] tier IPC no reconocido: {tier}");
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetWorldRadius { radius }) => {
                self.set_world_bounds_3d_radius(radius);
                self.clamp_play_character_camera_to_bounds();
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetTaa {
                enabled,
                blend,
                jitter_scale,
            }) => {
                // TAA IPC log suppressed
                self.set_taa(enabled, blend, jitter_scale);
                send_event(&EngineEvent::TaaChanged {
                    enabled,
                    blend,
                    jitter_scale,
                });
            }
            EngineCommand::Common(EngineCommandCommon::SetDebugMode { show }) => {
                self.debug_mode = show;
                log::info!("[debug] modo debug (colisiones): {}", show);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetCameraFov { fov_y }) => {
                self.camera.fov_y = fov_y.clamp(0.1, std::f32::consts::FRAC_PI_2 - 0.01);
                if self.has_play_character() {
                    self.emit_play_character_view_changed(false);
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetPlayEditorFrustumDistance {
                distance,
            }) => {
                self.fps_editor_frustum_distance = distance.clamp(0.5, 50.0);
                if self.has_play_character() {
                    self.emit_play_character_view_changed(false);
                }
            }
            other => return Some(other),
        }
        None
    }
}
