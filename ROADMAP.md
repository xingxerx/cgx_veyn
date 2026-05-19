# CGX VEYN — Phase Roadmap

-----

## Phase 0 — Foundation ✅

- [x] Workspace structure (`veyn-schemas` / `veyn-adapters` / `veyn-core` / `veyn-plugins` / `veyn-mcp` / `sdk`)
- [x] Unified `VeynEvent` schema
- [x] `VeynAdapter` trait + registry
- [x] Mock adapter (dev / CI)
- [x] HealthKit relay adapter (TCP listener)
- [x] BLE adapter stub (`btleplug`)
- [x] Event bus (`tokio::mpsc`)
- [x] Dispatcher (log + JSONL persist)
- [x] REST API skeleton (Axum)
- [x] iOS Companion App skeleton (Swift / HealthKit)
- [x] Architecture diagram + docs

-----

## Phase 1 — HealthKit Bridge ✅

- [x] iOS companion app: full HealthKit query + background delivery
- [x] iOS companion app: auto-discover daemon on LAN (mDNS / Bonjour)
- [x] iOS companion app: SwiftUI status screen
- [x] Daemon: LMDB state layer (latest value per metric)
- [x] Daemon: SQLite migration runner + history queries
- [x] REST: `GET /metrics/:metric` returns real LMDB value
- [x] REST: `GET /devices` tracks active companion sessions
- [x] README: Getting Started guide

-----

## Phase 2 — BLE Universal Wearable ✅

- [x] BLE adapter: scan + connect to GATT Heart Rate profile
- [x] BLE adapter: decode HR measurement characteristic
- [x] BLE adapter: battery level monitoring
- [x] BLE adapter: device persistence (known devices list)
- [x] Schema: `VeynDevice` LMDB registry

-----

## Phase 3 — Live Streaming ✅

- [x] WebSocket endpoint `GET /stream`
- [x] Broadcast channel (`tokio::broadcast`) wired to dispatcher
- [x] Client reconnect + event replay (last N)
- [x] Simple web dashboard (HTML/JS, served by daemon)

-----

## Phase 4 — Plugin System ✅

- [x] WASM plugin runtime (`wasmtime`)
- [x] Plugin manifest format (TOML)
- [x] Plugin SDK (Rust + `wasm32` target)
- [x] Example plugin: Garmin Connect (OAuth pull stub)
- [x] Example plugin: Whoop API stub

-----

## Phase 5 — Cross-Device Communication ✅

- [x] Notification routing: PC → Apple Watch (via companion TCP relay)
- [x] Gesture / input forwarding: Watch crown / tap → desktop events
- [x] Presence detection: watch heartbeat → PC automation trigger
- [x] Smart home bridge: MQTT output adapter

-----

## Phase 6 — Security & Semantic Compression ✅

- [x] Semantic Compression Engine (`veyn-core/src/compression.rs`)
  - State Reduction Layer: delta filtering, temporal debounce per device class, epsilon magnitude thresholds
  - Semantic Synthesis Engine: rule-based intent classification with hot-reloadable `rules.toml` (30 s, no restart)
  - Output: structured `ContextSnapshot` — `intent`, `confidence`, `active_devices`, `state_deltas`, `session_id`
- [x] Token-based auth: 256-bit random token, stored at `~/.local/share/veyn/token` (chmod 600)
- [x] `Authorization: Bearer <token>` enforced on all REST + WebSocket endpoints
- [x] Host header validation — DNS-rebinding protection
- [x] Strict CORS — deny cross-origin by default, configurable allowlist
- [x] `strip_raw_hid = true` default — semantic intent only, no raw input content exposed
- [x] Audit log at `~/.local/share/veyn/audit.log`
- [x] Versioned `/v1/` API prefix; legacy unversioned routes retained
- [x] `GET /v1/context/current` — current semantic snapshot
- [x] `GET /v1/context/history?n=10` — last N context snapshots (ring buffer, default 32)
- [x] Config priority: CLI flags > env vars > `veyn.toml` > defaults
- [x] Graceful shutdown on SIGTERM / SIGINT
- [x] WebSocket ping/pong keepalive (30 s interval)

-----

## Phase 7 — Platform Hardening ✅

- [x] WASM Device Proxy Layer — plugins declare device descriptors; daemon opens hardware and passes buffers into sandbox
- [x] WASM host-function imports: `veyn::read_device`, `veyn::http_get`, `veyn::time_ms`, `veyn::log`
- [x] Plugin signature verification — reject unsigned `.wasm` binaries unless `allow_unsigned = true`
- [x] `veyn plugin install <path>` CLI subcommand
- [x] Linux `evdev` HID adapter (`/dev/input/event*`)
- [x] Linux `hidraw` adapter (raw USB HID)
- [x] macOS `IOKit` / `IOHIDManager` adapter (`veyn-adapters/src/iokit.rs`, `cfg(target_os = "macos")`)
- [x] Windows `WinUSB` / `RawInput` adapter (`veyn-adapters/src/winusb.rs`, `cfg(target_os = "windows")`)
- [x] MIDI adapter (`midir`) — CC events, note on/off, clock
- [x] Serial/UART adapter (`serialport`) — configurable baud, parity, stop bits
- [x] Filesystem watcher adapter (`notify`)
- [x] Device hot-plug / unplug detection — adapter marks devices `Disconnected` on error; restart with exponential backoff
- [x] Scope-limited tokens — read-only or device-class-scoped access
- [x] Rate limiting on REST endpoints — token bucket, 100 req/s per client IP
- [x] OpenAPI 3.0 spec (`openapi.yaml`) published at `GET /openapi.yaml`
- [x] WebSocket subscribe filtering — client sends filter object on connect

-----

## Phase 8 — Intero Infrastructure ✅

Intero is the first first-party application built on VEYN — a physiological
decision-support tool that measures involuntary biometric response (HRV, heart
rate, EEG, skin temperature) to help users understand their body's reaction to
decisions they are weighing.

### 8.1 — Synchronized Multi-Signal Session Recording ✅

- [x] `Session` type in `veyn-schemas` (`id`, `label`, `notes`, `started_at`, `ended_at`, `active_device_ids`)
- [x] `SessionBoundary` control event — broadcast `Start` / `End` envelopes on the event bus
- [x] `SessionManager` in `veyn-core` — opens/closes sessions, wired into `AppState`
- [x] REST: `POST /v1/session/start`, `POST /v1/session/stop`, `GET /v1/session/{id}`
- [x] WebSocket session framing — `session_frame` envelope during active sessions
- [x] MCP tools: `veyn_start_session`, `veyn_stop_session`, `veyn_get_session`

### 8.2 — Named Sessions with Metadata and Persistence ✅

- [x] SQLite `sessions` table migration (`id`, `label`, `annotation`, `started_at`, `ended_at`, `device_ids`)
- [x] `session_id` column added to `events` table — nullable, set for all events captured during a session
- [x] `GET /v1/session/{id}/replay` — full-resolution event timeline (bypasses compression)
- [x] `PATCH /v1/session/{id}` — update label / annotation post-hoc
- [x] `GET /v1/sessions` — paginated list with summary stats
- [x] `GET /v1/session/{id}/export?format=csv`

### 8.3 — Personal Baseline Tracking ✅

- [x] `BaselineStats` type in `veyn-schemas` (mean, stddev, p10, p90, `updated_at`)
- [x] `BaselineEngine` in `veyn-core` — rolling 30-day window per `(device_id, metric)`, recalculates every 15 min
- [x] `baseline_samples` SQLite table — persists samples across daemon restarts
- [x] `GET /v1/baseline/{device_id}/{metric}` — current stats
- [x] `baseline_delta` map on `ContextSnapshot` — z-score per metric, only when baseline sufficient
- [x] `GET /v1/baseline/{device_id}/{metric}/history?days=30`

### 8.4 — `intent_code` Structured Field ✅

- [x] `IntentCode` enum in `veyn-schemas`: `Neutral`, `CognitiveLoad`, `StressResponse`, `Approach`, `Avoidance`, `Fatigue`, `Recovery`, `Other(String)`
- [x] `intent_code: IntentCode` and `intent_confidence: f32` added to `ContextSnapshot`
- [x] Structured intent classification in `CompressionEngine` — z-score threshold rules per variant
- [x] `rules.toml` extended with optional `intent_code` field per rule
- [x] Unit tests for all `IntentCode` variants with synthetic metric inputs
- [x] Optional SLM integration: secondary classification pass via `llama.cpp` (FFI or subprocess) when `intent_confidence < 0.6`; gated behind `compression.use_slm = true` config flag

-----

## Phase 9 — Biometric Memory Layer ✅

Cross-session physiological memory that lets every AI session start with awareness
of recent biometric history without querying the agent first.

### 9.1 — Schema & Storage ✅

- [x] `MemoryKind` enum in `veyn-schemas`: `Ambient` | `Semantic`
- [x] `MemoryRecord` struct: `id`, `timestamp_ms`, `session_id`, `kind`, `topic`, `summary`,
  `intent_at_time`, `confidence_at_time`, `hrv_at_time`, `hr_at_time`, `context_snapshot`
- [x] `MemoryQuery` struct: `topic`, `since_ms`, `until_ms`, `kind`, `limit`
- [x] SQLite migration: `veyn_memory` table with indexes on `timestamp_ms`, `topic`, `kind`

### 9.2 — Memory Store Module ✅

- [x] `memory.rs` in `veyn-core` — `MemoryStore` wrapping the shared SQLite connection
- [x] `fn write(record: MemoryRecord) -> Result<()>` — persists record, prunes ambient overflow
- [x] `fn query(q: &MemoryQuery) -> Result<Vec<MemoryRecord>>` — filtered retrieval with `NULL`-safe SQL
- [x] `fn summarize_window(since_ms, until_ms) -> Result<String>` — collapses memory records in a window
- [x] `fn summarize_snapshots(snapshots: &[ContextSnapshot]) -> String` — statistical pass for ambient writer

### 9.3 — Ambient Writer Task ✅

- [x] Background tokio task (`memory::ambient_writer`) spawned from `main.rs`
- [x] Fires on configurable interval (default 15 min via `memory.ambient_interval_secs`)
- [x] On each tick: reads context ring buffer, computes avg HRV, avg HR, dominant intent, duration
- [x] Writes one `Ambient` MemoryRecord; skips idle windows (no active devices)
- [x] Oldest ambient records pruned when `max_records` exceeded; semantic records never pruned

### 9.4 — REST Endpoints ✅

- [x] `POST /v1/memory` — body: `{ topic, summary, context_snapshot? }` — writes a Semantic record,
  auto-attaches current physiological state from `latest_context`
- [x] `GET /v1/memory?topic=&since=&until=&kind=&limit=` — returns `Vec<MemoryRecord>` matching query
- [x] `GET /v1/memory/{id}` — retrieve a single memory record by ID (optional feature)
- [x] `DELETE /v1/memory/{id}` — remove a specific memory record (optional feature)
- [x] Both endpoints wired into `routes.rs` and protected by existing auth middleware

### 9.5 — Config ✅

- [x] `[memory]` section in `veyn.toml` and `Config` struct:
  `enabled` (default true), `ambient_interval_secs` (default 900), `max_records` (default 10 000)
- [x] `veyn.toml.example` updated with annotated `[memory]` section

### 9.6 — MCP Tools ✅

- [x] `veyn_write_memory` — args: `topic`, `summary` — POSTs to `/v1/memory`; daemon attaches biometrics
- [x] `veyn_recall_memory` — args: `topic?`, `since?` (human: "7d"/"24h"), `kind?` — GETs `/v1/memory`
- [x] Human-readable `since` strings parsed to Unix ms before forwarding
- [x] Both tools registered in `tool_list()` and dispatched in `dispatch_tool()`

### 9.7 — Session Bootstrap ✅

- [x] On `initialize` handshake: auto-recalls last 24 h of memory (limit 5) from daemon
- [x] Result embedded in `serverInfo.context` — every AI session starts pre-loaded with recent context
- [x] Graceful fallback: if daemon not yet ready, bootstrap is skipped silently

### 9.8 — Tests ✅

- [x] Unit tests: `summarize_window` stats, write/query round-trip, kind filtering, ambient pruning,
  ambient writer fires after one interval tick (async test with 1 s test interval)
- [x] Integration tests: `MemoryRecord` JSON round-trip, `MemoryKind` snake-case serialisation,
  optional fields skip `null` in JSON output

-----

## Phase 10 — Intero App 🔜

The Intero desktop application built on VEYN infrastructure.
Scope defined separately. Phases 8 and 9 are hard prerequisites.

-----

## Phase 10 — SDK & Ecosystem ✅ (Go SDK pending)

- [x] TypeScript/Node.js SDK (`sdk/ts/`) — typed client, HTTP + SSE + WebSocket, zero runtime deps
- [x] Python SDK (`sdk/py/`) — async `aiohttp`/`websockets` client, dataclass types
- [x] SDK usage examples: LLM agent integration, session recording, context reader
- [ ] Go SDK
- [x] `docker-compose.yml` — local dev stack (daemon + mock + example consumer)
- [x] Integration test suite — 34 tests covering schemas, serialisation, plugin utilities
- [x] `veyn doctor` CLI subcommand
- [x] Minimal web UI at `GET /` — live context feed, device list, log tail (dashboard.html)
- [x] Agent Handshake Protocol — Bearer token + tier scope + declarative SSE filter DSL
- [x] `GET /v1/context/subscribe` with declarative filter DSL (`?intents=`, `?min_confidence=`, `?source_class=`)
- [x] `context_tier` config: `raw` | `filtered` | `semantic` — token scope `tier:<value>`, daemon default via `veyn.toml` and `VEYN_CONTEXT_TIER`
- [x] Integration guides: local MCP clients via `veyn-mcp`, Ollama (Gemma 4), Gemma 4 tool calling (`docs/integrations/`)
- [x] Prometheus metrics at `GET /metrics`

-----

## Summary Table

|Phase|Focus                                                               |Status|
|-----|--------------------------------------------------------------------|------|
|0    |Foundation — schemas, adapters, event bus, REST skeleton            |✅     |
|1    |HealthKit bridge — iOS companion, LMDB, SQLite                      |✅     |
|2    |BLE universal wearable — GATT scan, decode, persistence             |✅     |
|3    |Live streaming — WebSocket broadcast, web dashboard                 |✅     |
|4    |WASM plugin system — runtime, SDK, example adapters                 |✅     |
|5    |Cross-device communication — notifications, gestures, presence      |✅     |
|6    |Security + semantic compression — auth, intent engine, versioned API|✅     |
|7    |Platform hardening — HID adapters, WASM proxy, token scopes         |✅     |
|8    |Intero infrastructure — sessions, baselines, `intent_code`          |✅     |
|9    |Biometric memory layer — persistent memory, ambient writer, MCP     |✅     |
|10   |Intero app — physiological decision-support desktop application     |🔜     |
|11   |SDK + ecosystem — language SDKs, DX tooling, agent integration      |✅     |
