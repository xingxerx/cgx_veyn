use anyhow::{Context, Result};
use async_trait::async_trait;
use rosc::{OscMessage, OscPacket, OscType};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

const DEVICE_ID: &str = "eeg-headset";
const SOURCE: &str = "eeg";
// OSC address prefixes emitted by Mind Monitor for Muse S
const ADDR_EEG: &str = "/muse/eeg";
const ADDR_DELTA_ABS: &str = "/muse/elements/delta_absolute";
const ADDR_THETA_ABS: &str = "/muse/elements/theta_absolute";
const ADDR_ALPHA_ABS: &str = "/muse/elements/alpha_absolute";
const ADDR_BETA_ABS: &str = "/muse/elements/beta_absolute";
const ADDR_GAMMA_ABS: &str = "/muse/elements/gamma_absolute";
const ADDR_HORSESHOE: &str = "/muse/elements/horseshoe";
const ADDR_TOUCHING: &str = "/muse/elements/touching_forehead";
const ADDR_BLINK: &str = "/muse/elements/blink";
const ADDR_JAW_CLENCH: &str = "/muse/elements/jaw_clench";
const ADDR_BATTERY: &str = "/muse/batt";

pub struct EegAdapter {
    pub osc_port: u16,
}

impl EegAdapter {
    pub fn new(osc_port: u16) -> Self {
        Self { osc_port }
    }
}

#[async_trait]
impl VeynAdapter for EegAdapter {
    fn name(&self) -> &str {
        SOURCE
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        let bind_addr = format!("0.0.0.0:{}", self.osc_port);
        let sock = UdpSocket::bind(&bind_addr)
            .await
            .with_context(|| format!("EEG adapter: bind UDP {bind_addr}"))?;
        info!(port = self.osc_port, "EEG/OSC adapter listening");

        let mut buf = vec![0u8; 4096];
        loop {
            let (len, peer) = match sock.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    error!("EEG UDP recv error: {e}");
                    continue;
                }
            };

            debug!(peer = %peer, bytes = len, "OSC packet");

            match rosc::decoder::decode_udp(&buf[..len]) {
                Ok((_, packet)) => {
                    let events = process_packet(packet);
                    for ev in events {
                        if tx.send(ev).await.is_err() {
                            info!("EEG adapter: channel closed, stopping");
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    warn!(peer = %peer, "OSC decode error: {e}");
                }
            }
        }
    }
}

fn process_packet(packet: OscPacket) -> Vec<VeynEvent> {
    match packet {
        OscPacket::Message(msg) => process_message(msg),
        OscPacket::Bundle(bundle) => bundle
            .content
            .into_iter()
            .flat_map(process_packet)
            .collect(),
    }
}

fn process_message(msg: OscMessage) -> Vec<VeynEvent> {
    let addr = msg.addr.as_str();
    let args = &msg.args;

    match addr {
        ADDR_DELTA_ABS => band_events(args, "delta_absolute", "log_uv2", 4),
        ADDR_THETA_ABS => band_events(args, "theta_absolute", "log_uv2", 4),
        ADDR_ALPHA_ABS => band_events(args, "alpha_absolute", "log_uv2", 4),
        ADDR_BETA_ABS => band_events(args, "beta_absolute", "log_uv2", 4),
        ADDR_GAMMA_ABS => band_events(args, "gamma_absolute", "log_uv2", 4),
        ADDR_EEG => channel_events(args),
        ADDR_HORSESHOE => scalar_event(args, "fit_indicator", ""),
        ADDR_TOUCHING => bool_event(args, "touching_forehead"),
        ADDR_BLINK => bool_event(args, "blink"),
        ADDR_JAW_CLENCH => bool_event(args, "jaw_clench"),
        ADDR_BATTERY => battery_event(args),
        _ => vec![],
    }
}

/// Emit one averaged event plus per-channel events for a 4-channel band power message.
fn band_events(args: &[OscType], metric: &str, unit: &str, channels: usize) -> Vec<VeynEvent> {
    let floats: Vec<f64> = args
        .iter()
        .filter_map(|a| match a {
            OscType::Float(f) => Some(*f as f64),
            OscType::Double(d) => Some(*d),
            _ => None,
        })
        .take(channels)
        .collect();

    if floats.is_empty() {
        return vec![];
    }

    let avg = floats.iter().sum::<f64>() / floats.len() as f64;
    let channel_labels = ["TP9", "AF7", "AF8", "TP10"];

    let mut events = vec![VeynEvent::new(DEVICE_ID, SOURCE, metric, avg, unit)];

    for (i, &val) in floats.iter().enumerate() {
        let ch_label = channel_labels.get(i).copied().unwrap_or("ch");
        let ch_metric = format!("{metric}_{ch_label}");
        events.push(VeynEvent::new(DEVICE_ID, SOURCE, ch_metric, val, unit));
    }

    events
}

/// Raw EEG channel voltages (µV).
fn channel_events(args: &[OscType]) -> Vec<VeynEvent> {
    let channel_labels = ["TP9", "AF7", "AF8", "TP10"];
    args.iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let val = match a {
                OscType::Float(f) => *f as f64,
                OscType::Double(d) => *d,
                _ => return None,
            };
            let label = channel_labels.get(i).copied().unwrap_or("ch");
            Some(VeynEvent::new(
                DEVICE_ID,
                SOURCE,
                format!("eeg_raw_{label}"),
                val,
                "uV",
            ))
        })
        .collect()
}

fn scalar_event(args: &[OscType], metric: &str, unit: &str) -> Vec<VeynEvent> {
    args.iter()
        .find_map(|a| match a {
            OscType::Float(f) => Some(*f as f64),
            OscType::Double(d) => Some(*d),
            _ => None,
        })
        .map(|v| vec![VeynEvent::new(DEVICE_ID, SOURCE, metric, v, unit)])
        .unwrap_or_default()
}

fn bool_event(args: &[OscType], metric: &str) -> Vec<VeynEvent> {
    args.iter()
        .find_map(|a| match a {
            OscType::Int(i) => Some(*i as f64),
            OscType::Float(f) => Some(*f as f64),
            OscType::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        })
        .map(|v| vec![VeynEvent::new(DEVICE_ID, SOURCE, metric, v, "bool")])
        .unwrap_or_default()
}

fn battery_event(args: &[OscType]) -> Vec<VeynEvent> {
    // Mind Monitor sends: battery_level, fuel_gauge_voltage, adc_voltage, temperature
    args.first()
        .and_then(|a| match a {
            OscType::Int(i) => Some(*i as f64),
            OscType::Float(f) => Some(*f as f64),
            _ => None,
        })
        .map(|v| vec![VeynEvent::new(DEVICE_ID, SOURCE, "battery", v, "%")])
        .unwrap_or_default()
}
