//! Cuadros de texto HUD (pantalla NDC) durante edición de UI del jugador.

rer_engine_shared::impl_player_ui_text_input! {
    crate::config_2d::player_ui,
    crate::ipc
}

// El 2D sí dibuja el HUD de texto con wgpu, así que añade el método de dibujado aparte (el 3D lo
// hace en su FrameRenderer de Vulkan y no lo instancia).
rer_engine_shared::impl_player_ui_text_input_gpu_draw! { crate::engine::State }
