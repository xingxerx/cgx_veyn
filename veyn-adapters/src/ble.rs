use anyhow::{Context, Result};
use async_trait::async_trait;
use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::Manager;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::uuid;
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

// Standard Bluetooth SIG GATT UUIDs (16-bit short UUIDs expanded to 128-bit)
const HR_SERVICE: uuid::Uuid = uuid!("0000180d-0000-1000-8000-00805f9b34fb");
const HR_CHAR: uuid::Uuid = uuid!("00002a37-0000-1000-8000-00805f9b34fb");
const BATTERY_CHAR: uuid::Uuid = uuid!("00002a19-0000-1000-8000-00805f9b34fb");

const KNOWN_DEVICES_FILE: &str = "veyn-ble-devices.json";

/// Persisted list of previously connected device IDs for automatic reconnection.
#[derive(Serialize, Deserialize, Default)]
struct KnownDevices {
    ids: Vec<String>,
}

impl KnownDevices {
    fn load() -> Self {
        std::fs::read_to_string(KNOWN_DEVICES_FILE)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(KNOWN_DEVICES_FILE, data);
        }
    }

    fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|s| s == id)
    }

    fn add(&mut self, id: &str) {
        if !self.contains(id) {
            self.ids.push(id.to_string());
            self.save();
        }
    }
}

pub struct BleAdapter;

#[async_trait]
impl VeynAdapter for BleAdapter {
    fn name(&self) -> &str {
        "ble"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        info!("BLE adapter starting — scanning for Heart Rate Profile devices");

        let manager = Manager::new()
            .await
            .context("failed to initialise BLE manager")?;

        let adapters = manager
            .adapters()
            .await
            .context("failed to list BLE adapters")?;

        let central = match adapters.into_iter().next() {
            Some(a) => a,
            None => {
                warn!("no BLE adapter found; BLE will remain inactive");
                std::future::pending::<()>().await;
                return Ok(());
            }
        };

        let mut events = central
            .events()
            .await
            .context("failed to subscribe to BLE central events")?;

        central
            .start_scan(ScanFilter {
                services: vec![HR_SERVICE],
            })
            .await
            .context("failed to start BLE scan")?;

        info!("BLE scan started (Heart Rate Service filter active)");

        // Track which peripheral IDs already have an active connection task
        let mut connected: HashSet<String> = HashSet::new();

        while let Some(event) = events.next().await {
            match event {
                CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) => {
                    let id_str = id.to_string();
                    if connected.contains(&id_str) {
                        continue;
                    }

                    let peripheral = match central.peripheral(&id).await {
                        Ok(p) => p,
                        Err(e) => {
                            warn!(id = %id_str, "failed to get peripheral: {}", e);
                            continue;
                        }
                    };

                    let props = match peripheral.properties().await {
                        Ok(Some(p)) => p,
                        _ => continue,
                    };

                    let known = KnownDevices::load();
                    let has_hr = props.services.contains(&HR_SERVICE);
                    if !has_hr && !known.contains(&id_str) {
                        continue;
                    }

                    let device_name = props.local_name.unwrap_or_else(|| id_str.clone());
                    info!(device = %device_name, id = %id_str, "connecting to BLE device");
                    connected.insert(id_str);

                    let tx = tx.clone();
                    let name = device_name;
                    tokio::spawn(async move {
                        if let Err(e) = handle_peripheral(peripheral, name.clone(), tx).await {
                            warn!(device = %name, "BLE peripheral error: {}", e);
                        }
                    });
                }

                CentralEvent::DeviceDisconnected(id) => {
                    let id_str = id.to_string();
                    connected.remove(&id_str);
                    info!(id = %id_str, "BLE device disconnected");
                }

                _ => {}
            }
        }

        Ok(())
    }
}

async fn handle_peripheral<P>(
    peripheral: P,
    name: String,
    tx: mpsc::Sender<VeynEvent>,
) -> Result<()>
where
    P: btleplug::api::Peripheral,
{
    peripheral.connect().await.context("connect failed")?;
    peripheral
        .discover_services()
        .await
        .context("service discovery failed")?;

    let device_id = peripheral.id().to_string();

    // Persist so we reconnect on next daemon start
    KnownDevices::load().add(&device_id);

    info!(device = %name, "connected; services discovered");

    let chars = peripheral.characteristics();
    let hr_char = chars.iter().find(|c| c.uuid == HR_CHAR).cloned();
    let bat_char = chars.iter().find(|c| c.uuid == BATTERY_CHAR).cloned();

    if let Some(hr) = &hr_char {
        peripheral
            .subscribe(hr)
            .await
            .context("failed to subscribe to Heart Rate notifications")?;
        info!(device = %name, "subscribed to Heart Rate notifications");
    } else {
        warn!(device = %name, "Heart Rate characteristic not found");
    }

    // Read battery level once at connect time
    if let Some(bat) = &bat_char {
        if let Ok(data) = peripheral.read(bat).await {
            if let Some(&level) = data.first() {
                let event = VeynEvent::new(&device_id, "ble", "battery", f64::from(level), "%")
                    .with_meta("device_name", serde_json::Value::String(name.clone()));
                let _ = tx.send(event).await;
                debug!(device = %name, battery = level, "battery level read");
            }
        }
    }

    // Stream HR (and any other subscribed) notifications
    let mut notifications = peripheral.notifications().await?;
    while let Some(n) = notifications.next().await {
        if n.uuid == HR_CHAR {
            if let Some(bpm) = decode_hr_measurement(&n.value) {
                let event = VeynEvent::new(&device_id, "ble", "heart_rate", f64::from(bpm), "bpm")
                    .with_meta("device_name", serde_json::Value::String(name.clone()));
                if tx.send(event).await.is_err() {
                    break;
                }
                debug!(device = %name, bpm, "heart rate notification");
            }
        }
    }

    Ok(())
}

/// Decode a BLE Heart Rate Measurement characteristic (GATT 0x2A37).
///
/// Byte 0 flags:
///   bit 0 — HR value format: 0 = UINT8, 1 = UINT16 LE
///   bit 4 — RR-interval present (ignored here)
fn decode_hr_measurement(data: &[u8]) -> Option<u16> {
    let flags = *data.first()?;
    if flags & 0x01 != 0 {
        let lo = *data.get(1)? as u16;
        let hi = *data.get(2)? as u16;
        Some(lo | (hi << 8))
    } else {
        Some(*data.get(1)? as u16)
    }
}
