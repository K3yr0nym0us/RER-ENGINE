//! Máscara del G-buffer 3D (Vulkan 1.3 dynamic rendering).
//!
//! Cinco ranuras fijas (`colorAttachmentCount = 5`, `location` 0..4 en los shaders). Las ranuras
//! apagadas usan `vk::Format::UNDEFINED` en el pipeline y `vk::RenderingAttachmentInfo::default()`
//! al grabar el pase (view nulo, layout UNDEFINED, resolve NONE). Las sondas viven siempre en
//! `COLOR` solo; el G-buffer de pantalla sigue `from_settings(taa, ssr, rt)`.

use ash::vk;

use super::pipelines::{
    empty_vertex, graphics, graphics_blend_color_only, graphics_no_depth_test, skinned_vertex,
    static_vertex, with_samples,
};
use super::{COLOR, DEPTH, Pipes, VELOCITY, VulkanError, WORLD};

/// Bits de adjuntos de color del G-buffer. El bit `COLOR` está siempre activo.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct GbufferMask(u8);

impl GbufferMask {
    pub(super) const COLOR: u8 = 1 << 0;
    pub(super) const NORMAL: u8 = 1 << 1;
    pub(super) const ALBEDO: u8 = 1 << 2;
    pub(super) const WORLD: u8 = 1 << 3;
    pub(super) const VELOCITY: u8 = 1 << 4;

    pub(super) const fn color_only() -> Self {
        Self(Self::COLOR)
    }

    /// Color siempre; Normal+Albedo+World si SSR o RT; Velocity si TAA.
    pub(super) fn from_settings(taa: bool, ssr: bool, rt: bool) -> Self {
        let mut bits = Self::COLOR;
        if ssr || rt {
            bits |= Self::NORMAL | Self::ALBEDO | Self::WORLD;
        }
        if taa {
            bits |= Self::VELOCITY;
        }
        Self(bits)
    }

    pub(super) const fn bits(self) -> u8 {
        self.0
    }

    pub(super) const fn contains(self, bit: u8) -> bool {
        self.0 & bit == bit && bit != 0
    }

    /// Formatos de las 5 ranuras. Inactivas: `UNDEFINED` (el shader conserva `location` 0..4).
    pub(super) fn formats(self) -> [vk::Format; 5] {
        [
            Self::slot_format(self.contains(Self::COLOR), COLOR),
            Self::slot_format(self.contains(Self::NORMAL), COLOR),
            Self::slot_format(self.contains(Self::ALBEDO), COLOR),
            Self::slot_format(self.contains(Self::WORLD), WORLD),
            Self::slot_format(self.contains(Self::VELOCITY), VELOCITY),
        ]
    }

    const fn slot_format(active: bool, format: vk::Format) -> vk::Format {
        if active {
            format
        } else {
            vk::Format::UNDEFINED
        }
    }
}

/// Pipelines forward de una máscara concreta, fuera de `pipe_handles` / `self.pipelines`.
pub(super) struct GbufferPipeSet {
    forward: vk::Pipeline,
    forward_blend: vk::Pipeline,
    skinned: vk::Pipeline,
    skinned_blend: vk::Pipeline,
    /// Quad de cielo MSAA. Nulo en 1× (el fondo se copia, no se rasteriza).
    sky: vk::Pipeline,
}

impl Default for GbufferPipeSet {
    fn default() -> Self {
        Self::null()
    }
}

impl GbufferPipeSet {
    pub(super) fn overlay(&self, mut base: Pipes) -> Pipes {
        base.forward = self.forward;
        base.forward_blend = self.forward_blend;
        base.skinned = self.skinned;
        base.skinned_blend = self.skinned_blend;
        if self.sky != vk::Pipeline::null() {
            base.sky = self.sky;
        }
        base
    }

    pub(super) fn null() -> Self {
        Self {
            forward: vk::Pipeline::null(),
            forward_blend: vk::Pipeline::null(),
            skinned: vk::Pipeline::null(),
            skinned_blend: vk::Pipeline::null(),
            sky: vk::Pipeline::null(),
        }
    }

    pub(super) fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.forward, None);
            device.destroy_pipeline(self.forward_blend, None);
            device.destroy_pipeline(self.skinned, None);
            device.destroy_pipeline(self.skinned_blend, None);
            if self.sky != vk::Pipeline::null() {
                device.destroy_pipeline(self.sky, None);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn create(
        device: &ash::Device,
        forward_layout: vk::PipelineLayout,
        skinned_layout: vk::PipelineLayout,
        forward_vert: vk::ShaderModule,
        forward_frag: vk::ShaderModule,
        skinned_vert: vk::ShaderModule,
        formats: &[vk::Format; 5],
        samples: u32,
        sky: Option<(vk::PipelineLayout, vk::ShaderModule, vk::ShaderModule)>,
    ) -> Result<Self, VulkanError> {
        let static_v = if samples > 1 {
            with_samples(static_vertex(), samples)
        } else {
            static_vertex()
        };
        let skinned_v = if samples > 1 {
            with_samples(skinned_vertex(), samples)
        } else {
            skinned_vertex()
        };
        let mut keep = Vec::new();
        let built = (|| {
            let forward = graphics(
                device,
                forward_layout,
                forward_vert,
                Some(forward_frag),
                &static_v,
                formats,
                Some(DEPTH),
                false,
                false,
                &mut keep,
            )?;
            let forward_blend = graphics_blend_color_only(
                device,
                forward_layout,
                forward_vert,
                Some(forward_frag),
                &static_v,
                formats,
                Some(DEPTH),
                false,
                &mut keep,
            )?;
            let skinned = graphics(
                device,
                skinned_layout,
                skinned_vert,
                Some(forward_frag),
                &skinned_v,
                formats,
                Some(DEPTH),
                false,
                false,
                &mut keep,
            )?;
            let skinned_blend = graphics_blend_color_only(
                device,
                skinned_layout,
                skinned_vert,
                Some(forward_frag),
                &skinned_v,
                formats,
                Some(DEPTH),
                false,
                &mut keep,
            )?;
            let sky_pipe = if samples > 1 {
                if let Some((layout, vert, frag)) = sky {
                    graphics_no_depth_test(
                        device,
                        layout,
                        vert,
                        Some(frag),
                        &with_samples(empty_vertex(), samples),
                        formats,
                        Some(DEPTH),
                        &mut keep,
                    )?
                } else {
                    vk::Pipeline::null()
                }
            } else {
                vk::Pipeline::null()
            };
            Ok(Self {
                forward,
                forward_blend,
                skinned,
                skinned_blend,
                sky: sky_pipe,
            })
        })();
        if built.is_err() {
            for pipeline in keep {
                unsafe { device.destroy_pipeline(pipeline, None) };
            }
        }
        built
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_only_is_always_the_first_slot() {
        let mask = GbufferMask::color_only();
        assert!(mask.contains(GbufferMask::COLOR));
        assert!(!mask.contains(GbufferMask::NORMAL));
        assert!(!mask.contains(GbufferMask::ALBEDO));
        assert!(!mask.contains(GbufferMask::WORLD));
        assert!(!mask.contains(GbufferMask::VELOCITY));
        assert_eq!(
            mask.formats(),
            [
                COLOR,
                vk::Format::UNDEFINED,
                vk::Format::UNDEFINED,
                vk::Format::UNDEFINED,
                vk::Format::UNDEFINED,
            ]
        );
    }

    #[test]
    fn from_settings_always_keeps_color() {
        assert_eq!(
            GbufferMask::from_settings(false, false, false),
            GbufferMask::color_only()
        );
        let taa = GbufferMask::from_settings(true, false, false);
        assert!(taa.contains(GbufferMask::COLOR));
        assert!(taa.contains(GbufferMask::VELOCITY));
        assert!(!taa.contains(GbufferMask::NORMAL));
        assert_eq!(
            taa.formats(),
            [
                COLOR,
                vk::Format::UNDEFINED,
                vk::Format::UNDEFINED,
                vk::Format::UNDEFINED,
                VELOCITY,
            ]
        );
        let ssr = GbufferMask::from_settings(false, true, false);
        assert!(ssr.contains(GbufferMask::COLOR));
        assert!(ssr.contains(GbufferMask::NORMAL));
        assert!(ssr.contains(GbufferMask::ALBEDO));
        assert!(ssr.contains(GbufferMask::WORLD));
        assert!(!ssr.contains(GbufferMask::VELOCITY));
        assert_eq!(
            ssr.formats(),
            [COLOR, COLOR, COLOR, WORLD, vk::Format::UNDEFINED]
        );
        let rt = GbufferMask::from_settings(false, false, true);
        assert_eq!(rt, ssr);
        let all = GbufferMask::from_settings(true, true, true);
        assert_eq!(all.formats(), [COLOR, COLOR, COLOR, WORLD, VELOCITY]);
        assert_eq!(all.bits(), 0b1_1111);
    }

    #[test]
    fn unused_slots_are_undefined_and_mask_is_hashable() {
        let mut map = std::collections::HashMap::new();
        map.insert((GbufferMask::color_only(), 1u32), 1);
        map.insert((GbufferMask::from_settings(true, false, false), 4), 2);
        assert_eq!(map.len(), 2);
        assert!(!GbufferMask::from_settings(false, false, false).contains(0));
    }
}
