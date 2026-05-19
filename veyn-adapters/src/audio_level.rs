//! Audio Level Adapter — RMS/peak metering from default input device.
//!
//! Uses `cpal` to capture audio samples and compute RMS (root mean square) and peak levels.
//! Emits `VeynEvent`s with metrics: `audio_rms`, `audio_peak`.
//!
//! Configured via `[audio_level]` section in `veyn.toml`:
//! ```toml
//! [audio_level]
//! enabled = true
//! # Sample window in milliseconds for RMS/peak computation
//! window_ms = 100
//! # Optional: specific device name (uses default if not set)
//! # device_name = "Built-in Microphone"
//! ```

use anyhow::{Context, Result};
use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

/// Configuration for the audio level adapter.
#[derive(Debug, Clone, Deserialize)]
pub struct AudioLevelConfig {
    /// Enable/disable the adapter.
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// Sample window in milliseconds for RMS/peak computation.
    #[serde(default = "default_window_ms")]
    pub window_ms: u32,
    /// Optional specific device name (uses default if not set).
    #[serde(default)]
    pub device_name: Option<String>,
}

fn default_false() -> bool {
    false
}

fn default_window_ms() -> u32 {
    100
}

impl Default for AudioLevelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            window_ms: default_window_ms(),
            device_name: None,
        }
    }
}

/// Audio level adapter that captures microphone input and emits RMS/peak levels.
pub struct AudioLevelAdapter {
    config: Arc<AudioLevelConfig>,
    running: Arc<AtomicBool>,
}

impl AudioLevelAdapter {
    /// Create a new audio level adapter.
    pub fn new(config: AudioLevelConfig) -> Self {
        Self {
            config: Arc::new(config),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Compute RMS and peak from a buffer of f32 samples.
    fn compute_levels(samples: &[f32]) -> (f64, f64) {
        if samples.is_empty() {
            return (0.0, 0.0);
        }

        let sum_squares: f32 = samples.iter().map(|s| s * s).sum();
        let rms = (sum_squares / samples.len() as f32).sqrt() as f64;
        let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max) as f64;

        (rms, peak)
    }

    /// Run the audio capture loop.
    async fn run_capture_loop(
        config: Arc<AudioLevelConfig>,
        tx: mpsc::Sender<VeynEvent>,
        running: Arc<AtomicBool>,
    ) -> Result<()> {
        let host = cpal::default_host();

        // Find device
        let device = if let Some(name) = &config.device_name {
            host.devices()?
                .find(|d| d.name().map(|n| n == *name).unwrap_or(false))
                .context("Specified audio device not found")?
        } else {
            host.default_input_device()
                .context("No default input device available")?
        };

        info!("Audio level adapter using device: {}", device.name()?);

        let stream_config: StreamConfig = device.default_input_config()?.into();
        let sample_rate = stream_config.sample_rate.0;
        let channels = stream_config.channels as usize;

        // Calculate samples per window
        let samples_per_window = (sample_rate * config.window_ms / 1000) as usize;
        let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_window);

        // Build stream based on sample format
        let running_clone = Arc::clone(&running);
        let tx_clone = tx.clone();

        let err_fn = |err| error!("Audio stream error: {}", err);

        let stream = match device.default_input_format()? {
            cpal::StreamFormat {
                data_type: cpal::SampleDataType::I16,
                ..
            } => {
                device.build_input_stream(
                    &stream_config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !running_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        // Convert i16 to f32 and accumulate
                        for &sample in data {
                            let normalized = sample as f32 / i16::MAX as f32;
                            buffer.push(normalized);
                            if buffer.len() >= samples_per_window {
                                let (rms, peak) = AudioLevelAdapter::compute_levels(&buffer);
                                let _ = tx_clone.try_send(VeynEvent::new(
                                    "audio_input",
                                    "audio_level",
                                    "audio_rms",
                                    rms,
                                    "normalized",
                                ));
                                let _ = tx_clone.try_send(VeynEvent::new(
                                    "audio_input",
                                    "audio_level",
                                    "audio_peak",
                                    peak,
                                    "normalized",
                                ));
                                buffer.clear();
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::StreamFormat {
                data_type: cpal::SampleDataType::U16,
                ..
            } => {
                device.build_input_stream(
                    &stream_config.into(),
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if !running_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        // Convert u16 to f32 and accumulate
                        for &sample in data {
                            let normalized = (sample as i16) as f32 / i16::MAX as f32;
                            buffer.push(normalized);
                            if buffer.len() >= samples_per_window {
                                let (rms, peak) = AudioLevelAdapter::compute_levels(&buffer);
                                let _ = tx_clone.try_send(VeynEvent::new(
                                    "audio_input",
                                    "audio_level",
                                    "audio_rms",
                                    rms,
                                    "normalized",
                                ));
                                let _ = tx_clone.try_send(VeynEvent::new(
                                    "audio_input",
                                    "audio_level",
                                    "audio_peak",
                                    peak,
                                    "normalized",
                                ));
                                buffer.clear();
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            _ => {
                // Default to f32
                device.build_input_stream(
                    &stream_config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !running_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        for &sample in data {
                            buffer.push(sample);
                            if buffer.len() >= samples_per_window {
                                let (rms, peak) = AudioLevelAdapter::compute_levels(&buffer);
                                let _ = tx_clone.try_send(VeynEvent::new(
                                    "audio_input",
                                    "audio_level",
                                    "audio_rms",
                                    rms,
                                    "normalized",
                                ));
                                let _ = tx_clone.try_send(VeynEvent::new(
                                    "audio_input",
                                    "audio_level",
                                    "audio_peak",
                                    peak,
                                    "normalized",
                                ));
                                buffer.clear();
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
        };

        stream.play()?;
        info!(
            "Audio level adapter started: {}ms window, {} Hz, {} channels",
            config.window_ms, sample_rate, channels
        );

        // Keep running until stopped
        while running.load(Ordering::Relaxed) {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        Ok(())
    }
}

#[async_trait]
impl VeynAdapter for AudioLevelAdapter {
    fn name(&self) -> &str {
        "audio_level"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        if !self.config.enabled {
            debug!("Audio level adapter disabled");
            return Ok(());
        }

        self.running.store(true, Ordering::Relaxed);

        let config = Arc::clone(&self.config);
        let running = Arc::clone(&self.running);

        tokio::spawn(async move {
            if let Err(e) = Self::run_capture_loop(config, tx, running).await {
                error!("Audio level adapter error: {}", e);
            }
        });

        Ok(())
    }
}

impl Drop for AudioLevelAdapter {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AudioLevelConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.window_ms, 100);
    }

    #[test]
    fn test_compute_levels_sine() {
        // Generate a simple sine wave approximation
        let samples: Vec<f32> = (0..1000)
            .map(|i| ((i as f32 * 0.1).sin() * 0.5))
            .collect();

        let (rms, peak) = AudioLevelAdapter::compute_levels(&samples);

        // RMS of sine with amplitude 0.5 should be ~0.35 (0.5 / sqrt(2))
        assert!(rms > 0.3 && rms < 0.4);
        // Peak should be close to 0.5
        assert!(peak > 0.49 && peak <= 0.5);
    }

    #[test]
    fn test_compute_levels_silence() {
        let samples = vec![0.0f32; 100];
        let (rms, peak) = AudioLevelAdapter::compute_levels(&samples);
        assert_eq!(rms, 0.0);
        assert_eq!(peak, 0.0);
    }
}
