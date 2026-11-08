//! Vulkan crudo (Ash) del motor 3D. El motor 2D sigue en wgpu.
//!
//! El `unsafe` de este módulo es FFI de Vulkan: ash expone handles cuyo orden de
//! destrucción lo marca la especificación (buffer, imagen, swapchain, device, surface,
//! instance) y esos tipos no implementan `Drop`.

mod accel;
mod error;
mod frame;
mod gpu;
mod select;

pub use frame::{FrameInput, FrameRenderer, GizmoBatch, MeshBatch, RtBatch, SkinnedBatch};

pub use error::VulkanError;
pub use gpu::{FrameAcquire, GpuBuffer, GpuImage, SharedGpu};
