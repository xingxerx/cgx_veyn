use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{error, info};
use veyn_schemas::VeynEvent;

use crate::api::state::AppState;

/// Reads events from the bus, logs them, appends to the JSONL audit log,
/// and forwards each event into the shared in-memory state.
pub async fn run(mut rx: mpsc::Receiver<VeynEvent>, state: AppState, jsonl_path: String) {
    let mut file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&jsonl_path)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            error!("cannot open JSONL log {}: {}", jsonl_path, e);
            return;
        }
    };

    info!("dispatcher started — writing audit log to {}", jsonl_path);

    while let Some(event) = rx.recv().await {
        info!(
            source = %event.source,
            device = %event.device_id,
            metric = %event.metric,
            value  = %event.value,
            unit   = %event.unit,
            "event"
        );

        if let Ok(mut line) = serde_json::to_string(&event) {
            line.push('\n');
            if let Err(e) = file.write_all(line.as_bytes()).await {
                error!("JSONL write error: {}", e);
            }
        }

        state.ingest(event);
    }
}
