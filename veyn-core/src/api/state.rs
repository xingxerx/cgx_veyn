use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

use chrono::Utc;
use serde::Serialize;
use tokio::sync::broadcast;
use veyn_schemas::{
    ContextSnapshot, DeviceState, PresenceInfo, SessionBoundary, VeynDevice, VeynEvent,
    VeynNotification,
};

use crate::auth::ScopedToken;
use crate::baseline::BaselineEngine;
use crate::config::Config;
use crate::session::SessionManager;
use crate::temporal::TemporalEngine;

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

const RECENT_CAP: usize = 1_000;
const BROADCAST_CAP: usize = 256;
const NOTIF_CAP: usize = 64;
const CONTEXT_BROADCAST_CAP: usize = 64;
const SESSION_BOUNDARY_CAP: usize = 32;

#[derive(Clone)]
pub struct AppState {
    pub recent_events: Arc<Mutex<VecDeque<VeynEvent>>>,
    pub latest_metrics: Arc<Mutex<HashMap<String, VeynEvent>>>,
    pub devices: Arc<Mutex<HashMap<String, VeynDevice>>>,
    pub broadcast_tx: broadcast::Sender<VeynEvent>,
    pub notification_tx: broadcast::Sender<VeynNotification>,
    pub presence: Arc<Mutex<HashMap<String, PresenceInfo>>>,
    pub start_time: Arc<Instant>,
    /// Filtered event count (after compression).
    pub event_count: Arc<AtomicU64>,
    /// Total raw event count (before compression).
    pub raw_event_count: Arc<AtomicU64>,
    pub plugins: Arc<Mutex<Vec<PluginInfo>>>,
    pub auth_token: Arc<String>,
    /// Additional scope-limited tokens beyond the primary full-access token.
    pub scoped_tokens: Arc<Vec<ScopedToken>>,
    pub config: Arc<Config>,
    pub session_id: Arc<String>,
    pub context_history: Arc<Mutex<VecDeque<ContextSnapshot>>>,
    pub latest_context: Arc<Mutex<Option<ContextSnapshot>>>,
    pub compression_ratio: Arc<Mutex<f64>>,
    /// Broadcast channel for context snapshots (used by SSE subscribers).
    pub context_broadcast_tx: broadcast::Sender<ContextSnapshot>,

    // ── Intero infrastructure ─────────────────────────────────────────────────
    /// Named recording session manager.
    pub session_manager: Arc<Mutex<SessionManager>>,
    /// Broadcast channel for session start/end boundary events.
    #[allow(dead_code)]
    pub session_boundary_tx: broadcast::Sender<SessionBoundary>,
    /// Rolling-window personal baseline engine.
    pub baseline_engine: Arc<Mutex<BaselineEngine>>,
    /// Sliding-window temporal trend engine.
    pub temporal_engine: Arc<Mutex<TemporalEngine>>,
    /// Optional SQLite connection for session + event persistence.
    pub db: Option<Arc<Mutex<rusqlite::Connection>>>,
}

impl AppState {
    pub fn new(
        auth_token: String,
        scoped_tokens: Vec<ScopedToken>,
        config: Config,
        db: Option<rusqlite::Connection>,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAP);
        let (notification_tx, _) = broadcast::channel(NOTIF_CAP);
        let (context_broadcast_tx, _) = broadcast::channel(CONTEXT_BROADCAST_CAP);
        let (session_boundary_tx, _) = broadcast::channel(SESSION_BOUNDARY_CAP);
        let session_id = uuid::Uuid::new_v4().to_string();
        let history_cap = config.context_history_size;
        let config = Arc::new(config);

        let session_manager =
            Arc::new(Mutex::new(SessionManager::new(session_boundary_tx.clone())));
        let baseline_engine = Arc::new(Mutex::new(BaselineEngine::new()));
        let temporal_engine = Arc::new(Mutex::new(TemporalEngine::new()));
        let db = db.map(|c| Arc::new(Mutex::new(c)));

        Self {
            recent_events: Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_CAP))),
            latest_metrics: Arc::new(Mutex::new(HashMap::new())),
            devices: Arc::new(Mutex::new(HashMap::new())),
            broadcast_tx,
            notification_tx,
            presence: Arc::new(Mutex::new(HashMap::new())),
            start_time: Arc::new(Instant::now()),
            event_count: Arc::new(AtomicU64::new(0)),
            raw_event_count: Arc::new(AtomicU64::new(0)),
            plugins: Arc::new(Mutex::new(Vec::new())),
            auth_token: Arc::new(auth_token),
            scoped_tokens: Arc::new(scoped_tokens),
            config,
            session_id: Arc::new(session_id),
            context_history: Arc::new(Mutex::new(VecDeque::with_capacity(history_cap))),
            latest_context: Arc::new(Mutex::new(None)),
            compression_ratio: Arc::new(Mutex::new(1.0)),
            context_broadcast_tx,
            session_manager,
            session_boundary_tx,
            baseline_engine,
            temporal_engine,
            db,
        }
    }

    pub fn register_plugin(&self, info: PluginInfo) {
        self.plugins.lock().unwrap().push(info);
    }

    /// Ingest a compression-filtered event into all state stores.
    pub fn ingest(&self, event: VeynEvent) {
        self.event_count.fetch_add(1, Ordering::Relaxed);

        {
            let mut devices = self.devices.lock().unwrap();
            let entry = devices.entry(event.device_id.clone()).or_insert_with(|| {
                VeynDevice::new(&event.device_id, &event.device_id, &event.source)
            });
            entry.state = DeviceState::Connected;
            entry.last_seen = Utc::now().timestamp_millis();
        }

        self.latest_metrics
            .lock()
            .unwrap()
            .insert(event.metric.clone(), event.clone());

        {
            let mut recent = self.recent_events.lock().unwrap();
            if recent.len() >= RECENT_CAP {
                recent.pop_front();
            }
            recent.push_back(event.clone());
        }

        let _ = self.broadcast_tx.send(event);
    }

    /// Push a new context snapshot into the history ring buffer and broadcast it.
    pub fn update_context(&self, snapshot: ContextSnapshot) {
        let cap = self.config.context_history_size;
        let mut hist = self.context_history.lock().unwrap();
        if hist.len() >= cap {
            hist.pop_front();
        }
        hist.push_back(snapshot.clone());
        drop(hist);
        *self.latest_context.lock().unwrap() = Some(snapshot.clone());
        let _ = self.context_broadcast_tx.send(snapshot);
    }
}
