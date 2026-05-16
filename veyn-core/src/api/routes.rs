use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
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
use tokio::time::{interval, Duration};
use tracing::warn;
use veyn_schemas::{ContextSnapshot, StateDelta, VeynEvent, VeynNotification};

use super::state::AppState;

const DASHBOARD: &str = include_str!("dashboard.html");

pub fn router(state: AppState) -> Router {
    // Legacy unversioned routes kept for backward compat.
    let legacy = Router::new()
        .route("/", get(dashboard))
        .route("/health", get(health))
        .route("/events/recent", get(events_recent))
        .route("/metrics/:metric", get(metrics_get))
        .route("/devices", get(devices_list))
        .route("/plugins", get(plugins_list))
        .route("/stream", get(ws_stream))
        .route("/notify", post(notify_post))
        .route("/presence", get(presence_get))
        .route("/gestures/recent", get(gestures_recent));

    // Versioned /v1/ routes.
    let v1 = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/events/recent", get(events_recent))
        .route("/v1/metrics/:metric", get(metrics_get))
        .route("/v1/devices", get(devices_list))
        .route("/v1/plugins", get(plugins_list))
        .route("/v1/stream", get(ws_stream))
        .route("/v1/notify", post(notify_post))
        .route("/v1/presence", get(presence_get))
        .route("/v1/gestures/recent", get(gestures_recent))
        .route("/v1/context/current", get(context_current))
        .route("/v1/context/history", get(context_history));

    legacy.merge(v1).with_state(state)
}

// ── Dashboard ──────────────────────────────────────────────────────────────────

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD)
}

// ── GET /health ────────────────────────────────────────────────────────────────

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let uptime = state.start_time.elapsed().as_secs();
    let raw = state.raw_event_count.load(Ordering::Relaxed);
    let filtered = state.event_count.load(Ordering::Relaxed);
    let ratio = *state.compression_ratio.lock().unwrap();
    let connected_devices = state.devices.lock().unwrap().len();
    let event_rate_hz = filtered.checked_div(uptime).unwrap_or(0);

    Json(json!({
        "status":            "ok",
        "version":           env!("CARGO_PKG_VERSION"),
        "uptime_s":          uptime,
        "session_id":        *state.session_id,
        "events_total":      filtered,
        "events_raw":        raw,
        "event_rate_hz":     event_rate_hz,
        "compression_ratio": ratio,
        "connected_devices": connected_devices,
    }))
}

// ── GET /events/recent ────────────────────────────────────────────────────────

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

// ── GET /metrics/:metric ──────────────────────────────────────────────────────

async fn metrics_get(
    State(state): State<AppState>,
    Path(metric): Path<String>,
) -> impl IntoResponse {
    let found = state.latest_metrics.lock().unwrap().get(&metric).cloned();

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

// ── GET /devices ──────────────────────────────────────────────────────────────

async fn devices_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let devices: Vec<_> = state.devices.lock().unwrap().values().cloned().collect();
    let count = devices.len();
    Json(json!({ "devices": devices, "count": count }))
}

// ── GET /plugins ──────────────────────────────────────────────────────────────

async fn plugins_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let plugins = state.plugins.lock().unwrap().clone();
    let count = plugins.len();
    Json(json!({ "plugins": plugins, "count": count }))
}

// ── GET /v1/context/current ───────────────────────────────────────────────────

async fn context_current(State(state): State<AppState>) -> impl IntoResponse {
    match state.latest_context.lock().unwrap().clone() {
        Some(snap) => (StatusCode::OK, Json(serde_json::to_value(snap).unwrap())).into_response(),
        None => {
            // Build a snapshot on the fly from latest metrics.
            let snap = build_snapshot_from_metrics(&state);
            (StatusCode::OK, Json(serde_json::to_value(snap).unwrap())).into_response()
        }
    }
}

// ── GET /v1/context/history?n=10 ─────────────────────────────────────────────

#[derive(Deserialize)]
struct HistoryParams {
    #[serde(default = "default_n")]
    n: usize,
}

fn default_n() -> usize {
    10
}

async fn context_history(
    State(state): State<AppState>,
    Query(params): Query<HistoryParams>,
) -> Json<serde_json::Value> {
    let history: Vec<ContextSnapshot> = state
        .context_history
        .lock()
        .unwrap()
        .iter()
        .rev()
        .take(params.n)
        .cloned()
        .collect();
    let count = history.len();
    Json(json!({ "history": history, "count": count }))
}

fn build_snapshot_from_metrics(state: &AppState) -> ContextSnapshot {
    let now = chrono::Utc::now().timestamp_millis();
    let metrics = state.latest_metrics.lock().unwrap();
    let devices: Vec<String> = state.devices.lock().unwrap().keys().cloned().collect();

    let state_map: std::collections::HashMap<String, f64> = metrics
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
        })
        .collect();

    // Simple built-in intent rules used when no external rules.toml is present.
    let (intent, confidence) = synthesize_intent_builtin(&state_map);

    ContextSnapshot {
        timestamp_ms: now,
        session_id: (*state.session_id).clone(),
        intent,
        confidence,
        active_devices: devices,
        state_deltas: deltas,
    }
}

fn synthesize_intent_builtin(state: &std::collections::HashMap<String, f64>) -> (String, f64) {
    if let Some(&hr) = state.get("heart_rate") {
        if hr > 100.0 {
            return ("user under physical stress".to_string(), 0.8);
        }
        if hr < 60.0 {
            if state.get("hrv").is_some_and(|&h| h > 50.0) {
                return ("user in calm/resting state".to_string(), 0.85);
            }
            return ("user in low-activity state".to_string(), 0.7);
        }
        return ("user in normal activity state".to_string(), 0.75);
    }
    ("observing".to_string(), 0.5)
}

// ── GET /stream  (WebSocket) ──────────────────────────────────────────────────

async fn ws_stream(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.broadcast_tx.subscribe();

    // Replay ring buffer to newly connected client.
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
        if socket.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    // Keepalive ping every 30 s.
    let mut ping_ticker = interval(Duration::from_secs(30));
    ping_ticker.tick().await; // consume the immediate first tick

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
                    Err(RecvError::Lagged(n)) => warn!("WebSocket subscriber lagged {} events", n),
                    Err(RecvError::Closed) => break,
                }
            }
            _ = ping_ticker.tick() => {
                if socket.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}

// ── Phase 5 routes ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NotifyRequest {
    title: String,
    body: String,
    target_device: Option<String>,
}

async fn notify_post(
    State(state): State<AppState>,
    Json(req): Json<NotifyRequest>,
) -> impl IntoResponse {
    let mut notif = VeynNotification::new(req.title, req.body);
    if let Some(dev) = req.target_device {
        notif = notif.for_device(dev);
    }
    let id = notif.id.clone();
    let _ = state.notification_tx.send(notif);
    (
        StatusCode::ACCEPTED,
        Json(json!({ "id": id, "status": "queued" })),
    )
}

async fn presence_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let presence: Vec<_> = state.presence.lock().unwrap().values().cloned().collect();
    let count = presence.len();
    Json(json!({ "presence": presence, "count": count }))
}

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
