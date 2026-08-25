//! Ajustes de calidad de sombras (tier UI): recrea el shadow map a la resolución del tier.

use crate::config_3d::shadow_graphics::{DEFAULT_SHADOW_TIER, ShadowTier};
use crate::engine::{DEPTH_FORMAT, State};
use crate::ipc::{EngineEvent, send_event};

impl State {
    pub(crate) fn set_shadow_tier(&mut self, tier: ShadowTier) {
        let prev = self.shadow_tier;
        if prev == tier {
            log::info!("[sombras] Nivel de sombras sin cambios: {}", tier.wire());
            return;
        }
        self.shadow_tier = tier;
        let new_size = tier.shadow_map_size();
        if new_size != self.shadow_map_size {
            self.recreate_shadow_map(new_size);
        }
        send_event(&EngineEvent::ShadowTierChanged {
            tier: tier.wire().to_string(),
        });
        log::info!(
            "[sombras] Nivel de sombras: {} -> {} ({} px)",
            prev.wire(),
            tier.wire(),
            self.shadow_map_size
        );
    }

    /// Recrea el shadow map a `size` px y reconstruye los bind groups que lo referencian
    /// (escena y HUD). El pase de sombras crea su vista desde `_shadow_texture` cada frame, así
    /// que basta con sustituir la textura. El cubemap de probes captura con sombras, por lo que
    /// también se recrea (sus bind groups apuntaban a la vista anterior).
    pub(crate) fn recreate_shadow_map(&mut self, size: u32) {
        let size = size.clamp(1, 8192);
        let shadow_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow-map"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("shadow-map-view"),
            ..Default::default()
        });
        self.scene_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-bg"),
            layout: &self.scene_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.scene_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                },
            ],
        });
        self.hud_scene_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud-scene-bg"),
            layout: &self.scene_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.hud_scene_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                },
            ],
        });
        self._shadow_texture = shadow_texture;
        self.shadow_map_size = size;
        self.rebuild_probe_env(self.probe_cubemap_size);
    }
}

pub(crate) fn apply_shadow_settings_from_world_wire(state: &mut State, tier: Option<&str>) {
    let resolved = tier
        .and_then(ShadowTier::from_wire)
        .unwrap_or(DEFAULT_SHADOW_TIER);
    if state.shadow_tier != resolved {
        state.set_shadow_tier(resolved);
    }
}
