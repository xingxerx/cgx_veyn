use std::collections::HashMap;

use chrono::Utc;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

// ── IntentCode ─────────────────────────────────────────────────────────────────

/// Machine-readable intent classification for Intero physiological decision-support.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum IntentCode {
    #[default]
    Neutral,
    CognitiveLoad,
    StressResponse,
    Approach,
    Avoidance,
    Fatigue,
    Recovery,
    /// Pass-through for rule-defined or unknown codes.
    Other(String),
}

impl Serialize for IntentCode {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = match self {
            IntentCode::Neutral => "neutral",
            IntentCode::CognitiveLoad => "cognitive_load",
            IntentCode::StressResponse => "stress_response",
            IntentCode::Approach => "approach",
            IntentCode::Avoidance => "avoidance",
            IntentCode::Fatigue => "fatigue",
            IntentCode::Recovery => "recovery",
            IntentCode::Other(raw) => raw.as_str(),
        };
        s.serialize_str(v)
    }
}

impl<'de> Deserialize<'de> for IntentCode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = IntentCode;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an intent code string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<IntentCode, E> {
                Ok(match v {
                    "neutral" => IntentCode::Neutral,
                    "cognitive_load" => IntentCode::CognitiveLoad,
                    "stress_response" => IntentCode::StressResponse,
                    "approach" => IntentCode::Approach,
                    "avoidance" => IntentCode::Avoidance,
                    "fatigue" => IntentCode::Fatigue,
                    "recovery" => IntentCode::Recovery,
                    other => IntentCode::Other(other.to_string()),
                })
            }
        }
        d.deserialize_str(V)
    }
}

// ── TemporalTrend / TemporalSignal ────────────────────────────────────────────

/// Direction of change detected over the analysis window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalTrend {
    /// Change magnitude below threshold — effectively flat.
    Stable,
    /// Monotonically increasing slope.
    Rising,
    /// Monotonically decreasing slope.
    Falling,
    /// Rapid increase concentrated in the final quarter of the window.
    Spiking,
    /// Rapid decrease concentrated in the final quarter of the window.
    Declining,
    /// Recently rising after a prior falling period — signal rebounding.
    Recovering,
}

/// Trend analysis result for a single metric over a sliding time window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSignal {
    pub metric: String,
    pub trend: TemporalTrend,
    /// Rate of change in metric units per minute.
    pub slope_per_min: f64,
    /// Length of the analysis window in seconds.
    pub window_secs: u32,
    /// Confidence in the trend (R² of linear fit), 0.0–1.0.
    pub confidence: f32,
    /// Number of samples used for this analysis.
    pub samples: usize,
}

// ── ContextSnapshot ────────────────────────────────────────────────────────────

/// Semantic context snapshot — the AI-ready world-state summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub timestamp_ms: i64,
    pub session_id: String,
    /// Human-readable intent description.
    pub intent: String,
    /// Machine-readable intent code; agents branch on this, not the free-form string.
    #[serde(default)]
    pub intent_code: IntentCode,
    pub confidence: f64,
    /// Intero confidence score (0.0–1.0) derived from baseline z-scores.
    #[serde(default)]
    pub intent_confidence: f32,
    pub active_devices: Vec<String>,
    pub state_deltas: Vec<StateDelta>,
    /// Per-metric z-scores relative to personal baseline; None when baseline unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_delta: Option<HashMap<String, f64>>,
    /// The currently open recording session UUID, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_session_id: Option<String>,
    /// Temporal trend analysis for each metric over the sliding window.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub temporal_patterns: Vec<TemporalSignal>,
}

// ── StateDelta ─────────────────────────────────────────────────────────────────

/// One metric observation included in a context snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    pub device_id: String,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub ts: i64,
    /// Adapter source class — lets agents filter deltas by type without inspecting device_id.
    /// Values: "ble" | "mqtt" | "plugin" | "mock" | "healthkit" | "eeg" |
    ///         "evdev" | "hidraw" | "midi" | "serial" | "fs" | "presence"
    #[serde(default)]
    pub source_class: String,
}

// ── Session ────────────────────────────────────────────────────────────────────

/// A named recording session with optional annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub label: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub active_device_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Session {
    pub fn new(label: impl Into<String>, active_device_ids: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            label: label.into(),
            started_at: Utc::now().timestamp_millis(),
            ended_at: None,
            active_device_ids,
            notes: None,
        }
    }
}

/// Boundary event broadcast when a session starts or ends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBoundary {
    pub session_id: String,
    pub kind: SessionBoundaryKind,
    pub ts: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBoundaryKind {
    Start,
    End,
}

// ── BaselineStats ──────────────────────────────────────────────────────────────

/// Rolling-window baseline statistics for a single (device_id, metric) pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineStats {
    pub device_id: String,
    pub metric: String,
    pub mean: f64,
    pub stddev: f64,
    pub p10: f64,
    pub p90: f64,
    pub sample_count: usize,
    pub window_days: u32,
    pub updated_at: i64,
}

// ── VeynNotification ──────────────────────────────────────────────────────────

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

// ── Presence ──────────────────────────────────────────────────────────────────

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

// ── VeynEvent ─────────────────────────────────────────────────────────────────

/// Unified event emitted by every adapter, regardless of source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeynEvent {
    /// UUID v4
    pub id: String,
    /// Unix timestamp in milliseconds
    pub ts: i64,
    pub device_id: String,
    /// Source adapter name: "mock" | "healthkit" | "ble" | "eeg" | "evdev" | …
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

// ── VeynDevice ────────────────────────────────────────────────────────────────

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

// ── MemoryKind ────────────────────────────────────────────────────────────────

/// Classification of a memory record's origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// Auto-generated summary of a time window produced by the ambient writer task.
    Ambient,
    /// Explicitly written by the AI agent at the end of a meaningful session.
    #[default]
    Semantic,
}

// ── OutcomeRating ─────────────────────────────────────────────────────────────

/// Outcome rating anchored to a memory record after the fact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeRating {
    Positive,
    Neutral,
    Negative,
}

// ── MemoryRecord ──────────────────────────────────────────────────────────────

/// A persistent biometric memory entry linking a session topic to physiological state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryRecord {
    pub id: String,
    pub timestamp_ms: i64,
    pub session_id: String,
    pub kind: MemoryKind,
    pub topic: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_at_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_at_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hrv_at_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hr_at_time: Option<f64>,
    /// Full ContextSnapshot JSON blob at the time of the write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<serde_json::Value>,
    /// Outcome rating anchored after the fact via PATCH /v1/memory/{id}/outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_rating: Option<OutcomeRating>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_at_ms: Option<i64>,
}

// ── MemoryQuery ───────────────────────────────────────────────────────────────

/// Filter for querying memory records.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub topic: Option<String>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub kind: Option<MemoryKind>,
    pub limit: Option<usize>,
}

// ── PatternRecord ─────────────────────────────────────────────────────────────

/// Physiological pattern computed by veyn-insight for a memory topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRecord {
    /// Memory topic this pattern describes.
    pub topic: String,
    /// Number of memory records analysed.
    pub sample_count: u32,
    /// Mean heart rate observed while working on this topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_hr: Option<f64>,
    /// Mean HRV observed while working on this topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_hrv: Option<f64>,
    /// Most frequent intent code recorded during this topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dominant_intent: Option<String>,
    /// Intent code frequency map; keys are intent strings, values are 0.0–1.0 fractions.
    pub intent_distribution: serde_json::Value,
    /// UTC hour (0–23) with the highest record density for this topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_hour: Option<u8>,
    /// Timestamp of the most recent memory record for this topic.
    pub last_seen_ms: i64,
    /// When this pattern was computed.
    pub computed_at_ms: i64,
}
