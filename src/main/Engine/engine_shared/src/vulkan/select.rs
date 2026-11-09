//! Selección de extensiones, formato de superficie y alineo del SBT.
//! No abre un device: los tests cubren estas decisiones sin GPU.

/// Extensiones de dispositivo que hacen falta juntas para `VK_KHR_ray_tracing_pipeline`.
pub const RT_DEVICE_EXTENSIONS: &[&str] = &[
    "VK_KHR_acceleration_structure",
    "VK_KHR_ray_tracing_pipeline",
    "VK_KHR_deferred_host_operations",
];

/// Nombres de extensiones pedidas que no están en `available`.
pub fn missing_extensions<'a>(required: &[&'a str], available: &[String]) -> Vec<&'a str> {
    required
        .iter()
        .copied()
        .filter(|need| !available.iter().any(|got| got == need))
        .collect()
}

pub fn hw_rt_supported(available: &[String]) -> bool {
    missing_extensions(RT_DEVICE_EXTENSIONS, available).is_empty()
}

/// Formatos sRGB que el swapchain del editor acepta, en orden de preferencia.
pub fn is_srgb_format(format: ash::vk::Format) -> bool {
    matches!(
        format,
        ash::vk::Format::B8G8R8A8_SRGB
            | ash::vk::Format::R8G8B8A8_SRGB
            | ash::vk::Format::A8B8G8R8_SRGB_PACK32
    )
}

/// Mismo criterio que el init wgpu: primer formato sRGB, si no el primero de la lista.
pub fn pick_surface_format(
    formats: &[ash::vk::SurfaceFormatKHR],
) -> Option<ash::vk::SurfaceFormatKHR> {
    if formats.is_empty() {
        return None;
    }
    const PREFERRED: [ash::vk::Format; 2] = [
        ash::vk::Format::B8G8R8A8_SRGB,
        ash::vk::Format::R8G8B8A8_SRGB,
    ];
    for want in PREFERRED {
        if let Some(found) = formats.iter().copied().find(|fmt| fmt.format == want) {
            return Some(found);
        }
    }
    formats
        .iter()
        .copied()
        .find(|fmt| is_srgb_format(fmt.format))
        .or_else(|| formats.first().copied())
}

/// `MAILBOX` si existe; si no, `FIFO` (siempre presente en una surface válida).
pub fn pick_present_mode(
    modes: &[ash::vk::PresentModeKHR],
    vsync: bool,
) -> ash::vk::PresentModeKHR {
    if vsync {
        return ash::vk::PresentModeKHR::FIFO;
    }
    if modes.contains(&ash::vk::PresentModeKHR::MAILBOX) {
        ash::vk::PresentModeKHR::MAILBOX
    } else if modes.contains(&ash::vk::PresentModeKHR::IMMEDIATE) {
        ash::vk::PresentModeKHR::IMMEDIATE
    } else {
        ash::vk::PresentModeKHR::FIFO
    }
}

/// Alinea `value` hacia arriba a un múltiplo de `align` (el stride del SBT).
pub fn align_up(value: u32, align: u32) -> u32 {
    if align <= 1 {
        return value;
    }
    value.div_ceil(align) * align
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hw_rt_requires_all_three_extensions() {
        let partial = vec!["VK_KHR_acceleration_structure".to_string()];
        assert!(!hw_rt_supported(&partial));
        let missing = missing_extensions(RT_DEVICE_EXTENSIONS, &partial);
        assert!(missing.contains(&"VK_KHR_ray_tracing_pipeline"));
        assert!(missing.contains(&"VK_KHR_deferred_host_operations"));

        let full: Vec<String> = RT_DEVICE_EXTENSIONS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert!(hw_rt_supported(&full));
        assert!(missing_extensions(RT_DEVICE_EXTENSIONS, &full).is_empty());
    }

    #[test]
    fn surface_format_prefers_bgra_srgb() {
        let formats = [
            ash::vk::SurfaceFormatKHR {
                format: ash::vk::Format::B8G8R8A8_UNORM,
                color_space: ash::vk::ColorSpaceKHR::SRGB_NONLINEAR,
            },
            ash::vk::SurfaceFormatKHR {
                format: ash::vk::Format::B8G8R8A8_SRGB,
                color_space: ash::vk::ColorSpaceKHR::SRGB_NONLINEAR,
            },
        ];
        let picked = pick_surface_format(&formats).expect("formato");
        assert_eq!(picked.format, ash::vk::Format::B8G8R8A8_SRGB);
    }

    #[test]
    fn surface_format_falls_back_to_first_when_no_srgb() {
        let formats = [ash::vk::SurfaceFormatKHR {
            format: ash::vk::Format::B8G8R8A8_UNORM,
            color_space: ash::vk::ColorSpaceKHR::SRGB_NONLINEAR,
        }];
        let picked = pick_surface_format(&formats).expect("formato");
        assert_eq!(picked.format, ash::vk::Format::B8G8R8A8_UNORM);
        assert!(pick_surface_format(&[]).is_none());
    }

    #[test]
    fn present_mode_mailbox_without_vsync() {
        let modes = [
            ash::vk::PresentModeKHR::FIFO,
            ash::vk::PresentModeKHR::MAILBOX,
        ];
        assert_eq!(
            pick_present_mode(&modes, false),
            ash::vk::PresentModeKHR::MAILBOX
        );
        assert_eq!(
            pick_present_mode(&modes, true),
            ash::vk::PresentModeKHR::FIFO
        );
    }

    #[test]
    fn sbt_handle_is_aligned() {
        assert_eq!(align_up(32, 64), 64);
        assert_eq!(align_up(64, 64), 64);
        assert_eq!(align_up(1, 32), 32);
        assert_eq!(align_up(0, 32), 0);
    }
}
