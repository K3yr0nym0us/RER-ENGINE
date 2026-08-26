// CPU/GPU del proceso del motor (no uso global del SO).
// Windows: contadores GPU Engine por PID.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sysinfo::{Pid, ProcessesToUpdate, System};

/// Intervalo entre lecturas (cada lectura tarda ~2 s por SampleInterval + MaxSamples).
const GPU_SAMPLE_INTERVAL: Duration = Duration::from_secs(3);
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
        self.system
            .process(pid)
            .map(|p| p.cpu_usage())
            .unwrap_or(0.0)
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

fn store_gpu_percent(raw: &AtomicU32, percent: Option<f32>) {
    match percent {
        Some(v) if v.is_finite() && v >= 0.0 => {
            raw.store(((v.min(100.0)) * 10.0).round() as u32, Ordering::Relaxed);
        }
        _ => raw.store(GPU_RAW_NONE, Ordering::Relaxed),
    }
}

fn spawn_gpu_poll_thread(
    pid: u32,
    raw: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
) -> Option<JoinHandle<()>> {
    if !gpu_sampling_available() {
        return None;
    }
    Some(thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            store_gpu_percent(&raw, query_process_gpu_percent(pid));
            thread::sleep(GPU_SAMPLE_INTERVAL);
        }
    }))
}

/// Indica si el motor puede intentar muestrear % GPU en esta plataforma.
pub fn is_engine_gpu_metrics_available() -> bool {
    gpu_sampling_available()
}

fn gpu_sampling_available() -> bool {
    true
}

/// Suma utilización GPU de todos los motores/gráficos asociados a un PID (solo ese proceso).
fn query_process_gpu_percent(pid: u32) -> Option<f32> {
    windows::query_process_gpu_percent(pid)
}

mod windows {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    pub fn query_process_gpu_percent(pid: u32) -> Option<f32> {
        let script = format!(
            r#"$ErrorActionPreference = 'SilentlyContinue'; $c = Get-Counter '\GPU Engine(*)\Utilization Percentage' -SampleInterval 1 -MaxSamples 2; if (-not $c) {{ Write-Output ''; exit 0 }}; $samples = $c[-1].CounterSamples | Where-Object {{ $_.InstanceName -match 'pid_{pid}_' }}; if (-not $samples) {{ Write-Output ''; exit 0 }}; $sum = ($samples | Measure-Object -Property CookedValue -Sum).Sum; if ($null -eq $sum) {{ Write-Output ''; exit 0 }}; Write-Output ($sum.ToString('0.###', [System.Globalization.CultureInfo]::InvariantCulture))"#
        );
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_gpu_stdout(&String::from_utf8_lossy(&out.stdout))
    }

    fn parse_gpu_stdout(raw: &str) -> Option<f32> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let normalized = trimmed.replace(',', ".");
        let value: f32 = normalized.parse().ok()?;
        if value.is_finite() && value >= 0.0 {
            Some(value.min(100.0))
        } else {
            None
        }
    }
}
