use std::collections::HashMap;

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{error, info};
use veyn_schemas::{ContextSnapshot, StateDelta, VeynEvent};

use crate::api::state::AppState;
use crate::compression::CompressionEngine;

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

    let mut engine = CompressionEngine::new(
        state.config.rules_path.clone(),
        state.config.debounce_ms.clone(),
        state.config.epsilons.clone(),
    );

    info!("dispatcher started — JSONL log: {}", jsonl_path);

    while let Some(event) = rx.recv().await {
        state
            .raw_event_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if !engine.should_emit(&event) {
            continue;
        }

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

        state.ingest(event.clone());

        // Build metric state and deltas in a single lock scope to avoid contention.
        let (metric_state, deltas) = {
            let metrics = state.latest_metrics.lock().unwrap();
            let metric_state: HashMap<String, f64> = metrics
                .values()
                .map(|e| (e.metric.clone(), e.value))
                .collect();
            let deltas: Vec<StateDelta> = metrics
                .values()
                .map(|e| StateDelta {
                    device_id: e.device_id.clone(),
                    metric: e.metric.clone(),
                    value: e.value,
                    unit: e.unit.clone(),
                    ts: e.ts,
                    source_class: e.source.clone(),
                })
                .collect();
            (metric_state, deltas)
        };

        let (intent, intent_code, confidence) = engine.synthesize(&metric_state);

        let active_devices: Vec<String> = state.devices.lock().unwrap().keys().cloned().collect();

        let snapshot = ContextSnapshot {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            session_id: (*state.session_id).clone(),
            intent,
            intent_code,
            confidence,
            active_devices,
            state_deltas: deltas,
        };

        state.update_context(snapshot);

        *state.compression_ratio.lock().unwrap() = engine.compression_ratio();
    }
}
