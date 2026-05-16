use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Instant;


use chrono::Utc;
use serde::Serialize;
use tokio::sync::broadcast;
use veyn_schemas::{DeviceState, PresenceInfo, VeynDevice, VeynEvent, VeynNotification};

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

const RECENT_CAP: usize = 1_000;
const BROADCAST_CAP: usize = 256;
const NOTIF_CAP: usize = 64;

#[derive(Clone)]
pub struct AppState {
    pub token:            Arc<str>,
    pub recent_events:    Arc<Mutex<VecDeque<VeynEvent>>>,
    pub latest_metrics:   Arc<Mutex<HashMap<String, VeynEvent>>>,
    pub devices:          Arc<Mutex<HashMap<String, VeynDevice>>>,
    pub broadcast_tx:     broadcast::Sender<VeynEvent>,
    pub notification_tx:  broadcast::Sender<VeynNotification>,
    pub presence:         Arc<Mutex<HashMap<String, PresenceInfo>>>,
    pub start_time:       Arc<Instant>,
    pub event_count:      Arc<AtomicU64>,
    pub plugins:          Arc<Mutex<Vec<PluginInfo>>>,
}

impl AppState {
    pub fn new(token: String) -> Self {
        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAP);
        let (notification_tx, _) = broadcast::channel(NOTIF_CAP);
        Self {
            token:           Arc::from(token.as_str()),
            recent_events:   Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_CAP))),
            latest_metrics:  Arc::new(Mutex::new(HashMap::new())),
            devices:         Arc::new(Mutex::new(HashMap::new())),
            broadcast_tx,
            notification_tx,
            presence:        Arc::new(Mutex::new(HashMap::new())),
            start_time:      Arc::new(Instant::now()),
            event_count:     Arc::new(AtomicU64::new(0)),
            plugins:         Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn register_plugin(&self, info: PluginInfo) {
        self.plugins.lock().unwrap().push(info);
    }

    /// Ingest an event into all state stores and broadcast to WebSocket subscribers.
    pub fn ingest(&self, event: VeynEvent) {
        self.event_count.fetch_add(1, Ordering::Relaxed);

        // Update device registry
        {
            let mut devices = self.devices.lock().unwrap();
            let entry = devices
                .entry(event.device_id.clone())
                .or_insert_with(|| VeynDevice::new(&event.device_id, &event.device_id, &event.source));
            entry.state = DeviceState::Connected;
            entry.last_seen = Utc::now().timestamp_millis();
        }

        // Track latest value per metric
        {
            self.latest_metrics
                .lock()
                .unwrap()
                .insert(event.metric.clone(), event.clone());
        }

        // Append to bounded ring buffer
        {
            let mut recent = self.recent_events.lock().unwrap();
            if recent.len() >= RECENT_CAP {
                recent.pop_front();
            }
            recent.push_back(event.clone());
        }

        // Fan-out to any active WebSocket subscribers; ignore if none
        let _ = self.broadcast_tx.send(event);
    }
}
