//! Superficie y swapchain extra sobre el mismo `SharedGpu`.
//!
//! El editor usa esto para ventanas nativas secundarias (modales): no se crea
//! otro device. El present no usa FIFO ni `device_wait_idle`: esa combinación
//! congelaba el motor al minimizar o desenfocar la modal. El `Drop` destruye
//! solo esta superficie.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use super::super::error::{VulkanError, vk_error};
use super::frame::{FrameSync, create_frames};
use super::swapchain::{SwapchainState, create_swapchain};
use super::{FRAMES_IN_FLIGHT, FrameAcquire, FrameTarget, SharedGpu};

/// Tiempo máximo para el fence / `acquire` de una ventana extra. `u64::MAX` bloquea el
/// event loop entero (editor + motor) si DWM no completa el present de una surface
/// minimizada u ocluida.
const AUX_WAIT_TIMEOUT_NS: u64 = 8_000_000;

pub struct PresentTarget {
    gpu: SharedGpu,
    surface_loader: ash::khr::surface::Instance,
    swapchain_loader: ash::khr::swapchain::Device,
    surface: vk::SurfaceKHR,
    swapchain: Mutex<SwapchainState>,
    frames: Mutex<[FrameSync; FRAMES_IN_FLIGHT]>,
    command_pool: vk::CommandPool,
    frame_slot: AtomicUsize,
}

impl PresentTarget {
    pub fn new(gpu: SharedGpu, window: &Window) -> Result<Self, VulkanError> {
        let display = window
            .display_handle()
            .map_err(|error| VulkanError::new(format!("Ventana sin display handle: {error}")))?
            .as_raw();
        let window_handle = window
            .window_handle()
            .map_err(|error| VulkanError::new(format!("Ventana sin window handle: {error}")))?
            .as_raw();
        let surface = unsafe {
            ash_window::create_surface(gpu.entry(), gpu.instance(), display, window_handle, None)
                .map_err(|error| vk_error("crear la superficie secundaria", error))?
        };
        let surface_loader = ash::khr::surface::Instance::new(gpu.entry(), gpu.instance());
        let swapchain_loader = ash::khr::swapchain::Device::new(gpu.instance(), gpu.device());

        let supported = unsafe {
            surface_loader.get_physical_device_surface_support(
                gpu.physical_device(),
                gpu.queue_family_index(),
                surface,
            )
        }
        .map_err(|error| vk_error("consultar presentación en la ventana secundaria", error))?;
        if !supported {
            unsafe {
                surface_loader.destroy_surface(surface, None);
            }
            return Err(VulkanError::new(
                "La cola del editor no puede presentar en esta ventana.".to_string(),
            ));
        }

        let size = window.inner_size();
        let swapchain = create_swapchain(
            gpu.device(),
            &surface_loader,
            &swapchain_loader,
            gpu.physical_device(),
            surface,
            size.width.max(1),
            size.height.max(1),
            // MAILBOX/IMMEDIATE: un FIFO extra en la misma cola que el editor se
            // queda esperando el vblank de una surface en segundo plano y congela el motor.
            false,
            vk::SwapchainKHR::null(),
        )?;
        let command_pool = unsafe {
            gpu.device().create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(gpu.queue_family_index())
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .map_err(|error| vk_error("crear el command pool de la ventana secundaria", error))?;
        let frames = unsafe { create_frames(gpu.device(), command_pool)? };

        Ok(Self {
            gpu,
            surface_loader,
            swapchain_loader,
            surface,
            swapchain: Mutex::new(swapchain),
            frames: Mutex::new(frames),
            command_pool,
            frame_slot: AtomicUsize::new(0),
        })
    }

    pub fn swapchain_format(&self) -> vk::Format {
        self.swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .format
    }

    pub fn command_pool(&self) -> vk::CommandPool {
        self.command_pool
    }

    pub fn gpu(&self) -> &SharedGpu {
        &self.gpu
    }

    pub fn resize(&self, width: u32, height: u32) -> Result<(), VulkanError> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        unsafe {
            self.gpu.device().device_wait_idle().map_err(|error| {
                vk_error("esperar la GPU antes de recrear el swapchain extra", error)
            })?;
        }
        let mut swapchain = self
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let old = swapchain.handle;
        let created = create_swapchain(
            self.gpu.device(),
            &self.surface_loader,
            &self.swapchain_loader,
            self.gpu.physical_device(),
            self.surface,
            width.max(1),
            height.max(1),
            swapchain.vsync,
            old,
        )?;
        unsafe {
            for view in swapchain.views.drain(..) {
                self.gpu.device().destroy_image_view(view, None);
            }
            for semaphore in swapchain.render_finished.drain(..) {
                self.gpu.device().destroy_semaphore(semaphore, None);
            }
            self.swapchain_loader.destroy_swapchain(old, None);
        }
        *swapchain = created;
        Ok(())
    }

    pub fn begin_frame(&self) -> Result<FrameTarget, FrameAcquire> {
        let slot = self.frame_slot.load(Ordering::Relaxed) % FRAMES_IN_FLIGHT;
        let mut frames = self
            .frames
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let frame = &mut frames[slot];
        unsafe {
            match self
                .gpu
                .device()
                .wait_for_fences(&[frame.fence], true, AUX_WAIT_TIMEOUT_NS)
            {
                Ok(()) => {}
                Err(vk::Result::TIMEOUT) => return Err(FrameAcquire::Skip),
                Err(error) => {
                    return Err(FrameAcquire::Fail(vk_error(
                        "esperar el frame anterior de la ventana extra",
                        error,
                    )));
                }
            }
        }
        let swapchain = self
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if swapchain.extent.width == 0 || swapchain.extent.height == 0 {
            return Err(FrameAcquire::Skip);
        }
        let acquired = unsafe {
            self.swapchain_loader.acquire_next_image(
                swapchain.handle,
                AUX_WAIT_TIMEOUT_NS,
                frame.acquire,
                vk::Fence::null(),
            )
        };
        let image_index = match acquired {
            Ok((index, _suboptimal)) => index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Err(FrameAcquire::Outdated),
            Err(vk::Result::NOT_READY) | Err(vk::Result::TIMEOUT) => {
                return Err(FrameAcquire::Skip);
            }
            Err(error) => {
                return Err(FrameAcquire::Fail(vk_error(
                    "adquirir la imagen del swapchain extra",
                    error,
                )));
            }
        };
        unsafe {
            self.gpu
                .device()
                .reset_fences(&[frame.fence])
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("reiniciar el fence de la ventana extra", error))
                })?;
            self.gpu
                .device()
                .reset_command_buffer(frame.command, vk::CommandBufferResetFlags::empty())
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("reiniciar el command buffer extra", error))
                })?;
            self.gpu
                .device()
                .begin_command_buffer(frame.command, &vk::CommandBufferBeginInfo::default())
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("abrir el command buffer extra", error))
                })?;
        }
        self.frame_slot.fetch_add(1, Ordering::Relaxed);
        Ok(FrameTarget {
            slot,
            image_index,
            command: frame.command,
            color_view: swapchain.views[image_index as usize],
            color_image: swapchain.images[image_index as usize],
            extent: swapchain.extent,
            offset: vk::Offset2D::default(),
            format: swapchain.format,
        })
    }

    pub fn submit_and_present(&self, frame: &FrameTarget) -> Result<(), FrameAcquire> {
        unsafe {
            self.gpu
                .device()
                .end_command_buffer(frame.command)
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("cerrar el command buffer extra", error))
                })?;
        }
        let frames = self
            .frames
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let sync = &frames[frame.slot];
        let swapchain = self
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let wait_semaphores = [sync.acquire];
        let signal_semaphores = [swapchain.render_finished[frame.image_index as usize]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = [frame.command];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        let queue = self.gpu.lock_queue();
        unsafe {
            self.gpu
                .device()
                .queue_submit(*queue, &[submit], sync.fence)
                .map_err(|error| FrameAcquire::Fail(vk_error("enviar el frame extra", error)))?;
        }
        let indices = [frame.image_index];
        let swapchains = [swapchain.handle];
        let present = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&indices);
        let presented = unsafe { self.swapchain_loader.queue_present(*queue, &present) };
        drop(queue);
        match presented {
            Ok(false) => Ok(()),
            Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Err(FrameAcquire::Outdated),
            Err(vk::Result::ERROR_SURFACE_LOST_KHR) => Err(FrameAcquire::Outdated),
            Err(error) => Err(FrameAcquire::Fail(vk_error(
                "presentar el frame extra",
                error,
            ))),
        }
    }
}

impl Drop for PresentTarget {
    fn drop(&mut self) {
        unsafe {
            let _ = self.gpu.device().device_wait_idle();
            let frames = self
                .frames
                .get_mut()
                .unwrap_or_else(|poison| poison.into_inner());
            for frame in frames.iter() {
                self.gpu.device().destroy_semaphore(frame.acquire, None);
                self.gpu.device().destroy_fence(frame.fence, None);
            }
            self.gpu
                .device()
                .destroy_command_pool(self.command_pool, None);
            let swapchain = self
                .swapchain
                .get_mut()
                .unwrap_or_else(|poison| poison.into_inner());
            for view in swapchain.views.drain(..) {
                self.gpu.device().destroy_image_view(view, None);
            }
            for semaphore in swapchain.render_finished.drain(..) {
                self.gpu.device().destroy_semaphore(semaphore, None);
            }
            self.swapchain_loader
                .destroy_swapchain(swapchain.handle, None);
            self.surface_loader.destroy_surface(self.surface, None);
        }
    }
}
