use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Json,
    },
    routing::{get, patch, post},
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{interval, Duration};
use tokio_stream::{wrappers::BroadcastStream, StreamExt as _};
use tracing::warn;
use uuid::Uuid;
use veyn_schemas::{
    ContextSnapshot, IntentCode, MemoryKind, MemoryQuery, MemoryRecord, OutcomeRating, Session,
    StateDelta, VeynEvent, VeynNotification,
};

use super::state::{AppState, ClientInfo};
use crate::auth::TokenClaim;
use crate::config::ContextTier;

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
        .route("/gestures/recent", get(gestures_recent))
        .route("/metrics", get(prometheus_metrics))
        .route("/openapi.yaml", get(openapi_spec));

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
        .route("/v1/context/subscribe", get(context_subscribe))
        // Session routes (8.1 + 8.2)
        .route("/v1/session/start", post(session_start))
        .route("/v1/session/stop", post(session_stop))
        .route("/v1/sessions", get(sessions_list))
        .route("/v1/session/:id", get(session_get))
        .route("/v1/session/:id", patch(session_patch))
        .route("/v1/session/:id/replay", get(session_replay))
        .route("/v1/session/:id/export", get(session_export))
        // Baseline routes (8.3)
        .route("/v1/baseline/:device_id/:metric", get(baseline_get))
        .route(
            "/v1/baseline/:device_id/:metric/history",
            get(baseline_history),
        )
        .route("/v1/temporal/patterns", get(temporal_patterns))
        // Memory layer (Phase 9)
        .route("/v1/memory", post(memory_write))
        .route("/v1/memory", get(memory_query))
        .route("/v1/memory/:id/outcome", patch(memory_anchor_outcome))
        .route("/v1/memory/:id", get(memory_get))
        .route("/v1/memory/:id", axum::routing::delete(memory_delete))
        // Pattern detection (veyn-insight)
        .route("/v1/patterns", get(patterns_list))
        // 14.1 — multi-client tracking
        .route("/v1/clients", get(clients_list))
        // 14.3 — loopback-only auth-less broadcast (checked inside handler)
        .route("/v1/context/broadcast", get(context_broadcast))
        // 13.1 — inference params debug read
        .route("/v1/inference/params", get(inference_params_get))
        // 13.3 — coherence state debug read
        .route("/v1/coherence", get(coherence_get))
        // 15.3 — rules simulate
        .route("/v1/rules/simulate", post(rules_simulate))
        // 16.1 — batch export + session compare
        .route("/v1/export", get(export_all))
        .route("/v1/sessions/compare", get(sessions_compare))
        // 16.2 — baseline intelligence
        .route("/v1/baseline/summary", get(baseline_summary));

    legacy.merge(v1).with_state(state)
}

// ── Dashboard ──────────────────────────────────────────────────────────────────

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD)
}

// ── GET /health ────────────────────────────────────────────────────────────────

async fn health(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let uptime = state.start_time.elapsed().as_secs();
    let raw = state.raw_event_count.load(Ordering::Relaxed);
    let filtered = state.event_count.load(Ordering::Relaxed);
    let ratio = *state
        .compression_ratio
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let connected_devices = state
        .devices
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .len();
    let event_rate_hz = filtered.checked_div(uptime).unwrap_or(0);
    let recording_session_id = state
        .session_manager
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .current_id()
        .map(|a| (*a).clone());

    Ok(Json(json!({
        "status":               "ok",
        "version":              env!("CARGO_PKG_VERSION"),
        "uptime_s":             uptime,
        "session_id":           *state.session_id,
        "events_total":         filtered,
        "events_raw":           raw,
        "event_rate_hz":        event_rate_hz,
        "compression_ratio":    ratio,
        "connected_devices":    connected_devices,
        "recording_session_id": recording_session_id,
    })))
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
    /// (e.g. `?intents=neutral,recovery`).
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

    // 14.1 — Register SSE subscriber as a client.
    let client_id = Uuid::new_v4().to_string();
    {
        let now = chrono::Utc::now();
        let src_filter = if source_filter.is_empty() {
            None
        } else {
            Some(source_filter.clone())
        };
        let info = ClientInfo {
            client_id: client_id.clone(),
            connected_at: now.to_rfc3339(),
            connected_at_ms: now.timestamp_millis(),
            tier: "semantic".to_string(),
            transport: "sse".to_string(),
            source_filter: src_filter,
        };
        state
            .connected_clients
            .lock()
            .unwrap()
            .insert(client_id.clone(), info);
    }

    let rx = state.context_broadcast_tx.subscribe();
    let clients_ref = state.connected_clients.clone();
    let deregister_id = client_id.clone();

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
                    let code_str = intent_code_str(&snapshot.intent_code);
                    if !intents.iter().any(|i| i == code_str) {
                        return None;
                    }
                }

                // Apply source class filter from query params.
                if !source_filter.is_empty() {
                    snapshot
                        .state_deltas
                        .retain(|d| source_filter.iter().any(|s| s == &d.source_class));
                }

                // Apply source class filter from token scope.
                if let Some(ref allowed) = allowed_sources_token {
                    snapshot
                        .state_deltas
                        .retain(|d| allowed.iter().any(|s| s == &d.source_class));
                }

                // Choose event type: baseline_drift marker > context_degraded > context.
                let event_name = if matches!(&snapshot.intent_code, IntentCode::Other(s) if s == "baseline_drift") {
                    "baseline_drift"
                } else if snapshot.confidence < 0.4 {
                    "context_degraded"
                } else {
                    "context"
                };

                let json = serde_json::to_string(&snapshot).ok()?;
                Some(Ok(Event::default().event(event_name).data(json)))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                warn!("SSE subscriber lagged {} snapshots", n);
                None
            }
        }
    });

    // Wrap the stream so the client is deregistered when the SSE connection drops.
    let guarded_stream = SseDropGuard {
        inner: stream,
        clients: clients_ref,
        client_id: deregister_id,
    };

    Sse::new(guarded_stream).keep_alive(KeepAlive::default())
}

/// Wrapper stream that deregisters a client from `connected_clients` on drop.
struct SseDropGuard<S> {
    inner: S,
    clients: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, ClientInfo>>>,
    client_id: String,
}

impl<S> Drop for SseDropGuard<S> {
    fn drop(&mut self) {
        self.clients.lock().unwrap().remove(&self.client_id);
    }
}

impl<S> tokio_stream::Stream for SseDropGuard<S>
where
    S: tokio_stream::Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}


fn intent_code_str(code: &IntentCode) -> &str {
    match code {
        IntentCode::Neutral => "neutral",
        IntentCode::CognitiveLoad => "cognitive_load",
        IntentCode::StressResponse => "stress_response",
        IntentCode::Approach => "approach",
        IntentCode::Avoidance => "avoidance",
        IntentCode::Fatigue => "fatigue",
        IntentCode::Recovery => "recovery",
        IntentCode::Other(s) => s.as_str(),
    }
}

// ── GET /stream  (WebSocket) ──────────────────────────────────────────────────

async fn ws_stream(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    claim: Option<axum::Extension<TokenClaim>>,
) -> impl IntoResponse {
    let tier = effective_tier(&state, claim.as_ref().map(|c| &c.0));
    let client_id = Uuid::new_v4().to_string();
    ws.on_upgrade(move |socket| handle_socket(socket, state, tier, client_id))
}

/// Resolve the effective tier: token ceiling takes precedence; falls back to
/// the daemon-level default from config.
fn effective_tier(state: &AppState, claim: Option<&TokenClaim>) -> ContextTier {
    claim
        .and_then(|c| c.tier_ceiling.clone())
        .unwrap_or_else(|| state.config.context_tier.clone())
}

/// Client-to-server message for runtime WebSocket subscription filtering.
///
/// Send as JSON text frame after the WS handshake completes.
///
/// Raw/Filtered tier — filter raw `VeynEvent` objects:
/// ```json
/// { "type": "subscribe", "filter": { "device_class": ["ble"], "metrics": ["heart_rate"] } }
/// ```
///
/// Semantic tier — filter `ContextSnapshot` objects:
/// ```json
/// { "type": "subscribe", "context_filter": { "intents": ["stress_response"], "min_confidence": 0.7 } }
/// ```
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Subscribe {
        #[serde(default)]
        filter: WsEventFilter,
        #[serde(default)]
        context_filter: WsContextFilter,
    },
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

/// Runtime filter for the Semantic WebSocket tier — applied to each `ContextSnapshot`.
#[derive(Deserialize, Clone, Default)]
struct WsContextFilter {
    /// Only forward snapshots whose `intent_code` is in this list.
    intents: Option<Vec<String>>,
    /// Drop snapshots with `confidence` below this threshold.
    min_confidence: Option<f64>,
    /// Only retain `state_deltas` from these source classes.
    source_class: Option<Vec<String>>,
    /// When true, snapshots with intent_code == "neutral" are suppressed.
    #[serde(default)]
    exclude_neutral: bool,
}

impl WsContextFilter {
    fn accepts(&self, snap: &ContextSnapshot) -> bool {
        if let Some(min_conf) = self.min_confidence {
            if snap.confidence < min_conf {
                return false;
            }
        }
        if self.exclude_neutral && snap.intent_code == IntentCode::Neutral {
            return false;
        }
        if let Some(intents) = &self.intents {
            if !intents.is_empty() {
                let code_str = intent_code_str(&snap.intent_code);
                if !intents.iter().any(|i| i == code_str) {
                    return false;
                }
            }
        }
        true
    }

    fn filter_snapshot(&self, mut snap: ContextSnapshot) -> ContextSnapshot {
        if let Some(classes) = &self.source_class {
            snap.state_deltas
                .retain(|d| classes.iter().any(|s| s == &d.source_class));
        }
        snap
    }
}

async fn handle_socket(mut socket: WebSocket, state: AppState, tier: ContextTier, client_id: String) {
    let mut rx = state.broadcast_tx.subscribe();
    let mut cx_rx = state.context_broadcast_tx.subscribe();
    let mut filter = WsEventFilter::default();
    let mut context_filter = WsContextFilter::default();

    // 14.1 — Register this client.
    {
        let now = chrono::Utc::now();
        let tier_str = match &tier {
            ContextTier::Raw => "raw",
            ContextTier::Filtered => "filtered",
            ContextTier::Semantic => "semantic",
        };
        let info = ClientInfo {
            client_id: client_id.clone(),
            connected_at: now.to_rfc3339(),
            connected_at_ms: now.timestamp_millis(),
            tier: tier_str.to_string(),
            transport: "websocket".to_string(),
            source_filter: None,
        };
        state.connected_clients.lock().unwrap().insert(client_id.clone(), info);
    }

    // For Semantic tier, replay the latest context snapshot instead of raw events.
    if tier == ContextTier::Semantic {
        let maybe_snap = state.latest_context.lock().unwrap().clone();
        if let Some(snap) = maybe_snap {
            if let Ok(json) = serde_json::to_string(&snap) {
                let _ = socket.send(Message::Text(json)).await;
            }
        }
    } else {
        // Replay ring buffer to newly connected client (Raw / Filtered tiers).
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
                // 14.1 — Deregister on early disconnect.
                state.connected_clients.lock().unwrap().remove(&client_id);
                return;
            }
        }
    }

    let mut ping_ticker = interval(Duration::from_secs(30));
    ping_ticker.tick().await;

    loop {
        tokio::select! {
            // ── Raw / Filtered tier: stream VeynEvents (with optional session frame) ──
            result = rx.recv(), if tier != ContextTier::Semantic => {
                match result {
                    Ok(event) => {
                        if !filter.accepts(&event) {
                            continue;
                        }
                        // Session framing: wrap in envelope when a session is active.
                        let active_session = {
                            let sm = state.session_manager.lock().unwrap();
                            sm.current_id().map(|a| (*a).clone())
                        };

                        let json = if let Some(session_id) = active_session {
                            match serde_json::to_string(&json!({
                                "session_id": session_id,
                                "channel":    event.device_id,
                                "event":      event,
                            })) {
                                Ok(j) => j,
                                Err(_) => continue,
                            }
                        } else {
                            match serde_json::to_string(&event) {
                                Ok(j) => j,
                                Err(_) => continue,
                            }
                        };

                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => warn!("WebSocket subscriber lagged {} events", n),
                    Err(RecvError::Closed) => break,
                }
            }
            // ── Semantic tier: stream ContextSnapshots (with optional filter) ─────────
            result = cx_rx.recv(), if tier == ContextTier::Semantic => {
                match result {
                    Ok(snapshot) => {
                        if !context_filter.accepts(&snapshot) {
                            continue;
                        }
                        let snapshot = context_filter.filter_snapshot(snapshot);
                        let json = match serde_json::to_string(&snapshot) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => warn!("WS semantic subscriber lagged {} snapshots", n),
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
                        if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                            match client_msg {
                                ClientMessage::Subscribe { filter: f, context_filter: cf } => {
                                    filter = f;
                                    context_filter = cf;
                                }
                                ClientMessage::Unsubscribe => {
                                    filter = WsEventFilter::default();
                                    context_filter = WsContextFilter::default();
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

    // 14.1 — Deregister this client on disconnect.
    state.connected_clients.lock().unwrap().remove(&client_id);
}

// ── Session routes (8.1 + 8.2) ───────────────────────────────────────────────

#[derive(Deserialize)]
struct SessionStartRequest {
    label: String,
}

async fn session_start(
    State(state): State<AppState>,
    Json(req): Json<SessionStartRequest>,
) -> impl IntoResponse {
    let devices = state.devices.lock().unwrap().values().cloned().collect();
    let db_guard = state.db.as_ref().map(|d| d.lock().unwrap());
    let db_ref = db_guard.as_deref();

    let session = state
        .session_manager
        .lock()
        .unwrap()
        .open(req.label, devices, db_ref);

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(session).unwrap()),
    )
        .into_response()
}

async fn session_stop(State(state): State<AppState>) -> impl IntoResponse {
    let db_guard = state.db.as_ref().map(|d| d.lock().unwrap());
    let db_ref = db_guard.as_deref();

    match state.session_manager.lock().unwrap().close(db_ref) {
        Some(session) => {
            (StatusCode::OK, Json(serde_json::to_value(session).unwrap())).into_response()
        }
        None => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "no session is currently open" })),
        )
            .into_response(),
    }
}

async fn session_get(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    // Check in-memory first (current session).
    {
        let sm = state.session_manager.lock().unwrap();
        if let Some(ref s) = sm.current {
            if s.id == id {
                return (StatusCode::OK, Json(serde_json::to_value(s).unwrap())).into_response();
            }
        }
    }

    // Fall back to SQLite.
    match &state.db {
        Some(db) => match crate::storage::get_session(&db.lock().unwrap(), &id) {
            Ok(Some(session)) => {
                (StatusCode::OK, Json(serde_json::to_value(session).unwrap())).into_response()
            }
            Ok(None) => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "session not found" })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "session not found (persistence not available)" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct SessionPatchRequest {
    notes: Option<String>,
    label: Option<String>,
}

async fn session_patch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SessionPatchRequest>,
) -> impl IntoResponse {
    // Only the currently open session can be patched in-memory.
    {
        let db_guard = state.db.as_ref().map(|d| d.lock().unwrap());
        let db_ref = db_guard.as_deref();
        let mut sm = state.session_manager.lock().unwrap();
        if let Some(ref mut s) = sm.current {
            if s.id == id {
                if let Some(notes) = req.notes {
                    let _ = sm.annotate(notes, db_ref);
                }
                if let Some(label) = req.label {
                    sm.current.as_mut().unwrap().label = label;
                }
                let updated = sm.current.clone().unwrap();
                return (StatusCode::OK, Json(serde_json::to_value(updated).unwrap()))
                    .into_response();
            }
        }
    }

    // For closed sessions, patch via SQLite.
    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            match crate::storage::get_session(&conn, &id) {
                Ok(Some(mut session)) => {
                    if let Some(notes) = req.notes {
                        session.notes = Some(notes);
                    }
                    if let Some(label) = req.label {
                        session.label = label;
                    }
                    match crate::storage::update_session(&conn, &session) {
                        Ok(()) => (StatusCode::OK, Json(serde_json::to_value(session).unwrap()))
                            .into_response(),
                        Err(e) => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": e.to_string() })),
                        )
                            .into_response(),
                    }
                }
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "session not found" })),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "session not found (persistence not available)" })),
        )
            .into_response(),
    }
}

async fn session_replay(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            match crate::storage::replay_session_events(&conn, &id) {
                Ok(events) => {
                    let count = events.len();
                    (
                        StatusCode::OK,
                        Json(json!({ "events": events, "count": count })),
                    )
                        .into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct SessionsListParams {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_limit() -> usize {
    20
}

async fn sessions_list(
    State(state): State<AppState>,
    Query(params): Query<SessionsListParams>,
) -> impl IntoResponse {
    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            match crate::storage::list_sessions(&conn, params.limit, params.offset) {
                Ok(sessions) => {
                    let count = sessions.len();
                    (
                        StatusCode::OK,
                        Json(json!({ "sessions": sessions, "count": count })),
                    )
                        .into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => {
            // Return current session only if no DB.
            let current: Vec<Session> = state
                .session_manager
                .lock()
                .unwrap()
                .current
                .clone()
                .into_iter()
                .collect();
            let count = current.len();
            (
                StatusCode::OK,
                Json(json!({ "sessions": current, "count": count })),
            )
                .into_response()
        }
    }
}

// ── Baseline routes (8.3) ─────────────────────────────────────────────────────

async fn baseline_get(
    State(state): State<AppState>,
    Path((device_id, metric)): Path<(String, String)>,
) -> impl IntoResponse {
    match state
        .baseline_engine
        .lock()
        .unwrap()
        .get_stats(&device_id, &metric)
    {
        Some(stats) => (StatusCode::OK, Json(serde_json::to_value(stats).unwrap())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "insufficient baseline data",
                "device_id": device_id,
                "metric": metric,
                "hint": "at least 7 days of data required"
            })),
        )
            .into_response(),
    }
}

// ── GET /v1/session/:id/export?format=csv ────────────────────────────────────

#[derive(Deserialize)]
struct ExportParams {
    #[serde(default = "default_format")]
    format: String,
}

fn default_format() -> String {
    "csv".to_string()
}

async fn session_export(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ExportParams>,
) -> impl IntoResponse {
    if params.format != "csv" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "only format=csv is supported" })),
        )
            .into_response();
    }

    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            match crate::storage::export_session_csv(&conn, &id) {
                Ok(csv) => (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                        (
                            header::CONTENT_DISPOSITION,
                            &format!("attachment; filename=\"session-{id}.csv\""),
                        ),
                    ],
                    csv,
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── GET /v1/baseline/:device_id/:metric/history?days=30 ───────────────────────

#[derive(Deserialize)]
struct BaselineHistoryParams {
    #[serde(default = "default_days")]
    days: u32,
}

fn default_days() -> u32 {
    30
}

async fn baseline_history(
    State(state): State<AppState>,
    Path((device_id, metric)): Path<(String, String)>,
    Query(params): Query<BaselineHistoryParams>,
) -> impl IntoResponse {
    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            match crate::storage::load_baseline_daily_history(
                &conn,
                &device_id,
                &metric,
                params.days,
            ) {
                Ok(history) => {
                    let points: Vec<serde_json::Value> = history
                        .into_iter()
                        .map(|(ts, mean)| json!({ "ts": ts, "mean": mean }))
                        .collect();
                    (
                        StatusCode::OK,
                        Json(json!({
                            "device_id": device_id,
                            "metric": metric,
                            "days": params.days,
                            "history": points,
                        })),
                    )
                        .into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
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

    let recording_session_id = state
        .session_manager
        .lock()
        .unwrap()
        .current_id()
        .map(|a| (*a).clone());

    ContextSnapshot {
        timestamp_ms: now,
        session_id: (*state.session_id).clone(),
        intent: "neutral".to_string(),
        intent_code: IntentCode::Neutral,
        confidence: 0.5,
        intent_confidence: 0.5,
        active_devices: devices,
        state_deltas: deltas,
        baseline_delta: None,
        recording_session_id,
        temporal_patterns: vec![],
    }
}

// ── Notification + Presence + Gestures ───────────────────────────────────────

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

// ── GET /metrics  (Prometheus text/plain) ──────────────────────────────────────

async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let raw = state.raw_event_count.load(Ordering::Relaxed);
    let passed = state.event_count.load(Ordering::Relaxed);
    let ratio = *state.compression_ratio.lock().unwrap();
    let devices = state.devices.lock().unwrap().len();
    let uptime = state.start_time.elapsed().as_secs();
    let text = format!(
        "# HELP veyn_events_raw_total Total raw events received\n\
         # TYPE veyn_events_raw_total counter\n\
         veyn_events_raw_total {raw}\n\
         # HELP veyn_events_passed_total Events passed after compression\n\
         # TYPE veyn_events_passed_total counter\n\
         veyn_events_passed_total {passed}\n\
         # HELP veyn_compression_ratio Current compression ratio\n\
         # TYPE veyn_compression_ratio gauge\n\
         veyn_compression_ratio {ratio:.4}\n\
         # HELP veyn_active_devices Number of active devices\n\
         # TYPE veyn_active_devices gauge\n\
         veyn_active_devices {devices}\n\
         # HELP veyn_uptime_seconds Daemon uptime in seconds\n\
         # TYPE veyn_uptime_seconds counter\n\
         veyn_uptime_seconds {uptime}\n"
    );
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        text,
    )
}

// ── GET /v1/temporal/patterns ─────────────────────────────────────────────────

async fn temporal_patterns(State(state): State<AppState>) -> Json<serde_json::Value> {
    let patterns = state.temporal_engine.lock().unwrap().get_patterns();
    let count = patterns.len();
    Json(json!({ "patterns": patterns, "count": count }))
}

// ── GET /openapi.yaml ──────────────────────────────────────────────────────────

async fn openapi_spec() -> impl IntoResponse {
    const SPEC: &str = include_str!("../../../openapi.yaml");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/yaml")],
        SPEC,
    )
}

// ── POST /v1/memory ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct MemoryWriteRequest {
    topic: String,
    summary: String,
    context_snapshot: Option<serde_json::Value>,
}

async fn memory_write(
    State(state): State<AppState>,
    Json(req): Json<MemoryWriteRequest>,
) -> impl IntoResponse {
    if !state.config.memory_enabled {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "memory layer disabled" })),
        )
            .into_response();
    }

    let snap = state.latest_context.lock().unwrap().clone();
    let (hrv, hr, intent, confidence) = snap
        .as_ref()
        .map(|s| {
            let hrv = s
                .state_deltas
                .iter()
                .find(|d| d.metric == "hrv")
                .map(|d| d.value);
            let hr = s
                .state_deltas
                .iter()
                .find(|d| d.metric == "heart_rate")
                .map(|d| d.value);
            (hrv, hr, Some(s.intent.clone()), Some(s.confidence))
        })
        .unwrap_or((None, None, None, None));

    let ctx_blob = req
        .context_snapshot
        .or_else(|| snap.as_ref().and_then(|s| serde_json::to_value(s).ok()));

    let record = MemoryRecord {
        id: Uuid::new_v4().to_string(),
        timestamp_ms: Utc::now().timestamp_millis(),
        session_id: (*state.session_id).clone(),
        kind: MemoryKind::Semantic,
        topic: req.topic,
        summary: req.summary,
        intent_at_time: intent,
        confidence_at_time: confidence,
        hrv_at_time: hrv,
        hr_at_time: hr,
        context_snapshot: ctx_blob,
        outcome_rating: None,
        outcome_notes: None,
        outcome_at_ms: None,
    };

    match &state.db {
        Some(db) => {
            let store =
                crate::memory::MemoryStore::new(db.clone(), state.config.memory_max_records);
            match store.write(record.clone()) {
                Ok(()) => (
                    StatusCode::CREATED,
                    Json(serde_json::to_value(&record).unwrap()),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── GET /v1/memory/{id} ───────────────────────────────────────────────────────

async fn memory_get(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match &state.db {
        Some(db) => {
            let store =
                crate::memory::MemoryStore::new(db.clone(), state.config.memory_max_records);
            match store.get(&id) {
                Ok(Some(record)) => {
                    (StatusCode::OK, Json(serde_json::to_value(&record).unwrap())).into_response()
                }
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "record not found" })),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── DELETE /v1/memory/{id} ────────────────────────────────────────────────────

async fn memory_delete(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match &state.db {
        Some(db) => {
            let store =
                crate::memory::MemoryStore::new(db.clone(), state.config.memory_max_records);
            match store.delete(&id) {
                Ok(true) => StatusCode::NO_CONTENT.into_response(),
                Ok(false) => (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "record not found" })),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── GET /v1/memory ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct MemoryQueryParams {
    topic: Option<String>,
    since: Option<i64>,
    until: Option<i64>,
    kind: Option<String>,
    #[serde(default = "default_memory_limit")]
    limit: usize,
}

fn default_memory_limit() -> usize {
    20
}

async fn memory_query(
    State(state): State<AppState>,
    Query(params): Query<MemoryQueryParams>,
) -> impl IntoResponse {
    let kind = params.kind.as_deref().map(|k| {
        if k == "ambient" {
            MemoryKind::Ambient
        } else {
            MemoryKind::Semantic
        }
    });

    let q = MemoryQuery {
        topic: params.topic,
        since_ms: params.since,
        until_ms: params.until,
        kind,
        limit: Some(params.limit),
    };

    match &state.db {
        Some(db) => {
            let store =
                crate::memory::MemoryStore::new(db.clone(), state.config.memory_max_records);
            match store.query(&q) {
                Ok(records) => {
                    let count = records.len();
                    (
                        StatusCode::OK,
                        Json(json!({ "records": records, "count": count })),
                    )
                        .into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── PATCH /v1/memory/:id/outcome ──────────────────────────────────────────────

#[derive(Deserialize)]
struct OutcomeRequest {
    outcome_rating: String,
    notes: Option<String>,
}

async fn memory_anchor_outcome(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<OutcomeRequest>,
) -> impl IntoResponse {
    let rating = match req.outcome_rating.as_str() {
        "positive" => OutcomeRating::Positive,
        "negative" => OutcomeRating::Negative,
        "neutral" => OutcomeRating::Neutral,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "invalid outcome_rating; use positive, neutral, or negative"
                })),
            )
                .into_response()
        }
    };

    match &state.db {
        Some(db) => {
            let store =
                crate::memory::MemoryStore::new(db.clone(), state.config.memory_max_records);
            match store.anchor_outcome(&id, rating, req.notes) {
                Ok(()) => {
                    (StatusCode::OK, Json(json!({ "id": id, "anchored": true }))).into_response()
                }
                Err(e) => (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── GET /v1/patterns ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PatternsParams {
    min_samples: Option<u32>,
}

async fn patterns_list(
    State(state): State<AppState>,
    Query(params): Query<PatternsParams>,
) -> impl IntoResponse {
    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            match veyn_insight::analyze_patterns(&conn, params.min_samples) {
                Ok(patterns) => {
                    let count = patterns.len();
                    (
                        StatusCode::OK,
                        Json(json!({ "patterns": patterns, "count": count })),
                    )
                        .into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── GET /v1/clients  (14.1) ───────────────────────────────────────────────────

async fn clients_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let clients: Vec<ClientInfo> = state
        .connected_clients
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect();
    let count = clients.len();
    Json(json!({ "clients": clients, "count": count }))
}

// ── GET /v1/context/broadcast  (14.3) ─────────────────────────────────────────
// Auth-less loopback-only SSE endpoint for trusted local apps.

async fn context_broadcast(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> impl IntoResponse {
    // Loopback guard — only 127.0.0.1 and ::1 may subscribe.
    if !addr.ip().is_loopback() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "broadcast endpoint is loopback-only" })),
        )
            .into_response();
    }

    let rx = state.context_broadcast_tx.subscribe();
    let stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| match result {
            Ok(snapshot) => {
                let event_name = if matches!(&snapshot.intent_code, IntentCode::Other(s) if s == "baseline_drift") {
                    "baseline_drift"
                } else if snapshot.confidence < 0.4 {
                    "context_degraded"
                } else {
                    "context"
                };
                let json = serde_json::to_string(&snapshot).ok()?;
                Some(Ok::<_, std::convert::Infallible>(
                    axum::response::sse::Event::default()
                        .event(event_name)
                        .data(json),
                ))
            }
            Err(_) => None,
        });

    axum::response::sse::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

// ── GET /v1/inference/params  (13.1) ─────────────────────────────────────────

async fn inference_params_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let s = state.inference_state.lock().unwrap();
    Json(json!({
        "temperature":        s.current.temperature,
        "top_k":              s.current.top_k,
        "intent_code":        s.current.intent_code,
        "modulation_active":  s.modulation_active,
        "modulation_count":   s.modulation_count,
    }))
}

// ── GET /v1/coherence  (13.3) ─────────────────────────────────────────────────

async fn coherence_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let s = state.coherence_state.lock().unwrap();
    Json(json!({
        "kappa":          s.kappa,
        "mlr_triggered":  s.mlr_triggered,
        "mlr_count":      s.mlr_count,
        "last_audit_ms":  s.last_audit_ms,
        "threshold":      0.92,
    }))
}

// ── POST /v1/rules/simulate  (15.3) ──────────────────────────────────────────

#[derive(serde::Deserialize)]
struct SimulateRequest {
    /// Metric values to evaluate against rules.toml.
    metrics: std::collections::HashMap<String, f64>,
}

async fn rules_simulate(
    State(state): State<AppState>,
    Json(req): Json<SimulateRequest>,
) -> impl IntoResponse {
    // Build a synthetic snapshot from the supplied metrics and run the
    // compression engine's rule evaluator against it.
    use chrono::Utc;
    use veyn_schemas::StateDelta;

    let now = Utc::now().timestamp_millis();
    let deltas: Vec<StateDelta> = req
        .metrics
        .iter()
        .map(|(k, v)| StateDelta {
            device_id: "simulate".to_string(),
            metric: k.clone(),
            value: *v,
            unit: "".to_string(),
            ts: now,
            source_class: "simulate".to_string(),
        })
        .collect();

    // Evaluate rules using the shared compression engine.
    let rules_path = state.config.rules_path.clone();
    let (intent, intent_code, confidence) =
        crate::compression::evaluate_rules_from_path(&rules_path, &deltas);

    (
        StatusCode::OK,
        Json(json!({
            "intent":      intent,
            "intent_code": format!("{:?}", intent_code).to_lowercase(),
            "confidence":  confidence,
            "metrics":     req.metrics,
        })),
    )
}

// ── GET /v1/export?since=<ms>&until=<ms>  (16.1) ─────────────────────────────

#[derive(Deserialize)]
struct ExportAllParams {
    since: Option<i64>,
    until: Option<i64>,
    #[serde(default = "default_export_limit")]
    limit: usize,
}

fn default_export_limit() -> usize {
    10_000
}

async fn export_all(
    State(state): State<AppState>,
    Query(params): Query<ExportAllParams>,
) -> impl IntoResponse {
    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            let since = params.since.unwrap_or(i64::MIN);
            let until = params.until.unwrap_or(i64::MAX);
            let limit = params.limit as i64;
            let mut stmt = match conn.prepare(
                "SELECT ts, device_id, metric, value, unit, source
                 FROM events
                 WHERE ts >= ?1 AND ts <= ?2
                 ORDER BY ts ASC LIMIT ?3",
            ) {
                Ok(s) => s,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": e.to_string() })),
                    )
                        .into_response()
                }
            };
            let rows: Vec<serde_json::Value> = stmt
                .query_map(rusqlite::params![since, until, limit], |row| {
                    Ok(json!({
                        "ts":        row.get::<_, i64>(0)?,
                        "device_id": row.get::<_, String>(1)?,
                        "metric":    row.get::<_, String>(2)?,
                        "value":     row.get::<_, f64>(3)?,
                        "unit":      row.get::<_, String>(4)?,
                        "source":    row.get::<_, String>(5)?,
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();

            let count = rows.len();
            (
                StatusCode::OK,
                Json(json!({ "events": rows, "count": count, "since": since, "until": until })),
            )
                .into_response()
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── GET /v1/sessions/compare?ids=a,b  (16.1) ─────────────────────────────────

#[derive(Deserialize)]
struct SessionsCompareParams {
    /// Comma-separated list of session IDs to compare.
    ids: String,
}

async fn sessions_compare(
    State(state): State<AppState>,
    Query(params): Query<SessionsCompareParams>,
) -> impl IntoResponse {
    let ids: Vec<&str> = params.ids.split(',').map(str::trim).collect();
    if ids.is_empty() || ids.len() > 10 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "provide 1–10 session IDs in ?ids=a,b,c" })),
        )
            .into_response();
    }

    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            let mut result = serde_json::Map::new();
            for id in ids {
                match crate::storage::export_session_csv(&conn, id) {
                    Ok(csv) => {
                        result.insert(id.to_string(), serde_json::Value::String(csv));
                    }
                    Err(e) => {
                        result.insert(id.to_string(), json!({ "error": e.to_string() }));
                    }
                }
            }
            (StatusCode::OK, Json(serde_json::Value::Object(result))).into_response()
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}

// ── GET /v1/baseline/summary  (16.2) ─────────────────────────────────────────

async fn baseline_summary(State(state): State<AppState>) -> impl IntoResponse {
    match &state.db {
        Some(db) => {
            let conn = db.lock().unwrap();
            // Compute 7-day vs 30-day mean per (device_id, metric) pair.
            let pairs: Vec<(String, String)> = {
                match conn.prepare("SELECT DISTINCT device_id, metric FROM baseline_samples") {
                    Ok(mut stmt) => stmt
                        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                        .map(|rows| rows.filter_map(|r| r.ok()).collect())
                        .unwrap_or_default(),
                    Err(_) => Vec::new(),
                }
            };

            let mut summary = Vec::new();
            for (dev, met) in &pairs {
                let d7 =
                    crate::storage::load_baseline_samples(&conn, dev, met, 7).unwrap_or_default();
                let d30 =
                    crate::storage::load_baseline_samples(&conn, dev, met, 30).unwrap_or_default();

                let mean7 = if d7.is_empty() {
                    None
                } else {
                    Some(d7.iter().sum::<f64>() / d7.len() as f64)
                };
                let mean30 = if d30.is_empty() {
                    None
                } else {
                    Some(d30.iter().sum::<f64>() / d30.len() as f64)
                };

                let drift_sigma = match (mean7, mean30) {
                    (Some(m7), Some(m30)) if m30 != 0.0 => {
                        let std30: f64 = {
                            let var = d30.iter().map(|v| (v - m30).powi(2)).sum::<f64>()
                                / d30.len() as f64;
                            var.sqrt()
                        };
                        if std30 > 0.0 {
                            Some((m7 - m30) / std30)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                summary.push(json!({
                    "device_id":   dev,
                    "metric":      met,
                    "mean_7d":     mean7,
                    "mean_30d":    mean30,
                    "drift_sigma": drift_sigma,
                    "samples_7d":  d7.len(),
                    "samples_30d": d30.len(),
                }));
            }

            (
                StatusCode::OK,
                Json(json!({ "summary": summary, "count": summary.len() })),
            )
                .into_response()
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "persistence not available" })),
        )
            .into_response(),
    }
}
