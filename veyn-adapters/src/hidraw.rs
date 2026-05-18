//! Linux hidraw adapter — reads raw USB HID reports from /dev/hidraw*.
//! Emits: source="hidraw", metric="hid_report", value=report_len, with
//! the hex-encoded first 8 bytes in `meta.report_hex`.

use std::fs;
use std::io::Read as _;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

#[derive(Default)]
pub struct HidrawAdapter;

impl HidrawAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VeynAdapter for HidrawAdapter {
    fn name(&self) -> &str {
        "hidraw"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        let paths = discover_hidraw_devices();
        if paths.is_empty() {
            warn!("hidraw: no /dev/hidraw* devices found; adapter idle");
            std::future::pending::<()>().await;
            return Ok(());
        }

        info!(count = paths.len(), "hidraw adapter: opening HID devices");

        let mut handles = Vec::new();
        for path in paths {
            let tx = tx.clone();
            let path_str = path.to_string_lossy().to_string();
            let handle = tokio::task::spawn_blocking(move || {
                read_hidraw_device(path_str, tx);
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.await;
        }
        Ok(())
    }
}

fn discover_hidraw_devices() -> Vec<PathBuf> {
    fs::read_dir("/dev")
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| n.starts_with("hidraw"))
                        .unwrap_or(false)
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

fn read_hidraw_device(path: String, tx: mpsc::Sender<VeynEvent>) {
    let mut file = match fs::OpenOptions::new().read(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            warn!("hidraw: cannot open {}: {}", path, e);
            return;
        }
    };

    let device_id = format!("hidraw:{}", path.replace("/dev/", ""));
    info!(path = %path, "hidraw device opened");

    let mut buf = [0u8; 64];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                debug!("hidraw: read error on {}: {}", path, e);
                break;
            }
        };

        let snippet: String = buf[..n.min(8)]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");

        let event = VeynEvent::new(&device_id, "hidraw", "hid_report", n as f64, "bytes")
            .with_meta("report_hex", serde_json::Value::String(snippet));

        if tx.blocking_send(event).is_err() {
            return;
        }
    }
}
