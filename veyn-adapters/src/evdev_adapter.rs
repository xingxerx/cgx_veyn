//! Linux evdev adapter — reads keyboard, mouse, and gamepad events from /dev/input/event*.
//! Each key press emits a `key_event` metric (1.0 = press, 0.0 = release).
//! Each relative axis movement emits `rel_x` / `rel_y` / `rel_wheel` metrics.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

pub struct EvdevAdapter;

impl EvdevAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VeynAdapter for EvdevAdapter {
    fn name(&self) -> &str {
        "evdev"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        let paths = discover_event_devices();
        if paths.is_empty() {
            warn!("evdev: no /dev/input/event* devices found; adapter idle");
            std::future::pending::<()>().await;
            return Ok(());
        }

        info!(count = paths.len(), "evdev adapter: opening input devices");

        // Spawn a blocking reader thread per device.
        let mut handles = Vec::new();
        for path in paths {
            let tx = tx.clone();
            let path_str = path.to_string_lossy().to_string();
            let handle = tokio::task::spawn_blocking(move || {
                read_evdev_device(path_str, tx);
            });
            handles.push(handle);
        }

        // Wait for all readers (they only stop when the channel closes).
        for h in handles {
            let _ = h.await;
        }
        Ok(())
    }
}

fn discover_event_devices() -> Vec<PathBuf> {
    fs::read_dir("/dev/input")
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| n.starts_with("event"))
                        .unwrap_or(false)
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

fn read_evdev_device(path: String, tx: mpsc::Sender<VeynEvent>) {
    let mut device = match evdev::Device::open(&path) {
        Ok(d) => d,
        Err(e) => {
            warn!("evdev: cannot open {}: {}", path, e);
            return;
        }
    };

    let device_name = device
        .name()
        .unwrap_or("unknown")
        .to_string();
    let device_id = format!("evdev:{}", path.replace("/dev/input/", ""));

    info!(path = %path, name = %device_name, "evdev device opened");

    loop {
        let events = match device.fetch_events() {
            Ok(e) => e,
            Err(e) => {
                debug!("evdev: read error on {}: {}", path, e);
                break;
            }
        };

        for event in events {
            let veyn_event = match event.kind() {
                evdev::InputEventKind::Key(key) => {
                    let value = event.value() as f64; // 1=press, 0=release, 2=repeat
                    if value > 1.0 {
                        continue; // skip repeat events
                    }
                    Some(
                        VeynEvent::new(&device_id, "evdev", "key_event", value, "")
                            .with_meta(
                                "key",
                                serde_json::Value::String(format!("{key:?}")),
                            )
                            .with_meta(
                                "device_name",
                                serde_json::Value::String(device_name.clone()),
                            ),
                    )
                }
                evdev::InputEventKind::RelAxis(axis) => {
                    let metric = match axis {
                        evdev::RelativeAxisType::REL_X => "rel_x",
                        evdev::RelativeAxisType::REL_Y => "rel_y",
                        evdev::RelativeAxisType::REL_WHEEL => "rel_wheel",
                        _ => continue,
                    };
                    Some(VeynEvent::new(
                        &device_id,
                        "evdev",
                        metric,
                        event.value() as f64,
                        "px",
                    ))
                }
                _ => None,
            };

            if let Some(ev) = veyn_event {
                if tx.blocking_send(ev).is_err() {
                    return; // channel closed
                }
            }
        }
    }
}
