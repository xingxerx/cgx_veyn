use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::info;
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

/// Phase 2 — BLE universal wearable adapter (stub).
///
/// Full implementation will use `btleplug` to scan for GATT Heart Rate
/// Profile devices, subscribe to the HR Measurement characteristic, and
/// decode the standard BLE HR value packet.
pub struct BleAdapter;

#[async_trait]
impl VeynAdapter for BleAdapter {
    fn name(&self) -> &str {
        "ble"
    }

    async fn start(&self, _tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        info!("BLE adapter initialised (Phase 2 stub — passive, no scanning yet)");
        // Phase 2: initialise btleplug Central, scan, connect, notify
        std::future::pending::<()>().await;
        Ok(())
    }
}
