//! Vulkan crudo (Ash) del motor 3D.
//!
//! El núcleo (contexto, swapchain, buffers, imágenes, selección de formato) ya no está aquí: vive
//! en `rer_engine_core::vulkan` y se comparte con el motor 2D. Estos alias de módulo mantienen
//! vivas las rutas `super::gpu::…`, `super::error::…` y `super::select::…` que usan `frame.rs` y
//! `accel.rs`, así que la extracción no obligó a reescribir el renderer.
//!
//! Lo que sí se queda en el crate: `frame.rs` (raster + G-buffer + MSAA + TAA + SSR + sondas +
//! sombras) y `accel.rs` (BLAS/TLAS), que son específicos de la escena 3D.
//!
//! El `unsafe` de este módulo es FFI de Vulkan: ash expone handles cuyo orden de destrucción lo
//! marca la especificación (buffer → device → instance → entry) y esos tipos no implementan
//! `Drop`.

mod accel;
mod frame;

pub(crate) mod error {
    pub use rer_engine_core::vulkan::error::*;
}

pub(crate) mod gpu {
    pub use rer_engine_core::vulkan::gpu::*;
}

pub(crate) mod select {
    pub use rer_engine_core::vulkan::select::*;
}

pub use frame::{
    FrameInput, FrameRenderer, GizmoBatch, MeshBatch, ProbeCapture, RtBatch, SkinnedBatch,
};

pub use rer_engine_core::vulkan::{
    FrameAcquire, FrameTarget, GpuBuffer, GpuImage, SharedGpu, VulkanError, VulkanOptions,
};
