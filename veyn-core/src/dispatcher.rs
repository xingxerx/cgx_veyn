use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use veyn_schemas::{ContextSnapshot, StateDelta, VeynEvent};

use crate::api::state::AppState;
use crate::compression::CompressionEngine;

/// Flush baseline samples to SQLite every 15 minutes.
const BASELINE_PERSIST_INTERVAL: Duration = Duration::from_secs(15 * 60);

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

    let mut last_baseline_persist = Instant::now();

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

        // Update baseline and temporal windows with this sample.
        {
            let mut baseline = state.baseline_engine.lock().unwrap_or_else(|e| {
                warn!("lock poisoned: {}", e);
                e.into_inner()
            });
            baseline.update(&event.device_id, &event.metric, event.value);
        }
        {
            let mut temporal = state.temporal_engine.lock().unwrap_or_else(|e| {
                warn!("lock poisoned: {}", e);
                e.into_inner()
            });
            temporal.push(&event.metric, event.ts, event.value);
        }

        // Persist event to SQLite if a session is open.
        let recording_session_id = {
            let sm = state.session_manager.lock().unwrap_or_else(|e| {
                warn!("lock poisoned: {}", e);
                e.into_inner()
            });
            sm.current_id()
        };

        if let Some(ref sid) = recording_session_id {
            if let Some(ref db_arc) = state.db {
                let conn = db_arc.lock().unwrap_or_else(|e| {
                    warn!("lock poisoned: {}", e);
                    e.into_inner()
                });
                if let Err(e) = crate::storage::insert_event(&conn, &event, Some(sid)) {
                    warn!("failed to persist event to SQLite: {}", e);
                }
                // Also record baseline sample.
                if let Err(e) = crate::storage::insert_baseline_sample(
                    &conn,
                    &event.device_id,
                    &event.metric,
                    event.ts,
                    event.value,
                ) {
                    warn!("failed to persist baseline sample: {}", e);
                }
            }
        }

        state.ingest(event.clone());

        // Build metric state and deltas + compute z-scores in a single lock scope.
        let (metric_state, deltas, z_scores) = {
            let metrics = state.latest_metrics.lock().unwrap_or_else(|e| {
                warn!("lock poisoned: {}", e);
                e.into_inner()
            });
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
            drop(metrics);

            let z_scores = state
                .baseline_engine
                .lock()
                .unwrap_or_else(|e| {
                    warn!("lock poisoned: {}", e);
                    e.into_inner()
                })
                .z_scores(&metric_state);
            (metric_state, deltas, z_scores)
        };

        let (intent, intent_code, intent_confidence) = engine.synthesize(&metric_state, &z_scores);

        let active_devices: Vec<String> = state
            .devices
            .lock()
            .unwrap_or_else(|e| {
                warn!("lock poisoned: {}", e);
                e.into_inner()
            })
            .keys()
            .cloned()
            .collect();

        let baseline_delta = if z_scores.is_empty() {
            None
        } else {
            Some(z_scores)
        };

        let temporal_patterns = state
            .temporal_engine
            .lock()
            .unwrap_or_else(|e| {
                warn!("lock poisoned: {}", e);
                e.into_inner()
            })
            .get_patterns();

        let snapshot = ContextSnapshot {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            session_id: (*state.session_id).clone(),
            intent,
            intent_code,
            confidence: intent_confidence as f64,
            intent_confidence,
            active_devices,
            state_deltas: deltas,
            baseline_delta,
            recording_session_id: recording_session_id.map(|a| (*a).clone()),
            temporal_patterns,
        };

        state.update_context(snapshot);

        *state.compression_ratio.lock().unwrap_or_else(|e| {
            warn!("lock poisoned: {}", e);
            e.into_inner()
        }) = engine.compression_ratio();

        // Periodically flush baseline samples to SQLite.
        if last_baseline_persist.elapsed() >= BASELINE_PERSIST_INTERVAL {
            last_baseline_persist = Instant::now();
            if let Some(ref db_arc) = state.db {
                // Snapshot current metrics for persisting to baseline table.
                let metrics_snap: Vec<(String, String, i64, f64)> = {
                    let metrics = state.latest_metrics.lock().unwrap_or_else(|e| {
                        warn!("lock poisoned: {}", e);
                        e.into_inner()
                    });
                    metrics
                        .values()
                        .map(|e| (e.device_id.clone(), e.metric.clone(), e.ts, e.value))
                        .collect()
                };
                let conn = db_arc.lock().unwrap_or_else(|e| {
                    warn!("lock poisoned: {}", e);
                    e.into_inner()
                });
                for (dev, met, ts, val) in metrics_snap {
                    if let Err(e) =
                        crate::storage::insert_baseline_sample(&conn, &dev, &met, ts, val)
                    {
                        warn!("baseline flush error: {}", e);
                    }
                }
                info!("baseline samples flushed to SQLite");
            }
        }
    }
}
