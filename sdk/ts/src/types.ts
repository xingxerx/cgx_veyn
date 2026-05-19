// Types matching the Rust schema in veyn-schemas/src/lib.rs

// ── IntentCode ────────────────────────────────────────────────────────────────

/**
 * Machine-readable intent classification for Intero physiological decision-support.
 * Known variants are literal string constants; unknown/rule-defined codes are
 * represented as plain strings via the Other variant.
 */
export type IntentCode =
  | "neutral"
  | "cognitive_load"
  | "stress_response"
  | "approach"
  | "avoidance"
  | "fatigue"
  | "recovery"
  | string; // Other(String) pass-through

// ── StateDelta ────────────────────────────────────────────────────────────────

/**
 * One metric observation included in a context snapshot.
 * source_class values: "ble" | "mqtt" | "plugin" | "mock" | "healthkit" |
 *   "eeg" | "evdev" | "hidraw" | "midi" | "serial" | "fs" | "presence"
 */
export interface StateDelta {
  device_id: string;
  metric: string;
  value: number;
  unit: string;
  ts: number;
  source_class: string;
}

// ── ContextSnapshot ───────────────────────────────────────────────────────────

/** Semantic context snapshot — the AI-ready world-state summary. */
export interface ContextSnapshot {
  timestamp_ms: number;
  session_id: string;
  /** Human-readable intent description. */
  intent: string;
  /** Machine-readable intent code; agents branch on this, not the free-form string. */
  intent_code: IntentCode;
  confidence: number;
  /** Intero confidence score (0.0–1.0) derived from baseline z-scores. */
  intent_confidence: number;
  active_devices: string[];
  state_deltas: StateDelta[];
  /** Per-metric z-scores relative to personal baseline; absent when baseline unavailable. */
  baseline_delta?: Record<string, number>;
  /** The currently open recording session UUID, if any. */
  recording_session_id?: string;
  /** Temporal trend analysis for each metric over the sliding window. */
  temporal_patterns?: TemporalSignal[];
}

// ── Session ───────────────────────────────────────────────────────────────────

/** A named recording session with optional annotations. */
export interface Session {
  id: string;
  label: string;
  started_at: number;
  ended_at?: number;
  active_device_ids: string[];
  notes?: string;
}

/** Boundary event broadcast when a session starts or ends. */
export interface SessionBoundary {
  session_id: string;
  kind: "start" | "end";
  ts: number;
  label: string;
}

// ── BaselineStats ─────────────────────────────────────────────────────────────

/** Rolling-window baseline statistics for a single (device_id, metric) pair. */
export interface BaselineStats {
  device_id: string;
  metric: string;
  mean: number;
  stddev: number;
  p10: number;
  p90: number;
  sample_count: number;
  window_days: number;
  updated_at: number;
}

// ── VeynEvent ─────────────────────────────────────────────────────────────────

/** Unified event emitted by every adapter, regardless of source. */
export interface VeynEvent {
  /** UUID v4 */
  id: string;
  /** Unix timestamp in milliseconds */
  ts: number;
  device_id: string;
  /** Source adapter name: "mock" | "healthkit" | "ble" | "eeg" | "evdev" | … */
  source: string;
  /** Metric name: "heart_rate" | "hrv" | "spo2" | … */
  metric: string;
  value: number;
  /** SI / common unit string: "bpm" | "ms" | "%" | … */
  unit: string;
  /** Arbitrary adapter-specific key/value pairs */
  meta: Record<string, unknown>;
}

// ── VeynDevice ────────────────────────────────────────────────────────────────

export type DeviceState = "connected" | "disconnected" | "scanning";

/** Registered device with connection state. */
export interface VeynDevice {
  id: string;
  name: string;
  source: string;
  state: DeviceState;
  /** Unix timestamp in milliseconds of last received event */
  last_seen: number;
}

// ── ContextTier ───────────────────────────────────────────────────────────────

/**
 * Controls which layer of data a token (or the daemon default) exposes.
 *
 * - `"raw"`      — full VeynEvent stream, unfiltered
 * - `"filtered"` — compression-filtered events only (delta + debounce applied)
 * - `"semantic"` — only ContextSnapshot; raw events are not exposed
 *
 * Configure daemon default in veyn.toml: `context_tier = "semantic"`
 * Set token ceiling via scope `"tier:semantic"` in tokens.json.
 */
export type ContextTier = "raw" | "filtered" | "semantic";

// ── WebSocket session frame ───────────────────────────────────────────────────

/**
 * Wraps a VeynEvent when a recording session is active.
 * Emitted on the WebSocket stream (Raw/Filtered tier) instead of a bare event.
 */
export interface SessionFrame {
  session_id: string;
  channel: string;
  event: VeynEvent;
}

// ── Baseline history ──────────────────────────────────────────────────────────

export interface BaselineDailyPoint {
  ts: number;
  mean: number;
}

export interface BaselineHistoryResponse {
  device_id: string;
  metric: string;
  days: number;
  history: BaselineDailyPoint[];
}

// ── TemporalTrend / TemporalSignal ────────────────────────────────────────────

/**
 * Direction of change detected over the sliding analysis window.
 * - `stable`     — effectively flat (change < threshold)
 * - `rising`     — monotonically increasing slope
 * - `falling`    — monotonically decreasing slope
 * - `spiking`    — rapid increase concentrated near the end of the window
 * - `declining`  — rapid decrease concentrated near the end of the window
 * - `recovering` — recently rising after a prior falling period
 */
export type TemporalTrend =
  | "stable"
  | "rising"
  | "falling"
  | "spiking"
  | "declining"
  | "recovering";

/** Trend analysis result for one metric over the sliding time window. */
export interface TemporalSignal {
  metric: string;
  trend: TemporalTrend;
  /** Rate of change in metric units per minute. */
  slope_per_min: number;
  /** Length of the analysis window in seconds. */
  window_secs: number;
  /** Confidence in the trend (R² of linear fit), 0.0–1.0. */
  confidence: number;
  /** Number of samples used for this analysis. */
  samples: number;
}

// ── Client filter/response types ──────────────────────────────────────────────

export interface HealthResponse {
  status: string;
  [key: string]: unknown;
}

export interface SubscribeFilter {
  intents?: string[];
  minConfidence?: number;
  sourceClass?: string[];
}

export interface WsFilter {
  deviceClass?: string[];
  metrics?: string[];
}

/**
 * Runtime filter sent to the Semantic WebSocket tier to control which
 * `ContextSnapshot` objects are forwarded.  Send as:
 * ```json
 * { "type": "subscribe", "context_filter": { "intents": ["stress_response"], "min_confidence": 0.7 } }
 * ```
 */
export interface WsContextFilter {
  /** Only forward snapshots whose intent_code is in this list. */
  intents?: string[];
  /** Drop snapshots with confidence below this threshold. */
  minConfidence?: number;
  /** Retain only state_deltas from these source classes. */
  sourceClass?: string[];
  /** When true, suppress snapshots with intent_code == "neutral". */
  excludeNeutral?: boolean;
}

// ── Memory layer ──────────────────────────────────────────────────────────────

export type MemoryKind = "ambient" | "semantic";

export type OutcomeRating = "positive" | "neutral" | "negative";

/** A persistent biometric memory entry linking a topic to physiological state. */
export interface MemoryRecord {
  id: string;
  timestamp_ms: number;
  session_id: string;
  kind: MemoryKind;
  topic: string;
  summary: string;
  intent_at_time?: string;
  confidence_at_time?: number;
  hrv_at_time?: number;
  hr_at_time?: number;
  context_snapshot?: unknown;
  outcome_rating?: OutcomeRating;
  outcome_notes?: string;
  outcome_at_ms?: number;
}

/** Query filter for GET /v1/memory. */
export interface MemoryQuery {
  topic?: string;
  since?: number;
  until?: number;
  kind?: MemoryKind;
  limit?: number;
}

// ── Pattern detection (veyn-insight) ─────────────────────────────────────────

/** Physiological pattern computed by veyn-insight for a memory topic. */
export interface PatternRecord {
  topic: string;
  sample_count: number;
  avg_hr?: number;
  avg_hrv?: number;
  dominant_intent?: string;
  /** Intent code → frequency (0.0–1.0) */
  intent_distribution: Record<string, number>;
  /** UTC hour (0–23) with highest record density for this topic. */
  peak_hour?: number;
  last_seen_ms: number;
  computed_at_ms: number;
}
