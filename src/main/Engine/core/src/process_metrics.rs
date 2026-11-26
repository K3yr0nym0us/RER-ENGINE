// CPU/GPU del proceso del motor (no uso global del SO).
// Windows: contadores GPU Engine por PID.
// `sysinfo` devuelve 100% por núcleo en Windows y Linux; se normaliza al total
// de CPUs lógicos para que el panel tenga el mismo significado en ambos SO.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
#[cfg(target_os = "windows")]
use std::thread;
use std::thread::JoinHandle;
#[cfg(target_os = "windows")]
use std::time::Duration;

use sysinfo::{Pid, ProcessesToUpdate, System};

/// Intervalo entre lecturas. La consulta PDH solo toca los contadores de este PID.
#[cfg(target_os = "windows")]
const GPU_SAMPLE_INTERVAL: Duration = Duration::from_secs(3);
/// Cada cuánto se redescubren las instancias GPU del proceso (aparecen al renderizar).
#[cfg(target_os = "windows")]
const GPU_COUNTER_REFRESH: Duration = Duration::from_secs(30);
/// Valor centinela en `AtomicU32` (= sin dato GPU).
const GPU_RAW_NONE: u32 = u32::MAX;

/// Muestrea CPU/GPU del proceso actual (motor) sin bloquear el bucle de render.
pub struct ProcessMetricsSampler {
    system: System,
    gpu_raw: Arc<AtomicU32>,
    #[allow(dead_code)]
    gpu_stop: Arc<AtomicBool>,
    gpu_thread: Option<JoinHandle<()>>,
}

impl ProcessMetricsSampler {
    pub fn new() -> Self {
        let gpu_raw = Arc::new(AtomicU32::new(GPU_RAW_NONE));
        let gpu_stop = Arc::new(AtomicBool::new(false));
        let gpu_thread =
            spawn_gpu_poll_thread(std::process::id(), gpu_raw.clone(), gpu_stop.clone());
        Self {
            system: System::new(),
            gpu_raw,
            gpu_stop,
            gpu_thread,
        }
    }

    pub fn sample_cpu_percent(&mut self) -> f32 {
        let pid = Pid::from_u32(std::process::id());
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        let raw = self
            .system
            .process(pid)
            .map(|p| p.cpu_usage())
            .unwrap_or(0.0);
        // `refresh_processes` también carga la lista de CPUs usada por sysinfo para
        // calcular el porcentaje; usar su tamaño mantiene exactamente la misma escala.
        normalize_cpu_percent(raw, self.system.cpus().len())
    }

    /// % GPU del proceso del motor (contadores por PID; no uso global del sistema).
    pub fn sample_gpu_percent(&self) -> Option<f32> {
        raw_to_gpu_percent(self.gpu_raw.load(Ordering::Relaxed))
    }
}

impl Default for ProcessMetricsSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProcessMetricsSampler {
    fn drop(&mut self) {
        self.gpu_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.gpu_thread.take() {
            let _ = handle.join();
        }
    }
}

fn raw_to_gpu_percent(raw: u32) -> Option<f32> {
    if raw == GPU_RAW_NONE {
        return None;
    }
    Some((raw as f32) / 10.0)
}

/// Convierte el porcentaje por núcleo de `sysinfo` a porcentaje del total de CPUs.
fn normalize_cpu_percent(raw: f32, logical_cpus: usize) -> f32 {
    if !raw.is_finite() || raw <= 0.0 {
        return 0.0;
    }
    (raw / logical_cpus.max(1) as f32).clamp(0.0, 100.0)
}

#[cfg(target_os = "windows")]
fn store_gpu_percent(raw: &AtomicU32, percent: Option<f32>) {
    match percent {
        Some(v) if v.is_finite() && v >= 0.0 => {
            raw.store(((v.min(100.0)) * 10.0).round() as u32, Ordering::Relaxed);
        }
        _ => raw.store(GPU_RAW_NONE, Ordering::Relaxed),
    }
}

#[cfg(target_os = "windows")]
fn spawn_gpu_poll_thread(
    pid: u32,
    raw: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
) -> Option<JoinHandle<()>> {
    if !gpu_sampling_available() {
        return None;
    }
    Some(thread::spawn(move || {
        let mut counters = gpu_counters::PidGpuCounters::default();
        let mut refreshed_at = std::time::Instant::now() - GPU_COUNTER_REFRESH;
        while !stop.load(Ordering::Relaxed) {
            let now = std::time::Instant::now();
            if counters.is_empty() || now.duration_since(refreshed_at) >= GPU_COUNTER_REFRESH {
                counters.rebuild(pid);
                refreshed_at = std::time::Instant::now();
                counters.collect();
            }
            if !wait_or_stop(&stop, Duration::from_secs(1)) {
                break;
            }
            store_gpu_percent(&raw, counters.read_percent());
            let rest = GPU_SAMPLE_INTERVAL.saturating_sub(Duration::from_secs(1));
            if !wait_or_stop(&stop, rest) {
                break;
            }
        }
        counters.close();
    }))
}

/// Sin PDH no hay contadores por PID: no se crea ningún hilo GPU en Linux.
#[cfg(not(target_os = "windows"))]
fn spawn_gpu_poll_thread(
    _pid: u32,
    _raw: Arc<AtomicU32>,
    _stop: Arc<AtomicBool>,
) -> Option<JoinHandle<()>> {
    None
}

/// Duerme a trozos para que `Drop` no espere el intervalo entero.
#[cfg(target_os = "windows")]
fn wait_or_stop(stop: &AtomicBool, total: Duration) -> bool {
    let step = Duration::from_millis(200);
    let mut left = total;
    while !left.is_zero() {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let slice = left.min(step);
        thread::sleep(slice);
        left = left.saturating_sub(slice);
    }
    !stop.load(Ordering::Relaxed)
}

/// Indica si el motor puede intentar muestrear % GPU en esta plataforma.
pub fn is_engine_gpu_metrics_available() -> bool {
    gpu_sampling_available()
}

#[cfg(target_os = "windows")]
fn gpu_sampling_available() -> bool {
    true
}

#[cfg(not(target_os = "windows"))]
fn gpu_sampling_available() -> bool {
    false
}

/// Contadores PDH del proceso. No usa `Get-Counter` ni el comodín de todos los motores GPU:
/// ese comodín, relanzado en PowerShell, enumeraba el sistema entero y apilaba procesos.
#[cfg(target_os = "windows")]
mod gpu_counters {
    use windows::Win32::System::Performance::{
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
        PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
        PdhCollectQueryData, PdhExpandWildCardPathW, PdhGetFormattedCounterValue, PdhOpenQueryW,
    };
    use windows::core::{PCWSTR, PWSTR};

    /// Tope para no reservar la lista completa de motores GPU del sistema si PDH se desborda.
    const MAX_EXPAND_CHARS: u32 = 1 << 20;
    const GPU_ENGINE_WILDCARD: &str = r"\GPU Engine(*)\Utilization Percentage";

    #[derive(Default)]
    pub struct PidGpuCounters {
        query: PDH_HQUERY,
        counters: Vec<PDH_HCOUNTER>,
    }

    impl PidGpuCounters {
        pub fn is_empty(&self) -> bool {
            self.counters.is_empty()
        }

        pub fn close(&mut self) {
            if !self.query.is_invalid() {
                // Seguridad: el query lo abrió este hilo con PdhOpenQueryW y no se comparte.
                unsafe {
                    let _ = PdhCloseQuery(self.query);
                }
                self.query = PDH_HQUERY::default();
            }
            self.counters.clear();
        }

        pub fn rebuild(&mut self, pid: u32) {
            self.close();
            let Some(paths) = expand_pid_paths(pid) else {
                return;
            };
            let mut query = PDH_HQUERY::default();
            // Seguridad: PDH escribe el handle en `query`; el puntero es de esta pila.
            let opened = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
            if opened != 0 || query.is_invalid() {
                return;
            }
            let mut counters = Vec::with_capacity(paths.len());
            for path in &paths {
                let wide = wide_null(path);
                let mut counter = PDH_HCOUNTER::default();
                // Seguridad: `wide` vive hasta el retorno y PDH copia la ruta al añadirla.
                let added =
                    unsafe { PdhAddEnglishCounterW(query, PCWSTR(wide.as_ptr()), 0, &mut counter) };
                if added == 0 && !counter.is_invalid() {
                    counters.push(counter);
                }
            }
            if counters.is_empty() {
                unsafe {
                    let _ = PdhCloseQuery(query);
                }
                return;
            }
            self.query = query;
            self.counters = counters;
        }

        pub fn collect(&self) {
            if self.query.is_invalid() {
                return;
            }
            unsafe {
                let _ = PdhCollectQueryData(self.query);
            }
        }

        pub fn read_percent(&self) -> Option<f32> {
            if self.query.is_invalid() || self.counters.is_empty() {
                return None;
            }
            // Seguridad: el query sigue abierto y los contadores pertenecen a este hilo.
            let collected = unsafe { PdhCollectQueryData(self.query) };
            if collected != 0 {
                return None;
            }
            let mut sum = 0.0f64;
            let mut any = false;
            for counter in &self.counters {
                let mut value = PDH_FMT_COUNTERVALUE::default();
                let status = unsafe {
                    PdhGetFormattedCounterValue(*counter, PDH_FMT_DOUBLE, None, &mut value)
                };
                if status != 0 {
                    continue;
                }
                if value.CStatus != PDH_CSTATUS_VALID_DATA && value.CStatus != PDH_CSTATUS_NEW_DATA
                {
                    continue;
                }
                let sample = unsafe { value.Anonymous.doubleValue };
                if sample.is_finite() && sample >= 0.0 {
                    sum += sample;
                    any = true;
                }
            }
            if !any {
                return None;
            }
            Some((sum as f32).min(100.0))
        }
    }

    fn expand_pid_paths(pid: u32) -> Option<Vec<String>> {
        let needle = format!("pid_{pid}_");
        let paths = expand_wildcard()?;
        let matched: Vec<String> = paths
            .into_iter()
            .filter(|path| path_matches_pid(path, &needle))
            .collect();
        if matched.is_empty() {
            None
        } else {
            Some(matched)
        }
    }

    fn expand_wildcard() -> Option<Vec<String>> {
        let pattern = wide_null(GPU_ENGINE_WILDCARD);
        let mut len = 0u32;
        // Seguridad: PDH solo escribe la longitud pedida; el buffer llega en la segunda llamada.
        let status = unsafe {
            PdhExpandWildCardPathW(PCWSTR::null(), PCWSTR(pattern.as_ptr()), None, &mut len, 0)
        };
        if (status != PDH_MORE_DATA && status != 0) || len == 0 || len > MAX_EXPAND_CHARS {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        let status = unsafe {
            PdhExpandWildCardPathW(
                PCWSTR::null(),
                PCWSTR(pattern.as_ptr()),
                Some(PWSTR(buf.as_mut_ptr())),
                &mut len,
                0,
            )
        };
        if status != 0 {
            return None;
        }
        let paths = multi_sz_strings(&buf);
        if paths.is_empty() { None } else { Some(paths) }
    }

    fn wide_null(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn multi_sz_strings(buf: &[u16]) -> Vec<String> {
        buf.split(|unit| *unit == 0)
            .take_while(|part| !part.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }

    pub fn path_matches_pid(path: &str, needle: &str) -> bool {
        path.contains(needle)
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::gpu_counters::{multi_sz_strings, path_matches_pid};

    #[test]
    fn multi_sz_se_detiene_en_el_doble_nulo() {
        let buf = [b'a' as u16, 0, b'b' as u16, 0, 0, b'c' as u16];
        assert_eq!(
            multi_sz_strings(&buf),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn el_filtro_de_pid_no_mezcla_otro_proceso() {
        let path = r"\GPU Engine(pid_42_luid_0x1_phys_0_eng_0_engtype_3D)\Utilization Percentage";
        assert!(path_matches_pid(path, "pid_42_"));
        assert!(!path_matches_pid(path, "pid_4_"));
        assert!(!path_matches_pid(path, "pid_420_"));
    }
}

#[cfg(all(test, not(target_os = "windows")))]
mod linux_tests {
    use super::{ProcessMetricsSampler, is_engine_gpu_metrics_available};

    #[test]
    fn sin_pdh_no_hilo_gpu_y_el_panel_se_oculta() {
        assert!(!is_engine_gpu_metrics_available());
        let sampler = ProcessMetricsSampler::new();
        assert!(sampler.gpu_thread.is_none());
        assert!(sampler.sample_gpu_percent().is_none());
    }
}

#[cfg(test)]
mod cpu_tests {
    use super::normalize_cpu_percent;

    #[test]
    fn cpu_multinucleo_se_normaliza_al_total_de_cpus() {
        assert_eq!(normalize_cpu_percent(400.0, 4), 100.0);
        assert_eq!(normalize_cpu_percent(200.0, 8), 25.0);
        assert_eq!(normalize_cpu_percent(100.0, 8), 12.5);
        assert_eq!(normalize_cpu_percent(1000.0, 8), 100.0);
    }

    #[test]
    fn cpu_descarta_valores_invalidos_y_evita_dividir_por_cero() {
        assert_eq!(normalize_cpu_percent(f32::NAN, 8), 0.0);
        assert_eq!(normalize_cpu_percent(f32::INFINITY, 8), 0.0);
        assert_eq!(normalize_cpu_percent(-10.0, 8), 0.0);
        assert_eq!(normalize_cpu_percent(50.0, 0), 50.0);
    }
}
