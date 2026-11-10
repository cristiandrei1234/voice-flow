//! Inregistrare de la microfonul implicit (sau unul ales dupa nume).
//!
//! Streamul porneste o singura data si ramane deschis; `start`/`stop` doar decid
//! daca esantioanele sunt pastrate. Asa prima silaba nu se pierde la pornire.

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use vf_application::SAMPLE_RATE;

struct Shared {
    recording: AtomicBool,
    buffer: Mutex<Vec<f32>>,
    /// Nivelul audio curent (RMS * 1000), pentru animatia pastilei.
    level: AtomicU32,
}

pub struct Recorder {
    shared: Arc<Shared>,
    device_rate: u32,
    device_name: String,
    _stream: Stream,
}

// cpal::Stream nu e Send pe macOS; Recorder ramane pe firul care l-a creat.

impl Recorder {
    pub fn new(device: Option<&str>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let dev = match device {
            Some(name) => host
                .input_devices()?
                .find(|d| d.name().map(|n| n.contains(name)).unwrap_or(false))
                .ok_or_else(|| anyhow!("Microphone \"{name}\" not found"))?,
            None => host
                .default_input_device()
                .context("There is no default microphone")?,
        };
        let device_name = dev.name().unwrap_or_else(|_| "microphone".into());
        let supported = dev
            .default_input_config()
            .context("The microphone has no input configuration")?;
        let device_rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        let shared = Arc::new(Shared {
            recording: AtomicBool::new(false),
            buffer: Mutex::new(Vec::new()),
            level: AtomicU32::new(0),
        });

        let err_fn = |e| eprintln!("audio error: {e}");
        let stream = match format {
            SampleFormat::F32 => {
                let s = shared.clone();
                dev.build_input_stream(
                    &config,
                    move |d: &[f32], _: &_| push(&s, d, channels, |x| x),
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let s = shared.clone();
                dev.build_input_stream(
                    &config,
                    move |d: &[i16], _: &_| push(&s, d, channels, |x| x as f32 / i16::MAX as f32),
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let s = shared.clone();
                dev.build_input_stream(
                    &config,
                    move |d: &[u16], _: &_| {
                        push(&s, d, channels, |x| (x as f32 - 32768.0) / 32768.0)
                    },
                    err_fn,
                    None,
                )?
            }
            other => return Err(anyhow!("Unsupported audio format: {other:?}")),
        };
        stream.play()?;
        Ok(Self {
            shared,
            device_rate,
            device_name,
            _stream: stream,
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn start(&self) {
        self.shared.buffer.lock().unwrap().clear();
        self.shared.recording.store(true, Ordering::SeqCst);
    }

    /// Opreste si intoarce audio-ul la 16 kHz mono.
    pub fn stop(&self) -> Vec<f32> {
        self.shared.recording.store(false, Ordering::SeqCst);
        let raw = std::mem::take(&mut *self.shared.buffer.lock().unwrap());
        resample(&raw, self.device_rate, SAMPLE_RATE)
    }

    /// Nivelul audio curent, 0.0 - 1.0 (aproximativ).
    pub fn level(&self) -> f32 {
        self.shared.level.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

/// Telecomanda unui microfon deschis pe alt fir: se poate trimite intre fire (streamul nu).
#[derive(Clone)]
pub struct RecorderHandle {
    shared: Arc<Shared>,
    device_rate: u32,
    device_name: String,
}

impl RecorderHandle {
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn start(&self) {
        self.shared.buffer.lock().unwrap().clear();
        self.shared.recording.store(true, Ordering::SeqCst);
    }

    /// Opreste si intoarce audio-ul la 16 kHz mono.
    pub fn stop(&self) -> Vec<f32> {
        self.shared.recording.store(false, Ordering::SeqCst);
        let raw = std::mem::take(&mut *self.shared.buffer.lock().unwrap());
        resample(&raw, self.device_rate, SAMPLE_RATE)
    }

    pub fn level(&self) -> f32 {
        self.shared.level.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

/// Deschide microfonul pe un fir separat si anunta cand e gata.
///
/// Deschiderea poate dura oricat (macOS o tine pe loc cat ecranul e blocat sau cat asteapta
/// raspunsul la dialogul de permisiune), asa ca nu are voie sa blocheze pornirea aplicatiei.
/// Firul ramane in viata cat traieste procesul, fiindca el detine streamul audio.
pub fn spawn(
    device: Option<String>,
    on_ready: impl FnOnce(anyhow::Result<RecorderHandle>) + Send + 'static,
) {
    let _ = std::thread::Builder::new()
        .name("vf-audio".into())
        .spawn(move || match Recorder::new(device.as_deref()) {
            Ok(rec) => {
                on_ready(Ok(RecorderHandle {
                    shared: rec.shared.clone(),
                    device_rate: rec.device_rate,
                    device_name: rec.device_name.clone(),
                }));
                loop {
                    std::thread::park();
                }
            }
            Err(e) => on_ready(Err(e)),
        });
}

fn push<T: Copy>(s: &Shared, data: &[T], channels: usize, conv: impl Fn(T) -> f32) {
    let mut sum = 0.0f32;
    let mono: Vec<f32> = data
        .chunks(channels.max(1))
        .map(|frame| frame.iter().map(|&x| conv(x)).sum::<f32>() / frame.len() as f32)
        .inspect(|x| sum += x * x)
        .collect();
    if !mono.is_empty() {
        let rms = (sum / mono.len() as f32).sqrt();
        s.level.store((rms * 1000.0) as u32, Ordering::Relaxed);
    }
    if s.recording.load(Ordering::Relaxed) {
        s.buffer.lock().unwrap().extend_from_slice(&mono);
    }
}

/// Reesantionare simpla: filtru medie (anti-aliasing) + interpolare liniara.
/// Suficient pentru vorbire (48 kHz -> 16 kHz e factor 3 exact).
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let filtered: Vec<f32> = if ratio > 1.0 {
        let w = ratio.round().max(1.0) as usize;
        let mut acc = 0.0f32;
        let mut out = Vec::with_capacity(input.len());
        for i in 0..input.len() {
            acc += input[i];
            if i >= w {
                acc -= input[i - w];
            }
            out.push(acc / (i + 1).min(w) as f32);
        }
        out
    } else {
        input.to_vec()
    };
    let n = (input.len() as f64 / ratio).floor() as usize;
    (0..n)
        .map(|i| {
            let pos = i as f64 * ratio;
            let j = pos.floor() as usize;
            let frac = (pos - j as f64) as f32;
            let a = filtered[j.min(filtered.len() - 1)];
            let b = filtered[(j + 1).min(filtered.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_lengths() {
        assert_eq!(resample(&vec![0.0; 48_000], 48_000, 16_000).len(), 16_000);
        assert_eq!(resample(&vec![0.0; 44_100], 44_100, 16_000).len(), 16_000);
        assert_eq!(resample(&[1.0, 2.0], 16_000, 16_000), vec![1.0, 2.0]);
    }

    #[test]
    fn resample_keeps_low_frequency() {
        // Sinus de 440 Hz la 48 kHz trebuie sa ramana cu amplitudine ~1 la 16 kHz.
        let src: Vec<f32> = (0..48_000)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin())
            .collect();
        let out = resample(&src, 48_000, 16_000);
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.9, "peak {peak}");
    }
}
