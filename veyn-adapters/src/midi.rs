//! MIDI adapter — listens for CC events, note on/off, and clock via `midir`.
//! Emits:
//!   - `midi_cc_<N>` (value 0–127) for Control Change messages
//!   - `midi_note_on` / `midi_note_off` (value = note number 0–127)
//!   - `midi_clock` (value 1.0) for MIDI timing clock pulses

use anyhow::Result;
use async_trait::async_trait;
use midir::{MidiInput, MidiInputConnection};
use tokio::sync::mpsc;
use tracing::{info, warn};
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

pub struct MidiAdapter;

impl MidiAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VeynAdapter for MidiAdapter {
    fn name(&self) -> &str {
        "midi"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        // midir is sync — run entirely in a blocking task.
        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_midi_loop(tx) {
                warn!("MIDI adapter error: {}", e);
            }
        })
        .await?;
        Ok(())
    }
}

fn run_midi_loop(tx: mpsc::Sender<VeynEvent>) -> Result<()> {
    let midi_in = MidiInput::new("veyn-midi-input")?;
    let ports = midi_in.ports();

    if ports.is_empty() {
        warn!("MIDI: no input ports found; adapter idle");
        // Block forever (caller's retry loop handles exit when channel closes).
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if tx.is_closed() {
                return Ok(());
            }
        }
    }

    let port = &ports[0];
    let port_name = midi_in.port_name(port)?;
    info!(port = %port_name, "MIDI adapter connected");

    let tx_inner = tx.clone();
    // Keep _conn alive for the duration.
    let _conn: MidiInputConnection<()> =
        midi_in.connect(port, "veyn-listener", move |_ts, msg, _| {
            if let Some(ev) = parse_midi_message(msg) {
                let _ = tx_inner.blocking_send(ev);
            }
        }, ()).map_err(|e| anyhow::anyhow!("MIDI connect error: {}", e.kind()))?;

    // Hold the connection open until the channel closes.
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if tx.is_closed() {
            break;
        }
    }
    Ok(())
}

fn parse_midi_message(msg: &[u8]) -> Option<VeynEvent> {
    if msg.is_empty() {
        return None;
    }
    let status = msg[0] & 0xF0;
    let channel = msg[0] & 0x0F;
    let device_id = format!("midi:ch{channel}");

    match status {
        0x90 if msg.len() >= 3 => {
            // Note On
            let note = msg[1];
            let velocity = msg[2];
            let (metric, value) = if velocity == 0 {
                ("midi_note_off", note as f64)
            } else {
                ("midi_note_on", note as f64)
            };
            Some(
                VeynEvent::new(&device_id, "midi", metric, value, "note")
                    .with_meta("velocity", serde_json::Value::Number(velocity.into()))
                    .with_meta("channel", serde_json::Value::Number(channel.into())),
            )
        }
        0x80 if msg.len() >= 2 => {
            // Note Off
            Some(
                VeynEvent::new(&device_id, "midi", "midi_note_off", msg[1] as f64, "note")
                    .with_meta("channel", serde_json::Value::Number(channel.into())),
            )
        }
        0xB0 if msg.len() >= 3 => {
            // Control Change
            let cc_num = msg[1];
            let cc_val = msg[2];
            let metric = format!("midi_cc_{cc_num}");
            Some(
                VeynEvent::new(&device_id, "midi", &metric, cc_val as f64, "")
                    .with_meta("channel", serde_json::Value::Number(channel.into())),
            )
        }
        0xF8 => {
            // MIDI Timing Clock
            Some(VeynEvent::new("midi:clock", "midi", "midi_clock", 1.0, "pulse"))
        }
        _ => None,
    }
}
