use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

/// Accepts newline-delimited JSON `VeynEvent` frames from the iOS companion app.
///
/// The companion app connects via a plain TCP socket on the local network and
/// streams events as it receives them from HealthKit background delivery.
pub struct HealthKitAdapter {
    port: u16,
}

impl HealthKitAdapter {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

#[async_trait]
impl VeynAdapter for HealthKitAdapter {
    fn name(&self) -> &str {
        "healthkit"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("healthkit adapter listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    info!("iOS companion connected from {}", peer);
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stream);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            match serde_json::from_str::<VeynEvent>(&line) {
                                Ok(event) => {
                                    if tx.send(event).await.is_err() {
                                        return;
                                    }
                                }
                                Err(e) => warn!("healthkit parse error: {}", e),
                            }
                        }
                        info!("iOS companion disconnected from {}", peer);
                    });
                }
                Err(e) => error!("healthkit accept error: {}", e),
            }
        }
    }
}
