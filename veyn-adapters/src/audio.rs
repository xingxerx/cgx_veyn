//! 15.2 — CPAL Audio Adapter for Ambient RMS/Peak Ingestion
//!
//! Captures the default audio input device and computes real-time RMS (Root
//! Mean Square) and Peak levels. Emits `audio_rms` and `audio_peak` events.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

const DEVICE_ID: &str = "audio-ambient";
const SOURCE: &str = "audio";

pub struct AudioAdapter;

impl AudioAdapter {
	pub fn new() -> Self {
		Self
	}
}

#[async_trait]
impl VeynAdapter for AudioAdapter {
	fn name(&self) -> &str {
		SOURCE
	}

	async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
		// Clone tx so we can move it into the background thread.
		let tx_clone = tx.clone();

		// Spawn a dedicated background OS thread to run CPAL stream and evaluate RMS/Peak.
		// This avoids keeping the non-Send/non-Sync cpal::Stream in the async future.
		let handle = std::thread::spawn(move || -> Result<()> {
			let host = cpal::default_host();
			let device = host
				.default_input_device()
				.context("no default audio input device found")?;

			let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
			info!(device = %name, "audio adapter thread: using default input");

			let config = device
				.default_input_config()
				.context("failed to get default input config")?;

			// Shared state for accumulating samples.
			let sample_buffer = Arc::new(Mutex::new(Vec::new()));
			let buffer_clone = sample_buffer.clone();

			// Stream error callback.
			let err_fn = |err| error!("audio input stream error: {}", err);

			// Stream build helper.
			let stream = match config.sample_format() {
				cpal::SampleFormat::F32 => device.build_input_stream(
					&config.into(),
					move |data: &[f32], _: &_| {
						if let Ok(mut buf) = buffer_clone.lock() {
							buf.extend_from_slice(data);
						}
					},
					err_fn,
					None,
				),
				cpal::SampleFormat::I16 => device.build_input_stream(
					&config.into(),
					move |data: &[i16], _: &_| {
						if let Ok(mut buf) = buffer_clone.lock() {
							buf.extend(data.iter().map(|&s| s as f32 / i16::MAX as f32));
						}
					},
					err_fn,
					None,
				),
				cpal::SampleFormat::U16 => device.build_input_stream(
					&config.into(),
					move |data: &[u16], _: &_| {
						if let Ok(mut buf) = buffer_clone.lock() {
							buf.extend(data.iter().map(|&s| {
								(s as f32 - u16::MAX as f32 / 2.0) / (u16::MAX as f32 / 2.0)
							}));
						}
					},
					err_fn,
					None,
				),
				_ => return Err(anyhow::anyhow!("Unsupported sample format")),
			}?;

			stream.play().context("failed to start audio play stream")?;
			info!("audio input capture stream active");

			// Run periodic evaluation loop (every 100ms, i.e. 10Hz).
			loop {
				std::thread::sleep(Duration::from_millis(100));

				let mut samples = Vec::new();
				if let Ok(mut buf) = sample_buffer.lock() {
					std::mem::swap(&mut *buf, &mut samples);
				}

				if samples.is_empty() {
					// Check if channel is closed.
					if tx_clone.is_closed() {
						break;
					}
					continue;
				}

				// Compute Peak and RMS.
				let mut peak = 0.0f32;
				let mut sum_sq = 0.0f64;
				let count = samples.len();

				for &s in &samples {
					let abs = s.abs();
					if abs > peak {
						peak = abs;
					}
					sum_sq += (s as f64) * (s as f64);
				}

				let rms = (sum_sq / count as f64).sqrt();

				// Emit events using blocking_send.
				let ev_rms = VeynEvent::new(DEVICE_ID, SOURCE, "audio_rms", rms, "RMS");
				let ev_peak = VeynEvent::new(DEVICE_ID, SOURCE, "audio_peak", peak as f64, "Peak");

				if tx_clone.blocking_send(ev_rms).is_err() || tx_clone.blocking_send(ev_peak).is_err() {
					warn!("audio adapter receiver dropped; shutting down audio stream");
					break;
				}
			}

			drop(stream);
			Ok(())
		});

		// Monitor the handle or wait until channel is closed.
		tokio::task::spawn_blocking(move || {
			if let Err(e) = handle.join().unwrap() {
				error!("audio adapter thread crashed: {:?}", e);
			}
		});

		Ok(())
	}
}
