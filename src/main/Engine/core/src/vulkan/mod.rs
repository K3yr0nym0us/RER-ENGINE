//! Núcleo Vulkan crudo (Ash) compartido entre los motores.
//!
//! Aquí vive lo que no depende de la escena: instancia/adapter/device, superficie y swapchain,
//! `GpuBuffer`/`GpuImage` con su memoria (`gpu-allocator`), barreras, subidas por staging y la
//! selección de formato y present mode. Lo específico de cada motor (RT/BLAS/TLAS, G-buffer,
//! sondas, MSAA, HUD) sigue en su crate.
//!
//! El `unsafe` de este módulo es FFI de Vulkan: `ash` no libera `Instance` ni `Device` en `Drop`
//! y el driver ata la vida de cada objeto al padre (buffer → device → instance → entry), así que
//! el orden de destrucción lo marca la especificación y no hay wrapper safe que exprese ese grafo.

pub mod error;
pub mod gpu;
pub mod select;

pub use error::VulkanError;
pub use gpu::{
    FRAMES_IN_FLIGHT, FrameAcquire, FrameTarget, GpuBuffer, GpuImage, MemoryLocation, RtProperties,
    SharedGpu, VulkanOptions,
};
