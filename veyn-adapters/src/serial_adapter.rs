//! Serial/UART adapter — reads lines from a serial port and emits metrics.
//!
//! Line format: `KEY=FLOAT\n`  (e.g. `temperature=36.6\n`)
//! Each key becomes the metric name; the float value is the metric value.
//! Lines that don't match the format are silently skipped.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::io::{BufRead, BufReader};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

pub struct SerialAdapter {
    port_name: String,
    baud_rate: u32,
}

impl SerialAdapter {
    pub fn new(port_name: String, baud_rate: u32) -> Self {
        Self {
            port_name,
            baud_rate,
        }
    }
}

#[async_trait]
impl VeynAdapter for SerialAdapter {
    fn name(&self) -> &str {
        "serial"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        let port_name = self.port_name.clone();
        let baud_rate = self.baud_rate;

        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_serial_loop(&port_name, baud_rate, tx) {
                warn!("serial adapter error on {}: {}", port_name, e);
            }
        })
        .await?;
        Ok(())
    }
}

fn run_serial_loop(port_name: &str, baud_rate: u32, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
    let port = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_millis(500))
        .open()
        .with_context(|| format!("open serial port {port_name}"))?;

    info!(port = %port_name, baud = baud_rate, "serial adapter connected");

    let device_id = format!("serial:{}", port_name.trim_start_matches('/').replace('/', "_"));
    let reader = BufReader::new(port);

    for line_result in reader.lines() {
        if tx.is_closed() {
            break;
        }
        let line = match line_result {
            Ok(l) => l.trim().to_string(),
            Err(e) => {
                debug!("serial: read error: {}", e);
                break;
            }
        };
        if line.is_empty() {
            continue;
        }
        if let Some((key, val_str)) = line.split_once('=') {
            if let Ok(value) = val_str.trim().parse::<f64>() {
                let event = VeynEvent::new(&device_id, "serial", key.trim(), value, "");
                if tx.blocking_send(event).is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}
