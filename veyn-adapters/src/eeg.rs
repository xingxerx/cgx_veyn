use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::info;
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

/// Phase 2 — EEG / OSC adapter (stub).
///
/// Full implementation will bind a UDP socket and parse OSC bundles from
/// EEG headsets broadcasting Delta, Theta, Alpha, and Beta band power values.
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
        "eeg"
    }

    async fn start(&self, _tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        info!(
            "EEG/OSC adapter initialised on UDP :{} (Phase 2 stub)",
            self.osc_port
        );
        // Phase 2: bind UDP socket, parse OSC messages, emit VeynEvents
        std::future::pending::<()>().await;
        Ok(())
    }
}
