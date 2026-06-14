use std::sync::{Arc, Condvar, Mutex};

use rodio;
use rodio::Source as RodioSource;

use super::State;

/// Audio pre-decodificado a muestras PCM listas para reproducción instantánea.
/// Se genera una vez en `SetAnimation` y se reutiliza en cada `PlayAnimation`.
pub struct DecodedAudio {
    pub samples: Vec<i16>,
    pub channels: u16,
    pub sample_rate: u32,
}

/// Comandos enviados al thread de audio.
pub enum AudioCmd {
    /// Reproducir audio desde muestras PCM ya decodificadas en RAM.
    /// El Sink nunca se destruye: solo se vacía la cola y se agrega el nuevo source.
    Play { audio: Arc<DecodedAudio>, loop_: bool },
    /// Detener el audio en curso (vacía la cola, el Sink sigue vivo).
    Stop,
}

/// Single-slot "latest wins": solo el comando más reciente importa.
pub(crate) type AudioSlot = Arc<(Mutex<Option<AudioCmd>>, Condvar)>;

/// Envía un comando al thread de audio sobreescribiendo cualquier comando pendiente.
pub(crate) fn send_audio(slot: &AudioSlot, cmd: AudioCmd) {
    let (lock, cvar) = &**slot;
    *lock.lock().unwrap() = Some(cmd);
    cvar.notify_one();
}

/// Lanza el thread dedicado de audio.
pub(crate) fn start_audio_thread() -> Option<AudioSlot> {
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
                        if let Some(s) = loop_sink.take() {
                            drop(s);
                            
                        }
                    }
                    AudioCmd::Play { audio, loop_ } => {
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
                            if let Some(prev) = loop_sink.take() {
                                drop(prev);
                            }
                            sink.append(source.repeat_infinite());
                            sink.play();
                            loop_sink = Some(sink);
                        } else {
                            sink.append(source);
                            sink.play();
                            sink.detach();
                        }
                        
                    }
                }
            }
        })
        .expect("no se pudo crear el thread de audio");
    
    Some(slot)
}

impl State {
    pub(crate) fn play_audio_internal(&mut self, audio: Arc<DecodedAudio>, loop_: bool) {
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
