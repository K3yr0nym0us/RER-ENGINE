use super::*;

pub(super) fn push_bytes(joint_base: u32, pass_kind: f32) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&joint_base.to_le_bytes());
    out[4..].copy_from_slice(&pass_kind.to_le_bytes());
    out
}

pub(super) fn set_viewport(device: &ash::Device, cmd: vk::CommandBuffer, extent: vk::Extent2D) {
    set_viewport_rect(
        device,
        cmd,
        vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        },
    );
}

/// Viewport y scissor sobre un sub-rectángulo de la imagen (ventana única del editor).
pub(super) fn set_viewport_rect(device: &ash::Device, cmd: vk::CommandBuffer, rect: vk::Rect2D) {
    let viewport = vk::Viewport {
        x: rect.offset.x as f32,
        y: (rect.offset.y + rect.extent.height as i32) as f32,
        width: rect.extent.width as f32,
        height: -(rect.extent.height as f32),
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = rect;
    unsafe {
        device.cmd_set_viewport(cmd, 0, &[viewport]);
        device.cmd_set_scissor(cmd, 0, &[scissor]);
    }
}

pub(super) fn clear_color(rgba: [f32; 4]) -> vk::ClearValue {
    vk::ClearValue {
        color: vk::ClearColorValue { float32: rgba },
    }
}

/// Borra una imagen de color en `GENERAL` con barreras de memoria (sin cambio de layout).
/// Deja definido el cubemap de probes (6 caras) con el color de cielo.
pub(super) fn clear_probe_cube(gpu: &SharedGpu, image: vk::Image) -> Result<(), VulkanError> {
    gpu.one_shot(move |device, cmd| {
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: crate::engine3d::probe_registry::PROBE_CUBE_ARRAY_LAYERS,
        };
        unsafe {
            device.cmd_clear_color_image(
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                &vk::ClearColorValue {
                    float32: [0.72, 0.86, 0.98, 1.0],
                },
                &[range],
            );
        }
    })
}

pub(super) fn clear_image_color(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    extent: vk::Extent2D,
    rgba: [f32; 4],
) {
    crate::engine3d::vulkan::gpu::cmd_image_barrier(
        device,
        cmd,
        image,
        vk::ImageAspectFlags::COLOR,
        1,
        1,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::AccessFlags::SHADER_WRITE
            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::SHADER_READ,
        vk::AccessFlags::TRANSFER_WRITE,
    );
    let range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };
    unsafe {
        device.cmd_clear_color_image(
            cmd,
            image,
            vk::ImageLayout::GENERAL,
            &vk::ClearColorValue { float32: rgba },
            &[range],
        );
    }
    crate::engine3d::vulkan::gpu::cmd_image_barrier(
        device,
        cmd,
        image,
        vk::ImageAspectFlags::COLOR,
        1,
        1,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::TRANSFER_WRITE,
        vk::AccessFlags::SHADER_READ,
    );
    let _ = extent;
}

pub(super) fn begin_depth(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    extent: vk::Extent2D,
) {
    let depth = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        });
    let info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .depth_attachment(&depth);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

/// Inicia el pase forward sobre el G-buffer multisampleado y resuelve cada adjunto sobre su
/// imagen 1× (la que ya consumen TAA/SSR/present). La profundidad multisampleada se descarta:
/// `targets.depth` se rellena después con un segundo pase a 1×.
pub(super) fn begin_color_ms(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    ms_views: &[vk::ImageView; 5],
    ms_depth: vk::ImageView,
    resolve: &[vk::ImageView; 5],
    extent: vk::Extent2D,
    clear: bool,
) {
    let colors: Vec<vk::RenderingAttachmentInfo> = ms_views
        .iter()
        .zip(resolve)
        .enumerate()
        .map(|(index, (view, resolve_view))| {
            let value = if index == 0 {
                [0.72, 0.86, 0.98, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            vk::RenderingAttachmentInfo::default()
                .image_view(*view)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(if clear || index > 0 {
                    vk::AttachmentLoadOp::CLEAR
                } else {
                    vk::AttachmentLoadOp::LOAD
                })
                .store_op(vk::AttachmentStoreOp::STORE)
                .resolve_image_view(*resolve_view)
                .resolve_image_layout(vk::ImageLayout::GENERAL)
                // `resolveMode != NONE` es lo que hace que el contenido multisampleado llegue a la
                // imagen 1×; con NONE el resolve no ocurre y la imagen 1× quedaría vacía.
                .resolve_mode(vk::ResolveModeFlags::AVERAGE)
                .clear_value(clear_color(value))
        })
        .collect();
    let depth_info = vk::RenderingAttachmentInfo::default()
        .image_view(ms_depth)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        });
    let info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(&colors)
        .depth_attachment(&depth_info);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

pub(super) fn begin_color(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    views: &[vk::ImageView; 5],
    depth: Option<vk::ImageView>,
    extent: vk::Extent2D,
    clear: bool,
) {
    let colors: Vec<vk::RenderingAttachmentInfo> = views
        .iter()
        .enumerate()
        .map(|(index, view)| {
            let value = if index == 0 {
                [0.72, 0.86, 0.98, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            vk::RenderingAttachmentInfo::default()
                .image_view(*view)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(if clear || index > 0 {
                    vk::AttachmentLoadOp::CLEAR
                } else {
                    vk::AttachmentLoadOp::LOAD
                })
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(clear_color(value))
        })
        .collect();
    let depth_info = depth.map(|view| {
        vk::RenderingAttachmentInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            })
    });
    let mut info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(&colors);
    if let Some(depth_info) = depth_info.as_ref() {
        info = info.depth_attachment(depth_info);
    }
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

pub(super) fn begin_swapchain(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    area: vk::Rect2D,
) {
    let color = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_color([0.72, 0.86, 0.98, 1.0]));
    let colors = [color];
    let info = vk::RenderingInfo::default()
        .render_area(area)
        .layer_count(1)
        .color_attachments(&colors);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

pub(super) fn begin_swapchain_load(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    area: vk::Rect2D,
) {
    let color = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::LOAD)
        .store_op(vk::AttachmentStoreOp::STORE);
    let colors = [color];
    let info = vk::RenderingInfo::default()
        .render_area(area)
        .layer_count(1)
        .color_attachments(&colors);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

/// El descriptor y el adjunto declaran un layout; si la imagen no está realmente en ese layout,
/// lo que lee la GPU es indefinido (los bloques negros del cielo) y la validación lo reporta.
/// Esta es la ÚNICA función que cambia layouts de profundidad y deja el layout real registrado en
/// `current`, así no se desincroniza como el booleano "ya fue muestreada" que había antes.
/// Las máscaras son conservadoras porque la misma imagen alterna adjunto, lectura por compute/RT
/// y lectura por fragment; solo se emite barrera cuando el layout cambia de verdad.
pub(super) fn transition_depth(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    current: &mut vk::ImageLayout,
    want: vk::ImageLayout,
) {
    if *current == want {
        return;
    }
    crate::engine3d::vulkan::gpu::cmd_image_barrier(
        device,
        cmd,
        image,
        vk::ImageAspectFlags::DEPTH,
        1,
        1,
        *current,
        want,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
        vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
    );
    *current = want;
}

pub(super) fn copy_color(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    src: &GpuImage,
    dst: &GpuImage,
    extent: vk::Extent2D,
    layer: u32,
) {
    crate::engine3d::vulkan::gpu::cmd_image_barrier(
        device,
        cmd,
        src.image,
        vk::ImageAspectFlags::COLOR,
        src.mips,
        src.layers,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::AccessFlags::SHADER_WRITE,
        vk::AccessFlags::TRANSFER_READ,
    );
    crate::engine3d::vulkan::gpu::cmd_image_barrier(
        device,
        cmd,
        dst.image,
        vk::ImageAspectFlags::COLOR,
        dst.mips,
        dst.layers,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::AccessFlags::SHADER_READ,
        vk::AccessFlags::TRANSFER_WRITE,
    );
    let region = vk::ImageCopy::default()
        .src_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .dst_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: layer,
            layer_count: 1,
        })
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        });
    unsafe {
        device.cmd_copy_image(
            cmd,
            src.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            dst.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        )
    };
    crate::engine3d::vulkan::gpu::cmd_image_barrier(
        device,
        cmd,
        src.image,
        vk::ImageAspectFlags::COLOR,
        src.mips,
        src.layers,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::TRANSFER_READ,
        vk::AccessFlags::SHADER_READ,
    );
    crate::engine3d::vulkan::gpu::cmd_image_barrier(
        device,
        cmd,
        dst.image,
        vk::ImageAspectFlags::COLOR,
        dst.mips,
        dst.layers,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::TRANSFER_WRITE,
        vk::AccessFlags::SHADER_READ,
    );
}

pub(super) fn draw_one(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    vertex: &GpuBuffer,
    index: &GpuBuffer,
    index_count: u32,
    instances: u32,
    instance_buffer: &GpuBuffer,
    instance_offset: u64,
) {
    if index_count == 0 || instances == 0 {
        return;
    }
    unsafe {
        device.cmd_bind_vertex_buffers(
            cmd,
            0,
            &[vertex.buffer, instance_buffer.buffer],
            &[0, instance_offset],
        );
        device.cmd_bind_index_buffer(cmd, index.buffer, 0, vk::IndexType::UINT32);
        device.cmd_draw_indexed(cmd, index_count, instances, 0, 0, 0);
    }
}

pub(super) fn draw_fullscreen(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set: vk::DescriptorSet,
    target: vk::ImageView,
    extent: vk::Extent2D,
    push: &[u8],
) {
    begin_color_one(device, cmd, target, extent);
    set_viewport(device, cmd, extent);
    unsafe {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &[set],
            &[],
        );
        if !push.is_empty() {
            device.cmd_push_constants(
                cmd,
                layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                push,
            );
        }
        device.cmd_draw(cmd, 3, 1, 0, 0);
        device.cmd_end_rendering(cmd);
    }
}

pub(super) fn begin_color_one(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    extent: vk::Extent2D,
) {
    let color = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::GENERAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_color([0.0, 0.0, 0.0, 0.0]));
    let colors = [color];
    let info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(&colors);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

pub(super) fn buffer_barrier(device: &ash::Device, cmd: vk::CommandBuffer, buffer: vk::Buffer) {
    let barrier = vk::BufferMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::UNIFORM_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(buffer)
        .offset(0)
        .size(vk::WHOLE_SIZE);
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::VERTEX_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[barrier],
            &[],
        );
    }
}

pub(super) fn host_buffer(
    gpu: &SharedGpu,
    size: u64,
    name: &str,
) -> Result<GpuBuffer, VulkanError> {
    gpu.create_buffer(
        size,
        vk::BufferUsageFlags::UNIFORM_BUFFER
            | vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::VERTEX_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST,
        MemoryLocation::CpuToGpu,
        name,
    )
}
