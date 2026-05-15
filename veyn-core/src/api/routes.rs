use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;
use veyn_schemas::{VeynEvent, VeynNotification};

use super::state::AppState;

const DASHBOARD: &str = include_str!("dashboard.html");

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/",                  get(dashboard))
        .route("/health",            get(health))
        .route("/events/recent",     get(events_recent))
        .route("/metrics/:metric",   get(metrics_get))
        .route("/devices",           get(devices_list))
        .route("/plugins",           get(plugins_list))
        .route("/stream",            get(ws_stream))
        // Phase 5
        .route("/notify",            post(notify_post))
        .route("/presence",          get(presence_get))
        .route("/gestures/recent",   get(gestures_recent))
        .with_state(state)
}

// GET /
async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD)
}

// GET /health
async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status":         "ok",
        "version":        env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.start_time.elapsed().as_secs(),
        "events_total":   state.event_count.load(Ordering::Relaxed),
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

// GET /plugins
async fn plugins_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let plugins = state.plugins.lock().unwrap().clone();
    let count = plugins.len();
    Json(json!({ "plugins": plugins, "count": count }))
}

// GET /stream  (WebSocket upgrade)
async fn ws_stream(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    // Subscribe to the live broadcast before snapshotting history so no
    // events are lost between the two operations.
    let mut rx = state.broadcast_tx.subscribe();

    // Replay the recent-event ring buffer to the newly connected client.
    let replay: Vec<VeynEvent> = state
        .recent_events
        .lock()
        .unwrap()
        .iter()
        .cloned()
        .collect();

    for event in &replay {
        let Ok(json) = serde_json::to_string(event) else {
            continue;
        };
        if socket.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    // Stream live events, interleaved with client ping/close handling.
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        let json = match serde_json::to_string(&event) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
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
                match msg {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}

// ── Phase 5 routes ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NotifyRequest {
    title: String,
    body: String,
    target_device: Option<String>,
}

// POST /notify
async fn notify_post(
    State(state): State<AppState>,
    Json(req): Json<NotifyRequest>,
) -> impl IntoResponse {
    let mut notif = VeynNotification::new(req.title, req.body);
    if let Some(dev) = req.target_device {
        notif = notif.for_device(dev);
    }
    let id = notif.id.clone();

    // send() errors only when there are zero subscribers — that's fine.
    let _ = state.notification_tx.send(notif);

    (StatusCode::ACCEPTED, Json(json!({ "id": id, "status": "queued" })))
}

// GET /presence
async fn presence_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let presence: Vec<_> = state.presence.lock().unwrap().values().cloned().collect();
    let count = presence.len();
    Json(json!({ "presence": presence, "count": count }))
}

// GET /gestures/recent  — returns recent VeynEvents with source == "companion"
// and a metric that starts with "gesture_".
async fn gestures_recent(State(state): State<AppState>) -> Json<serde_json::Value> {
    let gestures: Vec<VeynEvent> = state
        .recent_events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.source == "companion" && e.metric.starts_with("gesture_"))
        .cloned()
        .collect();
    let count = gestures.len();
    Json(json!({ "gestures": gestures, "count": count }))
}
