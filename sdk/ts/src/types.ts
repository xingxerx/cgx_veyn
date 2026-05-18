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
