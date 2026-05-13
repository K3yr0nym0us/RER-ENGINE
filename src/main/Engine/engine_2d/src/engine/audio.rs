use std::sync::{Arc, Condvar, Mutex};

use rodio;
use rodio::Source as RodioSource;

use super::State;

// ---------------------------------------------------------------------------
// Thread dedicado de audio
// ---------------------------------------------------------------------------

/// Audio pre-decodificado a muestras PCM listas para reproducción instantánea.
/// Se genera una vez en `SetAnimation` y se reutiliza en cada `PlayAnimation`.
pub struct DecodedAudio {
    pub samples:     Vec<i16>,
    pub channels:    u16,
    pub sample_rate: u32,
}

/// Comandos enviados al thread de audio.
pub enum AudioCmd {
    /// Reproducir audio desde muestras PCM ya decodificadas en RAM.
    /// El Sink nunca se destruye — solo se vacía la cola y se agrega el nuevo source.
    Play { audio: Arc<DecodedAudio>, loop_: bool },
    /// Detener el audio en curso (vacía la cola, el Sink sigue vivo).
    Stop,
}

/// Single-slot "latest wins": solo el comando más reciente importa.
/// Si el thread de audio está ocupado procesando y llegan 10 Play seguidos,
/// solo se ejecuta el último — sin acumulación de cola.
pub(crate) type AudioSlot = Arc<(Mutex<Option<AudioCmd>>, Condvar)>;

/// Envía un comando al thread de audio sobreescribiendo cualquier
/// comando pendiente aún no procesado.
pub(super) fn send_audio(slot: &AudioSlot, cmd: AudioCmd) {
    let (lock, cvar) = &**slot;
    *lock.lock().unwrap() = Some(cmd);
    cvar.notify_one();
}

/// Lanza el thread dedicado de audio.
///
/// Diseño:
///   - Un único `OutputStream` (conexión ALSA) vive todo el tiempo del thread.
///   - Cada `Play` crea un Sink NUEVO desde el handle existente (sin sink.clear()).
///     `sink.clear()` puede deadlock en WSL/ALSA cuando el stream subyacente se invalida;
///     un Sink fresco evita ese riesgo completamente.
///   - Sonidos no-looping: `sink.detach()` → fire & forget, múltiples SFX simultáneos.
///   - Sonido looping: se guarda en `loop_sink` y se reemplaza en el siguiente Play.
///   - `Sink::try_new(&handle)` es O(1) (solo envía un mensaje al mixer existente),
///     muy distinto de `OutputStream::try_default()` que abre un nuevo dispositivo ALSA.
pub(super) fn start_audio_thread() -> Option<AudioSlot> {
    let slot: AudioSlot = Arc::new((Mutex::new(None), Condvar::new()));
    let slot_thread = Arc::clone(&slot);
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let (_stream, handle) = match rodio::OutputStream::try_default() {
                Ok(pair) => pair,
                Err(e) => {
                    log::error!("[audio] thread: no se pudo abrir dispositivo: {e}");
                    return;
                }
            };

            let (lock, cvar) = &*slot_thread;
            // Sink para sonido looping (música, ambience). None = ninguno activo.
            let mut loop_sink: Option<rodio::Sink> = None;

            loop {
                let cmd = {
                    let mut guard = lock.lock().unwrap();
                    loop {
                        if let Some(cmd) = guard.take() {
                            break cmd;
                        }
                        guard = cvar.wait(guard).unwrap();
                    }
                };
                match cmd {
                    AudioCmd::Stop => {
                        // Detener música looping si la hay; drop() detiene el Sink.
                        if let Some(s) = loop_sink.take() {
                            drop(s);
                        }
                    }
                    AudioCmd::Play { audio, loop_ } => {
                        // Crear un Sink fresco por reproducción — evita sink.clear() y
                        // permite múltiples SFX simultáneos vía detach().
                        let sink = match rodio::Sink::try_new(&handle) {
                            Ok(s) => s,
                            Err(e) => {
                                log::error!("[audio] no se pudo crear sink: {e}");
                                continue;
                            }
                        };
                        let source = rodio::buffer::SamplesBuffer::new(
                            audio.channels,
                            audio.sample_rate,
                            audio.samples.clone(),
                        );
                        if loop_ {
                            // Reemplazar música anterior (drop detiene la anterior).
                            if let Some(prev) = loop_sink.take() { drop(prev); }
                            sink.append(source.repeat_infinite());
                            sink.play();
                            loop_sink = Some(sink);
                        } else {
                            // SFX one-shot: fire & forget. Varios pueden solaparse.
                            sink.append(source);
                            sink.play();
                            sink.detach();
                        }
                        log::debug!("[audio] reproduciendo ({} muestras, {}ch, {}Hz, loop={})",
                            audio.samples.len(), audio.channels, audio.sample_rate, loop_);
                    }
                }
            }
        })
        .expect("no se pudo crear el thread de audio");
    log::info!("[audio] dispositivo de audio inicializado");
    Some(slot)
}

impl State {
    pub(super) fn play_audio_internal(&mut self, audio: Arc<DecodedAudio>, loop_: bool) {
        if let Some(slot) = &self.audio_slot {
            send_audio(slot, AudioCmd::Play { audio, loop_ });
        } else {
            log::error!("[audio] thread de audio no disponible");
        }
    }

    pub(crate) fn stop_audio_internal(&mut self) {
        if let Some(slot) = &self.audio_slot {
            send_audio(slot, AudioCmd::Stop);
        }
    }
}
