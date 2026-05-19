# VEYN — Master TODO

> Complete work backlog across all crates and subsystems.
> Priority: 🔴 Critical (blocks launch) → 🟡 High (needed for safety/correctness) → 🟢 Nice-to-have
> Build order within each section is top-to-bottom unless noted.

---

## 1. 🔴 Critical: Semantic Compression Engine

- [x] ✅ State Reduction Layer — delta filtering, temporal debounce, epsilon magnitude thresholds (`veyn-core/src/compression.rs`)
- [x] ✅ Semantic Synthesis Engine — rule-based intent classification with hot-reloadable `rules.toml` (30 s interval, no restart required)
- [x] ✅ `ContextSnapshot` output — `intent`, `confidence`, `active_devices`, `state_deltas`, `timestamp_ms`, `session_id`
- [x] ✅ 🟢 Optional: lightweight local SLM integration (e.g. `llama.cpp` via FFI or subprocess) as a secondary synthesis pass for ambiguous intent classification where `confidence < 0.6`

---

## 2. 🔴 Critical: Security

- [x] ✅ 256-bit random token generated on first run, stored at `~/.local/share/veyn/token` (chmod 600)
- [x] ✅ `Authorization: Bearer <token>` required on all REST and WebSocket endpoints when `require_auth = true`
- [x] ✅ Host header validation — blocks DNS-rebinding attacks
- [x] ✅ Strict CORS — deny cross-origin by default, configurable allowlist in `veyn.toml`
- [x] ✅ `strip_raw_hid = true` default — only semantic intent exposed, not raw input content
- [x] ✅ Audit log at `~/.local/share/veyn/audit.log`
- [x] ✅ Scope-limited tokens — read-only or device-class-scoped access
- [x] ✅ Rate limiting on REST endpoints — token bucket, max 100 req/s per client IP
- [ ] 🟢 Consider mutual TLS (mTLS) option for high-security deployments

---

## 3. 🔴 Critical: WASM Plugin Architecture

- [x] ✅ WASM Device Proxy Layer — plugins declare device descriptors; daemon opens hardware and passes buffers into sandbox
- [x] ✅ WASM host-function imports: `veyn::read_device`, `veyn::http_get`, `veyn::time_ms`, `veyn::log`
- [x] ✅ Plugin signature verification — reject unsigned `.wasm` binaries unless `allow_unsigned = true`
- [x] ✅ `veyn plugin install <path>` CLI subcommand
- [x] ✅ Plugin SDK (Rust + `wasm32` target)
- [x] ✅ Example plugin: Garmin Connect (OAuth pull stub)
- [x] ✅ Example plugin: Whoop API stub

---

## 4. 🟡 High: `veyn-adapters` — Platform Coverage

- [x] ✅ Mock adapter — synthetic event generator for dev and CI
- [x] ✅ BLE adapter — GATT Heart Rate Profile scan + decode + battery monitoring + device persistence
- [x] ✅ OSC/EEG adapter — UDP OSC input stream (Mind Monitor compatible)
- [x] ✅ HealthKit TCP Relay — iOS companion bridge with mDNS auto-discovery
- [x] ✅ MQTT adapter — IoT / smart home output
- [x] ✅ Linux: `evdev` HID adapter
- [x] ✅ Linux: `hidraw` adapter — raw USB HID
- [x] ✅ macOS: `IOKit`/`IOHIDManager` adapter
- [x] ✅ Windows: `WinUSB` / `RawInput` adapter
- [x] ✅ MIDI adapter (`midir`) — CC events, note on/off, clock
- [x] ✅ Serial/UART adapter (`serialport`)
- [x] ✅ Filesystem watcher adapter (`notify`)
- [ ] 🟢 OSC output adapter — send OSC messages to DAW/VJ software as a downstream sink
- [ ] 🟢 Audio level adapter (`cpal`) — RMS/peak metering from default input device

---

## 5. 🟡 High: `/context/current` API Contract

- [x] ✅ `GET /v1/context/current` — current semantic snapshot
- [x] ✅ `GET /v1/context/history?n=10` — last N snapshots (ring buffer, max 32)
- [x] ✅ Versioned `/v1/` prefix with legacy unversioned routes retained
- [x] ✅ OpenAPI 3.0 spec (`openapi.yaml`) at `GET /openapi.yaml`
- [x] ✅ WebSocket subscribe filtering — declarative filter DSL on connect
- [x] ✅ SSE alternative at `GET /v1/context/subscribe`

---

## 6. 🟡 High: SDK

- [x] ✅ Rust guest-side plugin SDK (`veyn-plugin-sdk`) with WASM ABI macros
- [x] ✅ TypeScript/Node.js SDK — typed `VeynEvent` and `ContextSnapshot`; async `subscribe()` over WebSocket; Bearer auth
- [x] ✅ Python SDK — async context manager; typed dataclasses; example Jupyter notebook
- [x] ✅ SDK usage examples: LLM agent integration, WASM plugin, context history reader
- [ ] 🟢 Go SDK — for server-side agent integrations

---

## 7. 🟢 Developer Experience

- [x] ✅ `docker-compose.yml` — daemon + mock adapter + example consumer agent
- [x] ✅ Integration test suite — 34 tests covering schemas, serialisation, plugin utilities
- [x] ✅ `veyn doctor` CLI subcommand — structured pass/fail prerequisite report
- [x] ✅ Minimal web UI at `GET /` — live context feed, device list, log tail (`dashboard.html`)

---

## 8. 🟢 AI Integration Prep

- [x] ✅ Agent Handshake Protocol — Bearer token + `tier:semantic` scope + SSE/WebSocket filter DSL
- [x] ✅ `GET /v1/context/subscribe` with declarative filter DSL
- [x] ✅ `context_tier` config: `raw` | `filtered` | `semantic`
- [x] ✅ Integration guides: MCP (`claude-mcp.md`), Ollama (Gemma 4), Gemma 4 tool calling

---

## 9. 🔴 Critical: Intero Infrastructure

Phases 9.1 → 9.2 → 9.3 → 9.4 must be completed in order.

> ⚠️ `AppState::session_id` is a daemon-lifecycle UUID. `InteroSession` is a user-initiated named session. Do not conflate them.

### 9.1 Synchronized Multi-Signal Session Recording

- [x] ✅ `InteroSession` and `SessionBoundary` types in `veyn-schemas`
- [x] ✅ `SessionManager` in `veyn-core`
- [x] ✅ REST: `POST /v1/session/start`, `POST /v1/session/stop`, `GET /v1/session/{id}`
- [x] ✅ WebSocket `session_frame` envelope during active sessions
- [x] ✅ MCP tools: `veyn_start_session`, `veyn_stop_session`, `veyn_get_session`

### 9.2 Named Sessions with Metadata and Persistence

- [x] ✅ SQLite `sessions` table migration
- [x] ✅ `session_id` column on `events` table (nullable)
- [x] ✅ `GET /v1/session/{id}/replay` — full-resolution event timeline
- [x] ✅ `PATCH /v1/session/{id}` — post-hoc label/annotation update
- [x] ✅ `GET /v1/sessions` — paginated list with summary stats
- [x] ✅ `GET /v1/session/{id}/export?format=csv`

### 9.3 Personal Baseline Tracking

- [x] ✅ `BaselineStats` type in `veyn-schemas`
- [x] ✅ `BaselineEngine` — rolling 30-day window per `(device_id, metric)`, recalculates every 15 min
- [x] ✅ `baseline_samples` SQLite table
- [x] ✅ `GET /v1/baseline/{device_id}/{metric}` and `/history?days=30`
- [x] ✅ `baseline_delta` z-score map on `ContextSnapshot`

### 9.4 `intent_code` Structured Field

- [x] ✅ `IntentCode` enum in `veyn-schemas`
- [x] ✅ `intent_code: IntentCode` and `intent_confidence: f32` added to `ContextSnapshot`
- [x] ✅ Structured z-score intent classification in `CompressionEngine`
- [x] ✅ `rules.toml` extended with optional `intent_code` per rule
- [x] ✅ Unit tests for all `IntentCode` variants

---

## 10. 🔴 Critical: Biometric Memory Layer

- [x] ✅ `MemoryRecord` and `MemoryKind` types in `veyn-schemas`
- [x] ✅ `MemoryStore` in `veyn-core` — SQLite-backed, ambient writer every 15 min
- [x] ✅ `POST /v1/memory` and `GET /v1/memory` endpoints
- [x] ✅ `veyn_write_memory` and `veyn_recall_memory` MCP tools
- [x] ✅ Session bootstrap — last 24 h of memory embedded in `serverInfo.context` on `initialize`

---

## 11. 🔴 Critical: Intero App (Phase 11)

The first-party physiological decision-support desktop application. Requires phases 9 and 10.

### 11.1 Intero Desktop Shell

- [ ] Choose and configure desktop framework — Tauri (Rust backend + WebView frontend) preferred for ELv2 alignment and native OS integration; no Electron
- [ ] Scaffold Tauri app: `intero/` directory at workspace root; add to Cargo workspace members
- [ ] Auto-launch VEYN daemon on app start if not already running; detect via `GET /v1/health`; show "Starting…" state in UI while daemon initialises
- [ ] System tray integration — icon reflects current `intent_code` (color-coded); click to open main window; right-click for quick actions (start session, view context)
- [ ] Secure token handoff — app reads `~/.local/share/veyn/token` on startup; never transmits token outside localhost

### 11.2 Live Biometric Dashboard

- [ ] Real-time `ContextSnapshot` display — subscribe to `GET /v1/context/subscribe` (SSE); render `intent_code`, `intent_confidence`, and per-metric `baseline_delta` z-scores
- [ ] Signal strip chart — scrolling time-series per active device/metric; render from SSE stream without storing data client-side beyond the visible window
- [ ] Baseline sufficiency indicator — surface `sufficient = false` warning per metric when < 7 days of baseline data exist
- [ ] Device panel — list active devices from `active_devices`; show adapter type, last-seen timestamp, battery level where available

### 11.3 Decision Session UX

- [ ] "Start session" flow — prompt for decision label and optional annotation; `POST /v1/session/start`; lock UI into session mode
- [ ] In-session view — live multi-channel signal strips synchronized to session start time; display running `intent_code` and `intent_confidence`; prominent elapsed timer
- [ ] "Stop session" flow — `POST /v1/session/stop`; transition to session review immediately
- [ ] Session review view — full-resolution replay via `GET /v1/session/{id}/replay`; timeline with intent overlaid; exportable via `GET /v1/session/{id}/export?format=csv`
- [ ] Session library — paginated list from `GET /v1/sessions`; filter by date range and intent; open any session in review view

### 11.4 Somatic Feedback Layer

- [ ] Approach/Avoidance indicator — prominent visual element showing real-time `Approach` vs `Avoidance` signal relative to the decision being considered; updates live during session
- [ ] Confidence-gated display — suppress intent classification UI when `intent_confidence < 0.4`; show "calibrating…" state instead of potentially misleading signal
- [ ] Historical pattern panel — for the current decision topic, surface similar past sessions from memory via `GET /v1/memory?topic=`; show what the body did then

### 11.5 Ambient Context Layer

- [ ] Background mode — when no session is active, Intero remains running in tray; ambient writer continues; no active UI required
- [ ] Daily digest — optional notification each morning summarising prior day's dominant intent distribution and HRV trend vs baseline; generated from memory records, no live signal required
- [ ] Focus/recovery nudges — when `Fatigue` persists > N minutes (configurable), surface a gentle notification; when `Recovery` is detected post-fatigue, acknowledge it; all gated behind user preference toggle

---

## 12. 🔴 Critical: Body-Aware Computing Layer (Platform Vision)

This phase fuses all six capability pillars into a unified ambient computing platform where every piece of software on the machine can optionally read physiological context.

### 12.1 Universal Context API — Per-App Subscriptions

- [ ] Introduce `client_id` concept — each connecting application registers with a name and capability tier; stored in memory for the session; surfaced in audit log
- [ ] Per-app filtered subscriptions — extend the filter DSL to support `client_id` namespacing so the same daemon serves a DAW plugin, a writing assistant, and a focus timer simultaneously without crosstalk
- [ ] `GET /v1/clients` — list currently connected subscribers with their filter configuration and tier; useful for debugging multi-app setups

### 12.2 Adaptive AI Agent Integration

- [ ] Publish reference agent integration spec (`docs/integrations/body-aware-agent.md`) — documents the canonical pattern: subscribe to SSE, branch on `intent_code`, adjust behavior, write `MemoryRecord` on significant events
- [ ] Claude MCP profile extension — extend `veyn-mcp` with `veyn_suggest_action` tool that accepts the agent's proposed next action and returns whether current physiological state is congruent (e.g. don't surface a complex review when `Fatigue` is active)
- [ ] Ollama agent loop example — runnable Python script demonstrating a Gemma 4 agent that adapts its response verbosity based on live `CognitiveLoad` signal; ship in `sdk/examples/`
- [ ] Agent state handoff — when `intent_confidence` drops below threshold mid-session, agents should receive a `context_degraded` SSE event and fall back to neutral behavior; implement event type and document it

### 12.3 Smart Environment Automation

- [ ] MQTT output rules engine — extend `rules.toml` with an `[mqtt_output]` section; rules map `intent_code` transitions to MQTT topic/payload pairs; daemon publishes automatically on transition
  - Example: `StressResponse` → publish `veyn/home/scene` = `"calm"`
  - Example: `Recovery` → publish `veyn/home/scene` = `"energise"`
- [ ] Home Assistant integration guide (`docs/integrations/home-assistant.md`) — documents MQTT broker config, recommended automation YAML, and scene mapping conventions
- [ ] Environment feedback loop test harness — mock MQTT broker in `docker-compose.yml`; log all published messages; verify rules fire correctly in CI without live hardware

### 12.4 Research & Longitudinal Analysis Pipeline

- [ ] Batch export endpoint — `GET /v1/export?since=&until=&format=csv|jsonl` — exports all events (or session-scoped events) for a time window; bypasses compression; for research use
- [ ] Jupyter notebook: longitudinal HRV analysis — ships in `sdk/examples/notebooks/`; demonstrates loading batch export, computing 30-day baseline trends, and visualising intent distribution over time
- [ ] Baseline drift alert — `BaselineEngine` publishes a `baseline_drift` SSE event when a metric's 7-day mean deviates > 1.5 σ from its 30-day mean; surfaces in Intero dashboard and is queryable via MCP
- [ ] Session comparison API — `GET /v1/sessions/compare?ids=id1,id2` — returns aligned multi-channel timelines and summary stats delta between two sessions; useful for "this vs last time I faced a similar decision"

### 12.5 WASM Plugin Ecosystem

- [ ] Plugin registry format — define `registry.toml` schema: name, version, author, description, verified flag, `.wasm` URL, sha256; ship example registry at `plugins/registry.toml`
- [ ] `veyn plugin search <keyword>` CLI subcommand — reads local and optionally a remote registry URL (configurable); lists matching plugins
- [ ] `veyn plugin verify <path>` CLI subcommand — checks sha256 and signature without installing; for user trust inspection
- [ ] Third-party plugin authoring guide (`docs/plugins/authoring.md`) — end-to-end walkthrough: scaffold with `sdk`, declare device descriptors, use host imports, publish to registry; include the Garmin example as the worked case
- [ ] Plugin capability declaration — extend `plugin.toml` manifest with `[capabilities]` section declaring which host functions the plugin uses (`http_get`, `read_device`, `time_ms`, `log`); daemon rejects plugins claiming capabilities not listed at install time

### 12.6 Cross-Platform Parity

- [ ] Verify full adapter matrix on all three platforms (macOS, Linux, Windows) in CI — matrix job in GitHub Actions; `VEYN_MOCK=true` smoke test + adapter unit tests; flag any platform-specific regressions
- [ ] Audio level adapter (`cpal`) — RMS/peak from default input device; useful for voice/environment sensing; emit as `VeynEvent` with `source = "audio"`
- [ ] OSC output adapter — send OSC messages downstream to DAW/VJ software; complement to existing OSC input adapter; configurable target host:port in `veyn.toml`
- [ ] Go SDK (`sdk/go/`) — idiomatic Go client; HTTP + SSE + WebSocket; typed structs matching `veyn-schemas`; target use case: server-side agent integrations running alongside the daemon

---

## 13. 🟢 Platform Hardening & Operations

- [ ] 🟢 Mutual TLS (mTLS) option — for high-security deployments; `[tls]` section in `veyn.toml`; document cert generation in `INSTALL.md`
- [ ] 🟢 Structured log output — `VEYN_LOG_FORMAT=json` env flag; emit `tracing` spans as JSON for log aggregation pipelines
- [ ] 🟢 Plugin sandbox resource limits — cap WASM guest memory and CPU time via `wasmtime::Config`; prevent runaway plugins from starving the daemon event loop
- [ ] 🟢 Prometheus metrics — expand `GET /metrics` coverage to include per-adapter event rates, compression engine latency, memory store write latency, and WebSocket client counts
- [ ] 🟢 `veyn benchmark` CLI subcommand — inject N mock events/s via the mock adapter; report compression engine throughput, p99 latency, and memory usage; useful for sizing deployments
