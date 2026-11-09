use std::num::{NonZeroU16, NonZeroU32};
use std::sync::{Arc, Condvar, Mutex};

use rodio::Source as RodioSource;

use super::State;

/// Audio pre-decodificado a muestras PCM listas para reproducción instantánea.
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

/// Comandos enviados al thread de audio.
pub enum AudioCmd {
    Play {
        audio: Arc<DecodedAudio>,
        loop_: bool,
    },
    Stop,
}

pub(crate) type AudioSlot = Arc<(Mutex<Option<AudioCmd>>, Condvar)>;

pub(crate) fn send_audio(slot: &AudioSlot, cmd: AudioCmd) {
    let (lock, cvar) = &**slot;
    *lock.lock().unwrap() = Some(cmd);
    cvar.notify_one();
}

fn samples_buffer(audio: &DecodedAudio) -> rodio::buffer::SamplesBuffer {
    let channels = NonZeroU16::new(audio.channels).expect("channels > 0");
    let sample_rate = NonZeroU32::new(audio.sample_rate).expect("sample_rate > 0");
    rodio::buffer::SamplesBuffer::new(channels, sample_rate, audio.samples.clone())
}

/// Lanza el thread dedicado de audio.
pub(crate) fn start_audio_thread() -> Option<AudioSlot> {
    let slot: AudioSlot = Arc::new((Mutex::new(None), Condvar::new()));
    let slot_thread = Arc::clone(&slot);
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let device_sink = match rodio::DeviceSinkBuilder::open_default_sink() {
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
