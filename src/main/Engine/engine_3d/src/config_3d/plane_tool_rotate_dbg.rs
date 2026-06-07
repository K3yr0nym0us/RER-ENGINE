//! Diagnóstico Q/E del ghost muro/trigger. Filtrar consola con `[plane_rot]`.
//! Rotación detectada solo en el motor (polling OS); estos logs no alteran la lógica.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use winit::keyboard::{KeyCode, PhysicalKey};

use crate::platform::{probe_vk_os, VkOsProbe};

const HOLD_LOG_INTERVAL: Duration = Duration::from_millis(400);

struct DbgState {
    last_hold_log: Instant,
    prev_final_left: bool,
    prev_final_right: bool,
    prev_os_q: bool,
    prev_os_e: bool,
}

fn dbg_state() -> &'static Mutex<DbgState> {
    static STATE: OnceLock<Mutex<DbgState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(DbgState {
            last_hold_log: Instant::now() - HOLD_LOG_INTERVAL,
            prev_final_left: false,
            prev_final_right: false,
            prev_os_q: false,
            prev_os_e: false,
        })
    })
}

fn rotate_key_label(key: PhysicalKey, text: Option<&str>) -> Option<&'static str> {
    match key {
        PhysicalKey::Code(KeyCode::KeyQ) => Some("KeyQ"),
        PhysicalKey::Code(KeyCode::KeyE) => Some("KeyE"),
        PhysicalKey::Code(KeyCode::ArrowLeft) => Some("ArrowLeft"),
        PhysicalKey::Code(KeyCode::ArrowRight) => Some("ArrowRight"),
        _ => text.and_then(|t| {
            if t.eq_ignore_ascii_case("q") {
                Some("text:q")
            } else if t.eq_ignore_ascii_case("e") {
                Some("text:e")
            } else {
                None
            }
        }),
    }
}

pub(crate) fn is_rotate_related_winit_key(key: PhysicalKey, text: Option<&str>) -> bool {
    rotate_key_label(key, text).is_some()
}

pub(crate) fn log_winit_key(key: PhysicalKey, text: Option<&str>, pressed: bool, repeat: bool) {
    let Some(label) = rotate_key_label(key, text) else {
        return;
    };
    let q = probe_vk_os(0x51);
    let e = probe_vk_os(0x45);
    log::info!(
        "[plane_rot] WINIT label={label} pressed={pressed} repeat={repeat} text={:?} \
         → SWALLOW (player_ui no recibe Q/E) | OS probe Q={} E={}",
        text,
        fmt_vk(&q),
        fmt_vk(&e),
    );
}

pub(crate) fn log_focus(focused: bool) {
    log::info!(
        "[plane_rot] FOCUS engine_window_focused={focused} \
         (la rotación usa polling OS global; no depende del foco winit)",
    );
}

pub(crate) fn log_clear(reason: &str) {
    log::info!("[plane_rot] CLEAR estado interno ← {reason}");
}

fn fmt_vk(p: &VkOsProbe) -> String {
    format!(
        "async↓={} async∆={} kbd↓={}",
        p.async_down, p.async_toggle, p.kbd_down
    )
}

/// Registra cada frame de rotación cuando hay entrada o cambia el estado OS.
pub(crate) fn log_apply_rotation(
    engine_window_focused: bool,
    os_q: bool,
    os_e: bool,
    final_left: bool,
    final_right: bool,
    degrees: f32,
) {
    let mut st = dbg_state().lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let active = final_left || final_right;
    let os_changed = os_q != st.prev_os_q || os_e != st.prev_os_e;
    let output_changed = final_left != st.prev_final_left || final_right != st.prev_final_right;
    let throttle_ok = now.duration_since(st.last_hold_log) >= HOLD_LOG_INTERVAL;

    if !active && !os_changed && !output_changed {
        return;
    }
    if active && !output_changed && !os_changed && !throttle_ok {
        return;
    }

    st.prev_os_q = os_q;
    st.prev_os_e = os_e;
    st.prev_final_left = final_left;
    st.prev_final_right = final_right;
    if active {
        st.last_hold_log = now;
    }

    let q = probe_vk_os(0x51);
    let e = probe_vk_os(0x45);

    let src_left = if final_left { "OS-Q" } else { "—" };
    let src_right = if final_right { "OS-E" } else { "—" };

    log::info!(
        "[plane_rot] APPLY motor-only OS os_Q={os_q} os_E={os_e} \
         → final_L={final_left} final_R={final_right} src_L={src_left} src_R={src_right} \
         deg={degrees:.3} winit_focused={engine_window_focused} | probe Q={} E={}",
        fmt_vk(&q),
        fmt_vk(&e),
    );
}
