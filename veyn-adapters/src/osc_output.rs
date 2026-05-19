//! 15.2 — OSC Output Adapter
//!
//! Pushes live somatic z-scores and intent data to DAW/VJ software over OSC.
//! Subscribes to the `ContextSnapshot` broadcast and sends:
//!
//! - `/veyn/intent`        (string) — current intent_code
//! - `/veyn/confidence`    (float)  — 0.0–1.0
//! - `/veyn/zscore/<metric>` (float) — per-metric baseline z-score
//!
//! Configure with `VEYN_OSC_OUTPUT_HOST` (default `127.0.0.1:9001`).

use std::net::UdpSocket;

use anyhow::{Context, Result};
use rosc::{encoder, OscMessage, OscPacket, OscType};
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use veyn_schemas::ContextSnapshot;

/// Run the OSC output adapter.  Sends context snapshots as OSC messages to the
/// given `host:port` destination.
pub async fn run(mut rx: broadcast::Receiver<ContextSnapshot>, dest: String) -> Result<()> {
    let sock =
        UdpSocket::bind("0.0.0.0:0").context("OSC output: bind ephemeral UDP")?;
    info!(dest = %dest, "OSC output adapter started");

    loop {
        match rx.recv().await {
            Ok(snapshot) => {
                let intent_str = intent_code_str(&snapshot.intent_code);

                // /veyn/intent
                send_osc_string(&sock, &dest, "/veyn/intent", intent_str);

                // /veyn/confidence
                send_osc_float(&sock, &dest, "/veyn/confidence", snapshot.confidence as f32);

                // /veyn/zscore/<metric>
                if let Some(ref z_scores) = snapshot.baseline_delta {
                    for (metric, zscore) in z_scores {
                        let addr = format!("/veyn/zscore/{metric}");
                        send_osc_float(&sock, &dest, &addr, *zscore as f32);
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("OSC output lagged {} snapshots", n);
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}

fn send_osc_float(sock: &UdpSocket, dest: &str, addr: &str, val: f32) {
    let msg = OscPacket::Message(OscMessage {
        addr: addr.to_string(),
        args: vec![OscType::Float(val)],
    });
    match encoder::encode(&msg) {
        Ok(buf) => {
            if let Err(e) = sock.send_to(&buf, dest) {
                error!("OSC send to {dest}: {e}");
            }
        }
        Err(e) => warn!("OSC encode error: {e}"),
    }
}

fn send_osc_string(sock: &UdpSocket, dest: &str, addr: &str, val: &str) {
    let msg = OscPacket::Message(OscMessage {
        addr: addr.to_string(),
        args: vec![OscType::String(val.to_string())],
    });
    match encoder::encode(&msg) {
        Ok(buf) => {
            if let Err(e) = sock.send_to(&buf, dest) {
                error!("OSC send to {dest}: {e}");
            }
        }
        Err(e) => warn!("OSC encode error: {e}"),
    }
}

fn intent_code_str(code: &veyn_schemas::IntentCode) -> &str {
    match code {
        veyn_schemas::IntentCode::Neutral => "neutral",
        veyn_schemas::IntentCode::CognitiveLoad => "cognitive_load",
        veyn_schemas::IntentCode::StressResponse => "stress_response",
        veyn_schemas::IntentCode::Approach => "approach",
        veyn_schemas::IntentCode::Avoidance => "avoidance",
        veyn_schemas::IntentCode::Fatigue => "fatigue",
        veyn_schemas::IntentCode::Recovery => "recovery",
        veyn_schemas::IntentCode::Other(s) => s.as_str(),
    }
}
