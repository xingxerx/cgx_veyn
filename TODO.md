# VEYN — Master TODO

> Complete work backlog across all crates and subsystems.
> Replace the existing `TODO.md` with this file entirely.
> 
> Priority: 🔴 Critical (blocks launch) → 🟡 High (needed for safety/correctness) → 🟢 Nice-to-have
> Build order within each section is top-to-bottom unless noted.

-----

## 1. 🔴 Critical: Semantic Compression Engine

The biggest architectural gap. Raw HID/BLE/MIDI data at 100–1000 Hz is
unusable by any AI agent without compression and synthesis first.

- [x] ✅ State Reduction Layer — delta filtering, temporal debounce, epsilon magnitude thresholds (`veyn-core/src/compression.rs`)
- [x] ✅ Semantic Synthesis Engine — rule-based intent classification with hot-reloadable `rules.toml` (30 s interval, no restart required)
- [x] ✅ `ContextSnapshot` output — `intent`, `confidence`, `active_devices`, `state_deltas`, `timestamp_ms`, `session_id`
- [ ] 🟢 Optional: lightweight local SLM integration (e.g. `llama.cpp` via FFI or subprocess) as a secondary synthesis pass for ambiguous intent classification where `confidence < 0.6`

-----

## 2. 🔴 Critical: Security

An unauthenticated WebSocket on `:7700` exposing raw HID is spyware-grade risk.

- [x] ✅ 256-bit random token generated on first run, stored at `~/.local/share/veyn/token` (chmod 600)
- [x] ✅ `Authorization: Bearer <token>` required on all REST and WebSocket endpoints when `require_auth = true`
- [x] ✅ Host header validation — blocks DNS-rebinding attacks
- [x] ✅ Strict CORS — deny cross-origin by default, configurable allowlist in `veyn.toml`
- [x] ✅ `strip_raw_hid = true` default — only semantic intent exposed, not raw input content
- [x] ✅ Audit log at `~/.local/share/veyn/audit.log`
- [x] ✅ Add **scope-limited tokens** — a token can be minted with read-only or device-class-scoped access (e.g. MIDI only, no HID); store scope in token metadata alongside the 256-bit secret
- [x] ✅ Add **rate limiting** on REST endpoints — token bucket, max 100 req/s per client IP; use `tower::ServiceBuilder` with a rate-limit layer
- [ ] 🟢 Consider mutual TLS (mTLS) option for high-security deployments

-----

## 3. 🔴 Critical: WASM Plugin Architecture

WASM guests are sandboxed — they cannot open OS device handles directly.
The current plugin stubs assume direct hardware access, which will silently
fail or panic at runtime.

- [x] ✅ Add a **Device Proxy Layer** in `veyn-core` — plugins declare a device descriptor (VID/PID, BLE UUID, serial pattern) in `plugin.toml`; the daemon opens the device and passes byte buffers into the WASM sandbox via host-function imports; guests never call OS APIs directly
- [x] ✅ Expose host-function imports in the WASM ABI: `veyn::read_device(handle) -> bytes`, `veyn::http_get(url) -> bytes`, `veyn::time_ms() -> i64`, `veyn::log(level, msg)`
- [x] ✅ Add **plugin signature verification** — each `.wasm` binary must be signed with a keypair; `veyn-core` validates the signature on load and rejects unsigned plugins unless `plugins.allow_unsigned = true` in `veyn.toml`
- [x] ✅ Add `veyn plugin install <path>` CLI subcommand — validates manifest, checks signature, copies to plugin directory
- [ ] 🟢 Publish `veyn-plugin-midi-launchpad` as a reference implementation showing the full Device Proxy pattern

-----

## 4. 🟡 High: `veyn-core` Daemon Foundation

- [x] ✅ Bounded event bus channel (1024) — drops oldest events under backpressure to prevent OOM
- [x] ✅ Graceful shutdown on SIGTERM / SIGINT
- [x] ✅ Config priority: CLI flags > env vars > `veyn.toml` > defaults
- [x] ✅ Context snapshot ring buffer (configurable, default 32)
- [x] ✅ Implement device hot-plug/unplug detection — when a BLE or HID device disconnects, the daemon must mark it absent in `VeynDevice` and `PresenceInfo` without crashing; adapter task should restart with exponential backoff (already partially in place for adapters, needs wiring to device registry)
- [x] ✅ Add Prometheus metrics endpoint `GET /metrics` — counters for `events_raw_total`, `events_passed_total`, `compression_ratio`, `active_devices`; compatible with Grafana/Prometheus scrape

-----

## 5. 🟡 High: `veyn-adapters` — Platform Coverage

- [x] ✅ Mock adapter — synthetic event generator for dev and CI
- [x] ✅ BLE adapter — GATT Heart Rate Profile scan + decode + battery monitoring + device persistence
- [x] ✅ OSC/EEG adapter — UDP OSC input stream (Mind Monitor compatible)
- [x] ✅ HealthKit TCP Relay — iOS companion bridge with mDNS auto-discovery
- [x] ✅ MQTT adapter — IoT / smart home output
- [x] ✅ Linux: `evdev` HID adapter — keyboard, mouse, gamepad via `/dev/input/event*`; emit `VeynEvent` with `source = "hid"` and appropriate metric names
- [x] ✅ Linux: `hidraw` adapter — raw USB HID for devices not covered by `evdev`
- [ ] 🟡 macOS: `IOKit`/`IOHIDManager` adapter — equivalent HID coverage on macOS
- [ ] 🟡 Windows: `WinUSB` / `RawInput` adapter — equivalent HID coverage on Windows
- [x] ✅ MIDI adapter (`midir` crate) — CC events, note on/off, clock; emit as `VeynEvent` with `source = "midi"`
- [x] ✅ Serial/UART adapter (`serialport` crate) — configurable baud, parity, stop bits; useful for custom biometric hardware
- [x] ✅ Filesystem watcher adapter (`notify` crate) — emit events on file create/modify/delete for specified watch paths
- [ ] 🟢 OSC output adapter — send OSC messages to DAW/VJ software as a downstream sink (current adapter is input only)
- [ ] 🟢 Audio level adapter (`cpal` crate) — RMS/peak metering from default input device; useful for voice/environment sensing

-----

## 6. 🟡 High: `/context/current` API Contract

- [x] ✅ `GET /v1/context/current` — current semantic snapshot
- [x] ✅ `GET /v1/context/history?n=10` — last N snapshots (ring buffer, max 32)
- [x] ✅ Versioned `/v1/` prefix with legacy unversioned routes retained for backward compat
- [x] ✅ Add OpenAPI 3.0 spec (`openapi.yaml`) — hand-write or generate from Axum routes; keep in sync with route changes; publish at `GET /openapi.yaml`
- [ ] 🟡 WebSocket subscribe filtering — clients send a JSON filter object on connect specifying device classes or intent categories; server only pushes matching events; avoids forcing clients to filter client-side
- [ ] 🟢 Server-Sent Events (SSE) alternative at `GET /v1/stream/sse` — for HTTP-only clients that cannot upgrade to WebSocket

-----

## 7. 🟡 High: SDK (`/sdk`)

- [x] ✅ Rust guest-side plugin SDK (`veyn-plugin-sdk`) with WASM ABI macros
- [x] ✅ TypeScript/Node.js SDK — `npm install veyn-sdk`; typed `VeynEvent` and `ContextSnapshot` interfaces; async `subscribe()` over WebSocket; Bearer auth handling
- [x] ✅ Python SDK — `pip install veyn-sdk`; async context manager; typed dataclasses matching Rust schema; example Jupyter notebook
- [x] ✅ SDK usage examples for: connecting an LLM agent, building a WASM plugin, reading context history programmatically
- [ ] 🟢 Go SDK — for server-side agent integrations

-----

## 8. 🟢 Developer Experience

- [ ] 🟢 Add `docker-compose.yml` — daemon + mock adapter + example consumer agent; useful for CI and onboarding
- [x] ✅ Add integration test suite — spin up daemon in `VEYN_MOCK=true` mode, connect mock adapters, assert that `ContextSnapshot` values match expected intent strings after injecting known metric sequences
- [x] ✅ Add `veyn doctor` CLI subcommand — checks prerequisites (Rust toolchain, OS permissions, BLE availability), token validity, device access; prints a structured pass/fail report
- [ ] 🟢 Add minimal web UI at `GET /ui` — live context feed, connected devices list, log tail; plain HTML/JS served by the daemon itself, no framework dependency

-----

## 9. 🟢 AI Integration Prep

- [ ] 🟢 Design and document the **Agent Handshake Protocol** — how an AI agent authenticates, declares its capability requirements, and subscribes to the appropriate context tier
- [ ] 🟢 Add `GET /v1/context/subscribe` with declarative filter DSL — e.g. `{ "intents": ["idle", "stress_response"], "min_confidence": 0.7 }`; returns a filtered SSE or WebSocket stream
- [ ] 🟢 Add `context_tier` config option: `raw` | `filtered` | `semantic` — tokens are minted with a tier ceiling; agents only receive data at or below their authorized tier
- [ ] 🟢 Document recommended integration patterns for: Claude via `veyn-mcp`, local Ollama agents, OpenAI function calling

-----

## 10. 🔴 Critical: Intero Infrastructure

Intero is the first-party physiological decision-support app built on VEYN.
These four workstreams are hard prerequisites — Intero cannot function without them.

**Build order: 10.1 → 10.2 → 10.3 → 10.4**

> ⚠️ Disambiguation: `AppState::session_id` is a daemon-lifecycle UUID (set once
> at startup). The `InteroSession` introduced in 10.1 is a user-initiated named
> session. These are two distinct concepts — do not conflate them. The existing
> `session_id` field on `ContextSnapshot` continues to hold the daemon-lifecycle
> UUID and must not be replaced.

-----

### 10.1 Synchronized Multi-Signal Session Recording (`veyn-schemas`, `veyn-core`)

VEYN normalises events into a single chronological stream with no coordination
between adapters. Intero requires all active adapters to sample within the same
bounded window so the output is a coherent multi-channel timeline.

- [x] ✅ Define `InteroSession` in `veyn-schemas`:
  
  ```rust
  pub struct InteroSession {
      pub id: Uuid,
      pub label: String,              // user-supplied name for the session
      pub annotation: Option<String>, // the decision being considered
      pub started_at: DateTime<Utc>,
      pub ended_at: Option<DateTime<Utc>>,
      pub active_device_ids: Vec<String>,
  }
  ```
- [x] ✅ Define `SessionBoundary` control event in `veyn-schemas` — a non-metric envelope broadcast on the event bus to signal `Start { session_id }` and `End { session_id }` to all active adapters; adapters must not alter their emit logic, only the dispatcher uses this to gate session-scoped writes
- [x] ✅ Add `SessionManager` to `veyn-core` — opens/closes `InteroSession` instances; broadcasts `SessionBoundary` events; holds the currently active session id in `Arc<Mutex<Option<Uuid>>>`; wired into `AppState`
- [x] ✅ REST endpoints for session control:
  - `POST /v1/session/start` — body: `{ "label": string, "annotation": string? }` → returns `InteroSession`
  - `POST /v1/session/stop` — stops the active session; returns final `InteroSession` with `ended_at` set
  - `GET /v1/session/{id}` — returns full multi-channel event timeline for the session
- [x] ✅ WebSocket session framing — when a session is active, the event stream wraps events in a `session_frame` envelope: `{ "session_id": uuid, "channel": device_id, "event": VeynEvent }` so clients can reconstruct aligned timelines
- [x] ✅ Add MCP tools to `veyn-mcp`: `veyn_start_session`, `veyn_stop_session`, `veyn_get_session`

-----

### 10.2 Named Sessions with Metadata and Persistence (`veyn-core`, SQLite)

The persistence layer stores events but has no concept of a named session or
user annotation. Intero needs to store what decision was being considered
alongside the physiological data.

- [x] ✅ Add `sessions` table to the SQLite schema (new migration):
  
  ```sql
  CREATE TABLE sessions (
      id          TEXT PRIMARY KEY,  -- UUID v4
      label       TEXT NOT NULL,
      annotation  TEXT,              -- the decision being considered
      started_at  INTEGER NOT NULL,  -- Unix ms
      ended_at    INTEGER,           -- Unix ms, NULL while active
      device_ids  TEXT NOT NULL      -- JSON array of strings
  );
  ```
- [x] ✅ Add `session_id` column (nullable `TEXT`) to the existing `events` table migration — set to the active `InteroSession.id` for all events captured during a session; NULL for events outside any session
- [x] ✅ Persist `InteroSession` to SQLite on `session/start` and `session/stop`
- [x] ✅ `GET /v1/session/{id}/replay` — returns all events for the session in chronological order at **full resolution** (bypasses `CompressionEngine`; reads directly from SQLite by `session_id`)
- [x] ✅ `PATCH /v1/session/{id}` — update `label` or `annotation` after the session ends; useful for post-hoc labelling
- [x] ✅ `GET /v1/sessions` — paginated list ordered by `started_at DESC`; response includes `{ id, label, annotation, started_at, ended_at, duration_ms, device_count, event_count }`
- [x] ✅ `GET /v1/session/{id}/export?format=csv` — flat CSV with columns `timestamp_ms, device_id, metric, value, unit`

-----

### 10.3 Personal Baseline Tracking (`veyn-core`, `veyn-schemas`)

A raw reading carries no meaning without a personal reference point. A heart
rate of 72 BPM means nothing; 72 BPM when your 30-day average is 58 BPM means
you are stressed. Intero displays deltas against personal baselines, not
population norms.

- [x] ✅ Add `BaselineStats` to `veyn-schemas`:
  
  ```rust
  pub struct BaselineStats {
      pub device_id: String,
      pub metric: String,
      pub mean: f64,
      pub stddev: f64,
      pub p10: f64,
      pub p90: f64,
      pub sample_count: u64,
      pub window_days: u32,
      pub computed_at: DateTime<Utc>,
      pub sufficient: bool, // false until >= 7 days of data exist
  }
  ```
- [x] ✅ Implement `BaselineEngine` in `veyn-core` — maintains a rolling window of statistics per `(device_id, metric)` tuple; default lookback: 30 days; queries SQLite event history; runs as a background task recalculating on a 15-minute interval
- [x] ✅ Persist `BaselineStats` snapshots to a `baselines` SQLite table on each recalculation so they survive daemon restarts
- [x] ✅ `GET /v1/baseline/{device_id}/{metric}` — returns current `BaselineStats` including `sufficient` flag; clients must check `sufficient` before using delta values
- [x] ✅ Attach `baseline_delta` map to `ContextSnapshot` — keyed by `"{device_id}/{metric}"`, value is the z-score `(current - mean) / stddev`; only populated for metrics where `sufficient = true`
- [x] ✅ Guard: when `sufficient = false`, exclude the metric from z-score calculations and set `intent_confidence` floor to `0.4` in the `CompressionEngine`
- [x] ✅ `GET /v1/baseline/{device_id}/{metric}/history?days=30` — time-series of daily mean values; used by Intero’s longitudinal trend view

-----

### 10.4 `intent_code` Structured Field (`veyn-schemas`, `veyn-core`)

`ContextSnapshot.intent` is currently a free-form `String` produced by
`rules.toml`. Intero’s AI analysis layer needs a typed, structured field it
can pattern-match against reliably — not a string to parse.

- [x] ✅ Define `IntentCode` enum in `veyn-schemas`:
  
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  #[serde(rename_all = "snake_case")]
  pub enum IntentCode {
      Neutral,
      CognitiveLoad,
      StressResponse,
      Approach,       // positive arousal + HRV stability
      Avoidance,      // elevated HR + suppressed HRV
      Fatigue,
      Recovery,
      Other(String),  // forward-compatible catch-all
  }
  ```
- [x] ✅ Add `intent_code: IntentCode` and `intent_confidence: f32` (0.0–1.0) fields to `ContextSnapshot` — keep the existing `intent: String` field for backward compat; `intent_code` is additive
- [x] ✅ Implement structured intent classification in `CompressionEngine::synthesize()` — derive `intent_code` from the combination of active metric deltas against personal baseline (requires `BaselineEngine` output; build 10.3 first):
  - `StressResponse`: HR z-score > 1.5 AND HRV z-score < -1.0
  - `Approach`: HR z-score 0.5–1.5 AND HRV z-score > 0.0
  - `Avoidance`: HR z-score > 0.5 AND HRV z-score < -0.5 AND skin_temp z-score > 0.5
  - `CognitiveLoad`: EEG beta z-score > 1.0 AND alpha z-score < -0.5
  - `Fatigue`: HR z-score < -0.5 AND EEG theta z-score > 1.0
  - `Recovery`: HRV z-score > 1.0 AND HR z-score < 0.0
  - `Neutral`: no conditions met above threshold
  - `intent_confidence`: derived from signal quality (number of active devices) and baseline sufficiency; cap at 0.4 if any contributing metric has `sufficient = false`
- [x] ✅ Update `rules.toml` semantic rules to set `intent_code` in addition to the existing `intent` string — rules file gains an optional `intent_code` field per rule; falls back to `IntentCode::Other(intent_string)` if not set
- [x] ✅ Write deterministic unit tests for each `IntentCode` variant — inject synthetic `BaselineStats` and metric delta inputs; assert expected `intent_code` and `intent_confidence` range without live hardware
- [x] ✅ Update `veyn-mcp` tool descriptions to document `intent_code` field in `veyn_get_context` response schema
- [ ] 🟢 Optional secondary classification pass — if `intent_confidence < 0.6`, invoke a local SLM (`llama.cpp` via FFI or subprocess) with the current metric snapshot as a structured prompt; use the SLM output to override `intent_code` and raise `intent_confidence`; gate behind `compression.use_slm = true` config flag