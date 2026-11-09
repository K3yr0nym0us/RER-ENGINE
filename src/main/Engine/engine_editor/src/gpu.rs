//! Inicialización de wgpu para el editor (Vulkan exclusivo, igual que el motor).

use std::sync::Arc;

use winit::window::Window;

pub struct Gpu {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

pub async fn init(window: Arc<Window>) -> Result<Gpu, String> {
    let instance = wgpu::Instance::new({
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::VULKAN;
        descriptor
    });

    let surface = instance
        .create_surface(window.clone())
        .map_err(|error| format!("No se pudo crear la superficie del editor: {error}"))?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .map_err(|error| format!("No se encontró un adaptador Vulkan compatible: {error}"))?;

    let info = adapter.get_info();
    if info.backend != wgpu::Backend::Vulkan {
        return Err(format!(
            "El editor requiere Vulkan, pero el adaptador reportó {:?}",
            info.backend
        ));
    }
    log::info!("[editor] GPU: {} — {}", info.backend, info.name);

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("rer-editor-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|error| format!("No se pudo crear el Device del editor: {error}"))?;

    let capabilities = surface.get_capabilities(&adapter);
    // egui-wgpu prefiere un framebuffer no-sRGB (hace su propia corrección de gamma).
    let format = [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ]
    .into_iter()
    .find(|format| capabilities.formats.contains(format))
    .or_else(|| {
        capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
    })
    .unwrap_or(capabilities.formats[0]);

    let size = window.inner_size();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: capabilities.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    Ok(Gpu {
        surface,
        device,
        queue,
        config,
    })
}
