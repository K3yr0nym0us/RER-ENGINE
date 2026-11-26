/// Backend usado por winit en Linux. El proyecto fuerza X11 desde Electron, pero
/// el binario también puede ejecutarse directamente bajo Wayland.
#[cfg(target_os = "linux")]
static X11_BACKEND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[cfg(target_os = "linux")]
pub(crate) fn is_x11_backend() -> bool {
    *X11_BACKEND.get_or_init(|| {
        let has_wayland = ["WAYLAND_DISPLAY", "WAYLAND_SOCKET"]
            .iter()
            .any(|name| std::env::var(name).is_ok_and(|value| !value.is_empty()));
        let has_x11 = std::env::var("DISPLAY").is_ok_and(|value| !value.is_empty());
        has_x11 && !has_wayland
    })
}

/// Distancia máxima aplicada a la cámara desde un solo evento de ratón. Evita
/// que un teletransporte del puntero o un salto de foco dispare un giro brusco.
#[cfg(target_os = "linux")]
const X11_MAX_MOTION_DELTA: f32 = 256.0;
/// Por encima de este valor, un `MouseMotion` positivo casi siempre es una
/// posición absoluta de un puntero virtual/RDP, no un delta físico.
#[cfg(target_os = "linux")]
const X11_ABSOLUTE_MOTION_MIN: f32 = 512.0;

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum X11MouseMotionMode {
    #[default]
    Unknown,
    Relative,
    Absolute,
}

/// Normaliza `DeviceEvent::MouseMotion` en X11.
///
/// Hay dos problemas conocidos de winit 0.30:
/// - con `Confined`, cada evento físico se entrega dos veces (winit#3773);
/// - punteros absolutos de VMs/RDP/WSLg se entregan como si fueran deltas
///   (winit#946), con valores de miles de píxeles que hacen girar la cámara.
///
/// Para el segundo caso conservamos la última posición absoluta y calculamos
/// su diferencia. Los eventos duplicados entonces producen delta cero.
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub(crate) struct X11MouseMotionFilter {
    mode: X11MouseMotionMode,
    last_absolute: Option<(f32, f32)>,
}

#[cfg(target_os = "linux")]
impl X11MouseMotionFilter {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn filter(&mut self, delta_x: f32, delta_y: f32) -> Option<(f32, f32)> {
        if !delta_x.is_finite() || !delta_y.is_finite() {
            return None;
        }

        if self.mode == X11MouseMotionMode::Unknown {
            if looks_like_absolute_motion(delta_x, delta_y) {
                self.mode = X11MouseMotionMode::Absolute;
                self.last_absolute = Some((delta_x, delta_y));
                return None;
            }
            self.mode = X11MouseMotionMode::Relative;
            return Some(clamp_motion_delta(delta_x * 0.5, delta_y * 0.5));
        }

        if self.mode == X11MouseMotionMode::Relative && looks_like_absolute_motion(delta_x, delta_y)
        {
            self.mode = X11MouseMotionMode::Absolute;
            self.last_absolute = Some((delta_x, delta_y));
            return None;
        }

        match self.mode {
            X11MouseMotionMode::Relative => Some(clamp_motion_delta(delta_x * 0.5, delta_y * 0.5)),
            X11MouseMotionMode::Absolute => {
                let (last_x, last_y) = self.last_absolute?;
                self.last_absolute = Some((delta_x, delta_y));
                let motion = (delta_x - last_x, delta_y - last_y);
                (motion.0 != 0.0 || motion.1 != 0.0).then(|| clamp_motion_delta(motion.0, motion.1))
            }
            X11MouseMotionMode::Unknown => None,
        }
    }
}

#[cfg(target_os = "linux")]
fn looks_like_absolute_motion(x: f32, y: f32) -> bool {
    x >= 0.0 && y >= 0.0 && (x > X11_ABSOLUTE_MOTION_MIN || y > X11_ABSOLUTE_MOTION_MIN)
}

#[cfg(target_os = "linux")]
fn clamp_motion_delta(x: f32, y: f32) -> (f32, f32) {
    let length = (x * x + y * y).sqrt();
    if length <= X11_MAX_MOTION_DELTA {
        return (x, y);
    }
    let scale = X11_MAX_MOTION_DELTA / length;
    (x * scale, y * scale)
}

/// Estado de una tecla virtual consultado mediante el sistema operativo.
#[derive(Clone, Copy, Debug, Default)]
pub struct VkOsProbe {
    pub async_down: bool,
    /// Bit de conmutación reportado por la última consulta de esa tecla.
    pub async_toggle: bool,
    pub kbd_down: bool,
}

#[cfg(target_os = "windows")]
pub(crate) fn probe_vk_os(vk: u8) -> VkOsProbe {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyboardState};
        let async_val = GetAsyncKeyState(i32::from(vk)) as u16;
        let mut kbd = [0u8; 256];
        let kbd_down = if GetKeyboardState(&mut kbd).is_ok() {
            kbd[vk as usize] & 0x80 != 0
        } else {
            false
        };
        VkOsProbe {
            async_down: async_val & 0x8000 != 0,
            async_toggle: async_val & 0x0001 != 0,
            kbd_down,
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn probe_vk_os(_vk: u8) -> VkOsProbe {
    VkOsProbe::default()
}

/// Q física (rotación izquierda del ghost muro/trigger). Lectura global vía OS.
#[cfg(target_os = "windows")]
pub(crate) fn query_key_q_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        (GetAsyncKeyState(0x51) as u16 & 0x8000) != 0
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn query_key_q_held_os() -> bool {
    false
}

/// E física (rotación derecha del ghost muro/trigger). Lectura global vía OS.
#[cfg(target_os = "windows")]
pub(crate) fn query_key_e_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        (GetAsyncKeyState(0x45) as u16 & 0x8000) != 0
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn query_key_e_held_os() -> bool {
    false
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{X11MouseMotionFilter, clamp_motion_delta, looks_like_absolute_motion};

    #[test]
    fn reconoce_posiciones_absolutas_por_tamano() {
        assert!(looks_like_absolute_motion(2237.0, 454.0));
        assert!(!looks_like_absolute_motion(3.0, -2.0));
        assert!(!looks_like_absolute_motion(-2237.0, 454.0));
    }

    #[test]
    fn convierte_posiciones_absolutas_en_deltas_e_ignora_duplicados() {
        let mut filter = X11MouseMotionFilter::default();
        assert_eq!(filter.filter(2237.0, 454.0), None);
        assert_eq!(filter.filter(2237.0, 454.0), None);
        assert_eq!(filter.filter(2236.0, 454.0), Some((-1.0, 0.0)));
        assert_eq!(filter.filter(2236.0, 454.0), None);
        assert_eq!(filter.filter(2236.0, 453.0), Some((0.0, -1.0)));
    }

    #[test]
    fn compensa_eventos_relativos_duplicados_en_x11() {
        let mut filter = X11MouseMotionFilter::default();
        assert_eq!(filter.filter(10.0, 4.0), Some((5.0, 2.0)));
        assert_eq!(filter.filter(10.0, 4.0), Some((5.0, 2.0)));
    }

    #[test]
    fn cambia_a_absoluto_si_el_puntero_virtual_aparece_despues() {
        let mut filter = X11MouseMotionFilter::default();
        assert_eq!(filter.filter(2.0, 1.0), Some((1.0, 0.5)));
        assert_eq!(filter.filter(2237.0, 454.0), None);
        assert_eq!(filter.filter(2236.0, 454.0), Some((-1.0, 0.0)));
    }

    #[test]
    fn limita_saltos_para_no_disparar_la_camara() {
        let (x, y) = clamp_motion_delta(3000.0, 4000.0);
        assert!((x * x + y * y).sqrt() <= 256.0);
        assert!((x / 3000.0 - y / 4000.0).abs() < 1e-5);
    }
}
