//! Formato compacto de logs del motor: `[módulo] mensaje` (sin fecha ni nivel).

/// Inicializa `env_logger` en stderr con formato `[module_path] args`.
pub fn init(default_filter: &str) {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_filter))
        .format(|buf, record| {
            use std::io::Write;
            writeln!(
                buf,
                "[{}] {}",
                record.module_path().unwrap_or(record.target()),
                record.args(),
            )
        })
        .init();
}
