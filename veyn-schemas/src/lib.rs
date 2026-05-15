use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: source.into(),
            state: DeviceState::Connected,
            last_seen: Utc::now().timestamp_millis(),
        }
    }
}
