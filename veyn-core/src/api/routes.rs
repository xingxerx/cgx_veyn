use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Json,
    },
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{interval, Duration};
use tokio_stream::{wrappers::BroadcastStream, StreamExt as _};
use tracing::warn;
use veyn_schemas::{ContextSnapshot, IntentCode, StateDelta, VeynEvent, VeynNotification};

use super::state::AppState;
use crate::auth::TokenClaim;

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
        .route("/v1/context/history", get(context_history))
        .route("/v1/context/subscribe", get(context_subscribe));

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

async fn context_current(
    State(state): State<AppState>,
    claim: Option<axum::Extension<TokenClaim>>,
) -> impl IntoResponse {
    let allowed_sources = claim.as_ref().and_then(|c| c.0.allowed_sources.clone());

    let snap = match state.latest_context.lock().unwrap().clone() {
        Some(s) => s,
        None => build_snapshot_from_metrics(&state),
    };

    let snap = apply_source_filter(snap, allowed_sources.as_deref());
    (StatusCode::OK, Json(serde_json::to_value(snap).unwrap())).into_response()
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
    claim: Option<axum::Extension<TokenClaim>>,
) -> Json<serde_json::Value> {
    let allowed_sources = claim.as_ref().and_then(|c| c.0.allowed_sources.clone());

    let history: Vec<ContextSnapshot> = state
        .context_history
        .lock()
        .unwrap()
        .iter()
        .rev()
        .take(params.n)
        .cloned()
        .map(|s| apply_source_filter(s, allowed_sources.as_deref()))
        .collect();
    let count = history.len();
    Json(json!({ "history": history, "count": count }))
}

// ── GET /v1/context/subscribe  (SSE) ─────────────────────────────────────────

/// Filter DSL query params accepted by the SSE subscribe endpoint.
#[derive(Deserialize, Clone)]
struct SubscribeParams {
    /// Comma-separated list of `intent_code` values to match
    /// (e.g. `?intents=resting,idle`).
    intents: Option<String>,
    /// Minimum confidence score [0.0–1.0].
    min_confidence: Option<f64>,
    /// Comma-separated list of source classes to include
    /// (e.g. `?source_class=ble,midi`).
    source_class: Option<String>,
}

async fn context_subscribe(
    State(state): State<AppState>,
    Query(params): Query<SubscribeParams>,
    claim: Option<axum::Extension<TokenClaim>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let allowed_sources_token = claim.as_ref().and_then(|c| c.0.allowed_sources.clone());

    let intents: Vec<String> = params
        .intents
        .as_deref()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default();

    let source_filter: Vec<String> = params
        .source_class
        .as_deref()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default();

    let min_conf = params.min_confidence.unwrap_or(0.0);

    let rx = state.context_broadcast_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let intents = intents.clone();
        let source_filter = source_filter.clone();
        let allowed_sources_token = allowed_sources_token.clone();

        match result {
            Ok(mut snapshot) => {
                // Apply confidence filter.
                if snapshot.confidence < min_conf {
                    return None;
                }

                // Apply intent code filter.
                if !intents.is_empty() {
                    let code_str = intent_code_to_str(&snapshot.intent_code);
                    if !intents.iter().any(|i| i == code_str) {
                        return None;
                    }
                }

                // Apply source class filter from query params.
                if !source_filter.is_empty() {
                    snapshot.state_deltas
                        .retain(|d| source_filter.iter().any(|s| s == &d.source_class));
                }

                // Apply source class filter from token scope.
                if let Some(ref allowed) = allowed_sources_token {
                    snapshot.state_deltas
                        .retain(|d| allowed.iter().any(|s| s == &d.source_class));
                }

                let json = serde_json::to_string(&snapshot).ok()?;
                Some(Ok(Event::default().event("context").data(json)))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                warn!("SSE subscriber lagged {} snapshots", n);
                None
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn intent_code_to_str(code: &IntentCode) -> &'static str {
    match code {
        IntentCode::Resting => "resting",
        IntentCode::Active => "active",
        IntentCode::Stressed => "stressed",
        IntentCode::Idle => "idle",
        IntentCode::Focus => "focus",
        IntentCode::Recovery => "recovery",
        IntentCode::HealthConcern => "health_concern",
        IntentCode::Observing => "observing",
    }
}

// ── GET /stream  (WebSocket) ──────────────────────────────────────────────────

async fn ws_stream(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Client-to-server message for runtime WebSocket subscription filtering.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Subscribe { filter: WsEventFilter },
    Unsubscribe,
}

#[derive(Deserialize, Clone, Default)]
struct WsEventFilter {
    /// Only forward events from these source classes.
    device_class: Option<Vec<String>>,
    /// Only forward events with these metric names.
    metrics: Option<Vec<String>>,
}

impl WsEventFilter {
    fn accepts(&self, event: &VeynEvent) -> bool {
        if let Some(classes) = &self.device_class {
            if !classes.iter().any(|c| c == &event.source) {
                return false;
            }
        }
        if let Some(metrics) = &self.metrics {
            if !metrics.iter().any(|m| m == &event.metric) {
                return false;
            }
        }
        true
    }
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.broadcast_tx.subscribe();
    let mut filter = WsEventFilter::default();

    // Replay ring buffer to newly connected client.
    let replay: Vec<VeynEvent> = state
        .recent_events
        .lock()
        .unwrap()
        .iter()
        .cloned()
        .collect();

    for event in &replay {
        if !filter.accepts(event) {
            continue;
        }
        let Ok(json) = serde_json::to_string(event) else {
            continue;
        };
        if socket.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    let mut ping_ticker = interval(Duration::from_secs(30));
    ping_ticker.tick().await;

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if !filter.accepts(&event) {
                            continue;
                        }
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
                    Some(Ok(Message::Text(text))) => {
                        // Accept client-sent subscribe filter messages.
                        if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                            match client_msg {
                                ClientMessage::Subscribe { filter: f } => {
                                    filter = f;
                                }
                                ClientMessage::Unsubscribe => {
                                    filter = WsEventFilter::default();
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn apply_source_filter(
    mut snapshot: ContextSnapshot,
    allowed: Option<&[String]>,
) -> ContextSnapshot {
    if let Some(sources) = allowed {
        snapshot
            .state_deltas
            .retain(|d| sources.iter().any(|s| s == &d.source_class));
    }
    snapshot
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
            source_class: e.source.clone(),
        })
        .collect();

    let (intent, intent_code, confidence) = synthesize_intent_builtin(&state_map);

    ContextSnapshot {
        timestamp_ms: now,
        session_id: (*state.session_id).clone(),
        intent,
        intent_code,
        confidence,
        active_devices: devices,
        state_deltas: deltas,
    }
}

fn synthesize_intent_builtin(
    state: &std::collections::HashMap<String, f64>,
) -> (String, IntentCode, f64) {
    if let Some(&hr) = state.get("heart_rate") {
        if hr > 100.0 {
            return (
                "user under physical stress".to_string(),
                IntentCode::Stressed,
                0.8,
            );
        }
        if hr < 60.0 {
            if state.get("hrv").is_some_and(|&h| h > 50.0) {
                return (
                    "user in calm/resting state".to_string(),
                    IntentCode::Resting,
                    0.85,
                );
            }
            return (
                "user in low-activity state".to_string(),
                IntentCode::Idle,
                0.7,
            );
        }
        return (
            "user in normal activity state".to_string(),
            IntentCode::Active,
            0.75,
        );
    }
    ("observing".to_string(), IntentCode::Observing, 0.5)
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
