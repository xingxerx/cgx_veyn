use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use veyn_schemas::VeynEvent;

pub mod ble;
pub mod eeg;
pub mod fs_watcher;
pub mod healthkit;
pub mod midi;
pub mod mock;
pub mod mqtt;
pub mod serial_adapter;

#[cfg(target_os = "linux")]
pub mod evdev_adapter;
#[cfg(target_os = "linux")]
pub mod hidraw;

/// Every data source implements this trait.
#[async_trait]
pub trait VeynAdapter: Send + Sync {
    fn name(&self) -> &str;
    /// Start ingesting data and push events onto `tx` until the channel closes.
    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()>;
}
