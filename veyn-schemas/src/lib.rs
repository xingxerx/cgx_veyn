use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Semantic context snapshot — the AI-ready world-state summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub timestamp_ms: i64,
    pub session_id: String,
    pub intent: String,
    pub confidence: f64,
    pub active_devices: Vec<String>,
    pub state_deltas: Vec<StateDelta>,
}

/// One metric observation included in a context snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    pub device_id: String,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub ts: i64,
}

/// Notification sent from the daemon to a companion device (e.g. Apple Watch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeynNotification {
    pub id: String,
    pub ts: i64,
    pub title: String,
    pub body: String,
    /// If set, only route to the companion that owns this device ID.
    pub target_device: Option<String>,
}

impl VeynNotification {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            ts: Utc::now().timestamp_millis(),
            title: title.into(),
            body: body.into(),
            target_device: None,
        }
    }

    pub fn for_device(mut self, device_id: impl Into<String>) -> Self {
        self.target_device = Some(device_id.into());
        self
    }
}

/// Per-device presence record tracked by the presence detection task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PresenceState {
    Present,
    Absent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceInfo {
    pub device_id: String,
    pub state: PresenceState,
    /// Unix ms timestamp of the last observed event from this device.
    pub last_seen: i64,
    /// Unix ms timestamp when the current state was entered.
    pub since_ts: i64,
}

/// Unified event emitted by every adapter, regardless of source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeynEvent {
    /// UUID v4
    pub id: String,
    /// Unix timestamp in milliseconds
    pub ts: i64,
    pub device_id: String,
    /// Source adapter name: "mock" | "healthkit" | "ble" | "eeg"
    pub source: String,
    /// Metric name: "heart_rate" | "hrv" | "spo2" | …
    pub metric: String,
    pub value: f64,
    /// SI / common unit string: "bpm" | "ms" | "%" | …
    pub unit: String,
    /// Arbitrary adapter-specific key/value pairs
    pub meta: HashMap<String, serde_json::Value>,
}

impl VeynEvent {
    pub fn new(
        device_id: impl Into<String>,
        source: impl Into<String>,
        metric: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            ts: Utc::now().timestamp_millis(),
            device_id: device_id.into(),
            source: source.into(),
            metric: metric.into(),
            value,
            unit: unit.into(),
            meta: HashMap::new(),
        }
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.meta.insert(key.into(), value);
        self
    }
}

/// Registered device with connection state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeynDevice {
    pub id: String,
    pub name: String,
    pub source: String,
    pub state: DeviceState,
    /// Unix timestamp in milliseconds of last received event
    pub last_seen: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceState {
    Connected,
    Disconnected,
    Scanning,
}

impl VeynDevice {
    pub fn new(id: impl Into<String>, name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: source.into(),
            state: DeviceState::Connected,
            last_seen: Utc::now().timestamp_millis(),
        }
    }
}
