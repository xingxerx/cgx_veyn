use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};
use veyn_schemas::{VeynEvent, VeynNotification};

use crate::VeynAdapter;

/// Accepts newline-delimited JSON `VeynEvent` frames from the iOS companion app
/// and writes `VeynNotification` frames back to the same socket so the companion
/// can forward them to the Apple Watch.
pub struct HealthKitAdapter {
    port: u16,
    /// Subscribe to receive outbound notifications routed to the companion.
    notification_tx: broadcast::Sender<VeynNotification>,
}

impl HealthKitAdapter {
    pub fn new(port: u16, notification_tx: broadcast::Sender<VeynNotification>) -> Self {
        Self {
            port,
            notification_tx,
        }
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
                    let notif_rx = self.notification_tx.subscribe();
                    tokio::spawn(handle_companion(stream, peer.to_string(), tx, notif_rx));
                }
                Err(e) => tracing::error!("healthkit accept error: {}", e),
            }
        }
    }
}

async fn handle_companion(
    stream: tokio::net::TcpStream,
    peer: String,
    tx: mpsc::Sender<VeynEvent>,
    mut notif_rx: broadcast::Receiver<VeynNotification>,
) {
    let (read_half, mut write_half) = stream.into_split();
    let reader = BufReader::new(read_half);

    // Track which device IDs have been seen on this connection so we can
    // apply target_device filtering for outbound notifications.
    let seen: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let seen_write = seen.clone();
    let peer_write = peer.clone();

    // Outbound task: relay notifications to the companion socket.
    let write_task = tokio::spawn(async move {
        loop {
            match notif_rx.recv().await {
                Ok(notif) => {
                    let should_send = match &notif.target_device {
                        None => true,
                        Some(target) => seen_write.lock().unwrap().contains(target),
                    };
                    if !should_send {
                        continue;
                    }
                    match serde_json::to_string(&notif) {
                        Ok(mut line) => {
                            line.push('\n');
                            if write_half.write_all(line.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => warn!("notification serialize error: {}", e),
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(peer = %peer_write, "notification receiver lagged {} messages", n);
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Inbound loop: parse VeynEvents (health metrics + gesture events) from the companion.
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        match serde_json::from_str::<VeynEvent>(&line) {
            Ok(event) => {
                seen.lock().unwrap().insert(event.device_id.clone());
                if tx.send(event).await.is_err() {
                    break;
                }
            }
            Err(e) => warn!("healthkit parse error: {}", e),
        }
    }

    info!("iOS companion disconnected from {}", peer);
    write_task.abort();
}
