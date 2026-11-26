use std::num::{NonZeroU16, NonZeroU32};
use std::sync::{Arc, Condvar, Mutex};

use rodio::Source as RodioSource;

use super::State;

// ---------------------------------------------------------------------------
// Thread dedicado de audio
// ---------------------------------------------------------------------------

/// Audio pre-decodificado a muestras PCM listas para reproducción instantánea.
/// Se genera una vez en `SetAnimation` y se reutiliza en cada `PlayAnimation`.
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

/// Comandos enviados al thread de audio.
pub enum AudioCmd {
    /// Reproducir audio desde muestras PCM ya decodificadas en RAM.
    Play {
        audio: Arc<DecodedAudio>,
        loop_: bool,
    },
    /// Detener el audio en curso (música looping).
    Stop,
}

/// Single-slot "latest wins": solo el comando más reciente importa.
pub(crate) type AudioSlot = Arc<(Mutex<Option<AudioCmd>>, Condvar)>;

pub(super) fn send_audio(slot: &AudioSlot, cmd: AudioCmd) {
    let (lock, cvar) = &**slot;
    *lock.lock().unwrap() = Some(cmd);
    cvar.notify_one();
}

fn samples_buffer(audio: &DecodedAudio) -> rodio::buffer::SamplesBuffer {
    let channels = NonZeroU16::new(audio.channels).expect("channels > 0");
    let sample_rate = NonZeroU32::new(audio.sample_rate).expect("sample_rate > 0");
    rodio::buffer::SamplesBuffer::new(channels, sample_rate, audio.samples.clone())
}

fn is_actionable_stream_error(error: &rodio::cpal::StreamError) -> bool {
    // ALSA/WSLg notifica xruns ocasionales como BufferUnderrun. Son un posible
    // glitch, pero no requieren recrear el stream y no deben inundar el log.
    !matches!(error, rodio::cpal::StreamError::BufferUnderrun)
}

fn open_device_sink() -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    let builder = rodio::DeviceSinkBuilder::from_default_device()?.with_error_callback(|error| {
        if is_actionable_stream_error(&error) {
            log::error!("[audio] stream: {error}");
        }
    });
    // WSLg/ALSA se beneficia de un colchón mayor cuando el render 2D satura CPU.
    #[cfg(target_os = "linux")]
    let builder = builder.with_buffer_size(rodio::cpal::BufferSize::Fixed(4096));
    let mut sink = builder.open_sink_or_fallback()?;
    sink.log_on_drop(false);
    Ok(sink)
}

/// Lanza el thread dedicado de audio.
pub(super) fn start_audio_thread() -> Option<AudioSlot> {
    let slot: AudioSlot = Arc::new((Mutex::new(None), Condvar::new()));
    let slot_thread = Arc::clone(&slot);
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let device_sink = match open_device_sink() {
                Ok(handle) => handle,
                Err(e) => {
                    log::error!("[audio] thread: no se pudo abrir dispositivo: {e}");
                    return;
                }
            };

            let (lock, cvar) = &*slot_thread;
            let mut loop_player: Option<rodio::Player> = None;

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
                        if let Some(player) = loop_player.take() {
                            drop(player);
                        }
                    }
                    AudioCmd::Play { audio, loop_ } => {
                        let player = rodio::Player::connect_new(device_sink.mixer());
                        let source = samples_buffer(&audio);
                        if loop_ {
                            if let Some(prev) = loop_player.take() {
                                drop(prev);
                            }
                            player.append(source.repeat_infinite());
                            player.play();
                            loop_player = Some(player);
                        } else {
                            player.append(source);
                            player.play();
                            player.detach();
                        }
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

#[cfg(test)]
mod tests {
    use super::is_actionable_stream_error;
    use rodio::cpal::StreamError;

    #[test]
    fn los_xruns_de_alsa_son_benignos() {
        assert!(!is_actionable_stream_error(&StreamError::BufferUnderrun));
        assert!(is_actionable_stream_error(&StreamError::DeviceNotAvailable));
        assert!(is_actionable_stream_error(&StreamError::StreamInvalidated));
    }
}
