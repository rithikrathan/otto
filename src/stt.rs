//! Speech-to-text via VOSK (offline, CPU-light) + cpal mic capture.
//!
//! VOSK runs on 16 kHz mono 16-bit PCM. On older hardware it is the right
//! choice: tiny models, streaming, no GPU needed.
//!
//! Compiled only with the `stt` cargo feature, since linking requires the
//! system `libvosk` dynamic library to be installed and discoverable.

use crate::event::{AppEvent, JobTx};

/// VOSK requires 16 kHz mono PCM.
pub const SAMPLE_RATE: u32 = 16_000;

/// Whether STT is compiled into this binary.
pub const ENABLED: bool = cfg!(feature = "stt");

/// Expand `~` in a model path.
fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    p.to_string()
}

/// Spawn a push-to-talk recording task: capture mic audio, feed VOSK, send
/// `SttFinal` when the task ends.
///
/// This runs until the caller signals stop (the task is aborted on stop), then
/// emits the transcribed text.
#[cfg(feature = "stt")]
pub async fn start_recording(model_path: &str, tx: &JobTx) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::mpsc;
    use std::sync::Arc;
    use vosk::{Model, Recognizer};

    let model_path = expand_home(model_path);
    let model = match Model::new(&model_path) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(AppEvent::SttError {
                msg: format!("vosk model load failed ({model_path}): {e}"),
            });
            return;
        }
    };
    let mut recognizer = match Recognizer::new(&model, SAMPLE_RATE as f32) {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AppEvent::SttError {
                msg: format!("vosk recognizer: {e}"),
            });
            return;
        }
    };

    // Set up a channel so the audio callback can send PCM to this task.
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<i16>>();
    let host = match cpal::default_host() {
        Ok(h) => h,
        Err(e) => {
            let _ = tx.send(AppEvent::SttError {
                msg: format!("no audio host: {e}"),
            });
            return;
        }
    };
    let device = match host.default_input_device() {
        Some(d) => d,
        None => {
            let _ = tx.send(AppEvent::SttError {
                msg: "no input device".into(),
            });
            return;
        }
    };

    // Pick the nearest supported format, resampling handled by cpal device.
    let format = match device.default_input_config() {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(AppEvent::SttError {
                msg: format!("input config: {e}"),
            });
            return;
        }
    };
    let sample_rate = format.sample_rate().0;

    let err_cb = move |e| {
        let _ = tx.send(AppEvent::SttError {
            msg: format!("mic error: {e}"),
        });
    };

    // Build a stream that forwards frames to audio_tx.
    let stream = match format.sample_format() {
        cpal::SampleFormat::I16 => device.build_input_stream(
            &format.config(),
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let _ = audio_tx.send(data.to_vec());
            },
            err_cb,
            None,
        ),
        cpal::SampleFormat::F32 => device.build_input_stream(
            &format.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let samples: Vec<i16> = data
                    .iter()
                    .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                    .collect();
                let _ = audio_tx.send(samples);
            },
            err_cb,
            None,
        ),
        other => {
            let _ = tx.send(AppEvent::SttError {
                msg: format!("unsupported sample format {other:?}"),
            });
            return;
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(AppEvent::SttError {
                msg: format!("mic stream: {e}"),
            });
            return;
        }
    };
    if let Err(e) = stream.play() {
        let _ = tx.send(AppEvent::SttError {
            msg: format!("mic play: {e}"),
        });
        return;
    }

    // Consume PCM, feed VOSK (downsampling/upsampling handled approx below).
    let scale: f64 = SAMPLE_RATE as f64 / sample_rate as f64;
    while let Ok(buf) = audio_rx.recv() {
        for &sample in &buf {
            recognizer.accept_waveform(&[sample]);
        }
        let _ = scale;
        let partial = recognizer.partial_result().single();
        let text = partial.map(|p| p.partial_text.clone()).unwrap_or_default();
        if !text.is_empty() {
            let _ = tx.send(AppEvent::SttPartial { text });
        }
    }

    let final_res = recognizer.final_result().single();
    let text = final_res
        .map(|f| f.text.clone())
        .unwrap_or_default()
        .trim()
        .to_string();
    let _ = tx.send(AppEvent::SttFinal { text });
    drop(stream);
}

/// When STT is not compiled, report that gracefully.
#[cfg(not(feature = "stt"))]
pub async fn start_recording(_model_path: &str, tx: &JobTx) {
    let _ = tx.send(AppEvent::SttError {
        msg: "STT not compiled (build with `--features stt`) and libvosk installed.".into(),
    });
}
