use std::{
    f32::consts::TAU,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread,
};

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};

type LogFn = Arc<dyn Fn(String) + Send + Sync + 'static>;

#[derive(Clone)]
pub struct Audio {
    tx: mpsc::Sender<Cmd>,
}

enum Cmd {
    SndOna(u8),
    Mute(bool),
}

impl Audio {
    pub fn spawn<F>(log: F) -> Self
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let log: LogFn = Arc::new(log);
        thread::spawn({
            let log = log.clone();
            move || audio_thread(rx, log)
        });
        Self { tx }
    }

    pub fn snd(&self, ona: u8) {
        let _ = self.tx.send(Cmd::SndOna(ona));
    }

    pub fn mute(&self, val: bool) {
        let _ = self.tx.send(Cmd::Mute(val));
    }
}

struct SharedState {
    freq_bits: AtomicU32,
    muted: AtomicBool,
}

fn audio_thread(rx: mpsc::Receiver<Cmd>, log: LogFn) {
    let shared = Arc::new(SharedState {
        freq_bits: AtomicU32::new(0.0f32.to_bits()),
        muted: AtomicBool::new(false),
    });

    let mut stream: Option<cpal::Stream> = None;
    let mut init_failed = false;
    let mut warned_unavailable = false;

    while let Ok(cmd) = rx.recv() {
        if stream.is_none() && !init_failed {
            match try_init_stream(shared.clone(), log.clone()) {
                Ok(s) => {
                    if let Err(err) = s.play() {
                        init_failed = true;
                        if !warned_unavailable {
                            warned_unavailable = true;
                            log(format!("audio: failed to play stream: {err}"));
                        }
                    } else {
                        stream = Some(s);
                    }
                }
                Err(err) => {
                    init_failed = true;
                    if !warned_unavailable {
                        warned_unavailable = true;
                        log(format!("audio: unavailable ({err})"));
                    }
                }
            }
        }

        match cmd {
            Cmd::SndOna(ona) => {
                let freq = ona_to_freq_hz(ona);
                shared.freq_bits.store(freq.to_bits(), Ordering::Relaxed);
            }
            Cmd::Mute(val) => {
                shared.muted.store(val, Ordering::Relaxed);
                if val {
                    shared.freq_bits.store(0.0f32.to_bits(), Ordering::Relaxed);
                }
            }
        }
    }
}

fn try_init_stream(shared: Arc<SharedState>, log: LogFn) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;

    let supported = device
        .default_output_config()
        .map_err(|err| format!("default_output_config: {err}"))?;
    let config = supported.config();

    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0 as f32;

    let err_fn = {
        let log = log.clone();
        move |err| {
            log(format!("audio: stream error: {err}"));
        }
    };

    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let shared = shared.clone();
            let mut phase = 0.0f32;
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [f32], _| {
                        fill_tone(data, channels, sample_rate, &shared, &mut phase)
                    },
                    err_fn,
                    None,
                )
                .map_err(|err| format!("build_output_stream(f32): {err}"))?
        }
        cpal::SampleFormat::I16 => {
            let shared = shared.clone();
            let mut phase = 0.0f32;
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [i16], _| {
                        fill_tone(data, channels, sample_rate, &shared, &mut phase)
                    },
                    err_fn,
                    None,
                )
                .map_err(|err| format!("build_output_stream(i16): {err}"))?
        }
        cpal::SampleFormat::U16 => {
            let shared = shared.clone();
            let mut phase = 0.0f32;
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [u16], _| {
                        fill_tone(data, channels, sample_rate, &shared, &mut phase)
                    },
                    err_fn,
                    None,
                )
                .map_err(|err| format!("build_output_stream(u16): {err}"))?
        }
        other => return Err(format!("unsupported sample format: {other:?}")),
    };

    Ok(stream)
}

fn fill_tone<T: cpal::Sample + cpal::FromSample<f32>>(
    output: &mut [T],
    channels: usize,
    sample_rate: f32,
    shared: &SharedState,
    phase: &mut f32,
) {
    let freq = f32::from_bits(shared.freq_bits.load(Ordering::Relaxed));
    let muted = shared.muted.load(Ordering::Relaxed);
    let amp = if muted || freq <= 0.0 { 0.0 } else { 0.20 };

    let step = if amp == 0.0 {
        0.0
    } else {
        TAU * freq / sample_rate
    };

    for frame in output.chunks_mut(channels) {
        let value_f32 = if amp == 0.0 {
            0.0
        } else {
            (*phase).sin() * amp
        };
        if amp != 0.0 {
            *phase += step;
            if *phase >= TAU {
                *phase -= TAU;
            }
        }

        let value: T = <T as cpal::FromSample<f32>>::from_sample_(value_f32);
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}

fn ona_to_freq_hz(ona: u8) -> f32 {
    if ona == 0 {
        return 0.0;
    }
    // TempleOS: Ona=60 is 440Hz; Freq = 440/32 * 2^(ona/12)
    let ona = ona as f64;
    (440.0 / 32.0 * 2.0_f64.powf(ona / 12.0)) as f32
}
