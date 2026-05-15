use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::json;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;
use veyn_schemas::VeynEvent;

use super::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health",           get(health))
        .route("/events/recent",    get(events_recent))
        .route("/metrics/:metric",  get(metrics_get))
        .route("/devices",          get(devices_list))
        .route("/stream",           get(ws_stream))
        .with_state(state)
}

// GET /health
async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status":          "ok",
        "version":         env!("CARGO_PKG_VERSION"),
        "uptime_seconds":  state.start_time.elapsed().as_secs(),
        "events_total":    state.event_count.load(Ordering::Relaxed),
    }))
}

// GET /events/recent
async fn events_recent(State(state): State<AppState>) -> Json<serde_json::Value> {
    let events: Vec<VeynEvent> = state
        .recent_events
        .lock()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let count = events.len();
    Json(json!({ "events": events, "count": count }))
}

// GET /metrics/:metric
async fn metrics_get(
    State(state): State<AppState>,
    Path(metric): Path<String>,
) -> impl IntoResponse {
    let found = state
        .latest_metrics
        .lock()
        .unwrap()
        .get(&metric)
        .cloned();

    match found {
        Some(e) => (
            StatusCode::OK,
            Json(json!({
                "metric":    e.metric,
                "value":     e.value,
                "unit":      e.unit,
                "ts":        e.ts,
                "device_id": e.device_id,
                "source":    e.source,
            })),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "metric not found", "metric": metric })),
        )
            .into_response(),
    }
}

// GET /devices
async fn devices_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let devices: Vec<_> = state.devices.lock().unwrap().values().cloned().collect();
    let count = devices.len();
    Json(json!({ "devices": devices, "count": count }))
}

// GET /stream  (WebSocket upgrade)
async fn ws_stream(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.broadcast_tx.subscribe();

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        let json = match serde_json::to_string(&event) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        warn!("WebSocket subscriber lagged {} events", n);
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                // Accept ping/pong silently; any error or close terminates the loop
                match msg {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}
