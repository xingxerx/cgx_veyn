# VEYN — Master TODO

> **North star:** Body state becomes ambient infrastructure — as available to software as location or voice,
> local, personal, compounding, and actionable. Every task below builds toward that.
>
> Priority: 🔴 Critical (blocks launch) → 🟡 High → 🟢 Nice-to-have
> Build order within each section is top-to-bottom unless noted.

---

## Already complete ✅

Sections 1–10 below are finished. They form the substrate everything else builds on.

---

## 1. ✅ Semantic Compression Engine

- [x] State Reduction Layer — delta filtering, temporal debounce, epsilon magnitude thresholds
- [x] Semantic Synthesis Engine — rule-based intent classification, hot-reloadable `rules.toml` (30 s)
- [x] `ContextSnapshot` output — `intent`, `confidence`, `active_devices`, `state_deltas`, `timestamp_ms`
- [x] Optional SLM secondary pass via `llama.cpp` when `confidence < 0.6`

---

## 2. ✅ Security

- [x] 256-bit Bearer token, stored at `~/.local/share/veyn/token` (chmod 600)
- [x] `Authorization: Bearer <token>` enforced on all REST + WebSocket endpoints
- [x] Host header validation — DNS-rebinding protection
- [x] Strict CORS, `strip_raw_hid = true`, audit log
- [x] Scope-limited tokens — read-only or device-class-scoped access
- [x] Rate limiting — token bucket, 100 req/s per client IP
- [ ] 🟢 Mutual TLS (mTLS) option for high-security deployments

---

## 3. ✅ WASM Plugin Architecture

- [x] Device Proxy Layer — plugins declare descriptors; daemon opens hardware, passes buffers into sandbox
- [x] Host imports: `veyn::read_device`, `veyn::http_get`, `veyn::time_ms`, `veyn::log`
- [x] Plugin signature verification; `veyn plugin install` CLI subcommand
- [x] Plugin SDK (Rust + `wasm32` target); example plugins: Garmin Connect, Whoop

---

## 4. ✅ Adapters

- [x] Mock, BLE, OSC/EEG, HealthKit relay, MQTT, evdev, hidraw, IOKit, WinUSB, MIDI, serial, filesystem

---

## 5. ✅ API Contract

- [x] REST, WebSocket, SSE — versioned `/v1/`, filter DSL, `context_tier` config
- [x] OpenAPI 3.0 spec at `GET /openapi.yaml`

---

## 6. ✅ SDKs

- [x] TypeScript, Python, Rust guest plugin SDKs; usage examples
- [ ] 🟢 Go SDK — for server-side agent integrations

---

## 7. ✅ Developer Experience

- [x] `docker-compose.yml`, integration test suite (34 tests), `veyn doctor`, web dashboard UI

---

## 8. ✅ AI Integration Prep

- [x] Agent Handshake Protocol, declarative SSE filter DSL, `context_tier` scopes
- [x] Integration guides: MCP, Ollama, Gemma 4 tool calling

---

## 9. ✅ Intero Infrastructure

- [x] Session recording, named sessions + SQLite persistence, session replay + CSV export
- [x] `BaselineEngine` — 30-day rolling z-scores per `(device_id, metric)`
- [x] `IntentCode` enum + `intent_confidence` on `ContextSnapshot`

---

## 10. ✅ Biometric Memory Layer

- [x] `MemoryStore` — ambient writer every 15 min, semantic records, `POST/GET /v1/memory`
- [x] MCP tools: `veyn_write_memory`, `veyn_recall_memory`
- [x] Session bootstrap — last 24 h of memory embedded in `serverInfo.context` on `initialize`

---
---

## What we're building toward

> **The physiological OS.** Body state as ambient infrastructure — like GPS for location, like the
> microphone for voice. Five expressions of the same shift:
>
> 1. **Body-aware computing** — every app on the machine reads the same context bus
> 2. **Decisions with receipts** — somatic record of every significant choice, replayable
> 3. **AI that already knows** — biometric memory pre-loaded before the first message
> 4. **Environment that responds** — state-driven automation, not schedules
> 5. **Accuracy that compounds** — z-scores against *your* baseline, improving over time

Build order for the remaining work: **11 → 12 → 13 → 14 → 15 → 16**

---

## 11. 🔴 Critical: Intero App — the anchor product

> Delivers: **decisions with receipts** · **accuracy that compounds** (first user-facing proof of both)

The first-party app that makes VEYN tangible. Without it, the platform has no front door.
Build order within: 11.1 → 11.2 → 11.3 → 11.4 → 11.5

### 11.1 Desktop shell (`intero/`)

- [ ] Choose Tauri (Rust backend + WebView) — ELv2-aligned, native OS integration, no Electron
- [ ] Scaffold `intero/` at workspace root; add to Cargo workspace members
- [ ] Auto-launch VEYN daemon on app start if not running; detect via `GET /v1/health`; show "Starting…" state while daemon initialises
- [ ] Secure token handoff — read `~/.local/share/veyn/token` on startup; never transmit outside localhost
- [ ] System tray — icon reflects current `intent_code` (color-coded); right-click for quick actions (start session, view context)

### 11.2 Live biometric dashboard

- [ ] Subscribe to `GET /v1/context/subscribe` (SSE); render `intent_code`, `intent_confidence`, per-metric `baseline_delta` z-scores live
- [ ] Scrolling signal strip chart per active device/metric — render from SSE, no client-side storage beyond the visible window
- [ ] Baseline sufficiency indicator — surface `sufficient = false` per metric when < 7 days of baseline data exist
- [ ] Device panel — active devices, adapter type, last-seen timestamp, battery level where available

### 11.3 Decision session UX

- [ ] Start session flow — prompt for decision label + optional annotation; `POST /v1/session/start`; lock UI into session mode
- [ ] In-session view — live multi-channel strips synced to session start time; running `intent_code` + `intent_confidence`; prominent elapsed timer
- [ ] Stop session flow — `POST /v1/session/stop`; transition immediately to session review
- [ ] Session review — full-resolution replay via `GET /v1/session/{id}/replay`; intent overlaid on timeline; export via `GET /v1/session/{id}/export?format=csv`
- [ ] Session library — paginated list from `GET /v1/sessions`; filter by date range and intent; open any session in review

### 11.4 Somatic feedback layer

- [ ] Approach / Avoidance indicator — prominent live element showing body's lean toward or away from the decision being considered; updates continuously during session
- [ ] Confidence-gated display — suppress intent classification UI when `intent_confidence < 0.4`; show "calibrating…" state instead of a potentially misleading signal
- [ ] Historical pattern panel — for the current decision topic, surface similar past sessions via `GET /v1/memory?topic=`; show what the body did then
- [ ] Baseline delta sparklines — per-metric z-score trend for the last 7 days; visible in session review to contextualise the session signal

### 11.5 Ambient background mode

- [ ] Background mode — Intero stays in tray when no session is active; ambient writer continues; no active UI required
- [ ] Daily digest — optional morning notification summarising prior day's dominant intent distribution + HRV trend vs baseline; generated from memory records, no live signal required
- [ ] Fatigue / recovery nudges — when `Fatigue` persists > N minutes (configurable), surface a gentle notification; acknowledge `Recovery` post-fatigue; all gated behind user preference toggle

---

## 12. 🔴 Critical: Body-aware computing layer

> Delivers: **body-aware computing** · **AI that already knows**

The infrastructure that turns VEYN from a single product into a platform.
Build order: 12.1 → 12.2 → 12.3

### 12.1 Multi-client subscription layer

- [ ] `client_id` concept — each connecting app registers with a name + capability tier; stored in session; surfaced in audit log
- [ ] Per-client namespaced subscriptions — extend filter DSL so a DAW plugin, a writing assistant, and a focus timer coexist on the same daemon without crosstalk
- [ ] `GET /v1/clients` — list currently connected subscribers with their filter config and tier; for debugging multi-app setups

### 12.2 Adaptive AI agent integration

- [ ] Body-aware agent spec (`docs/integrations/body-aware-agent.md`) — canonical pattern: subscribe to SSE, branch on `intent_code`, adjust behavior, write `MemoryRecord` on significant events
- [ ] `veyn_suggest_action` MCP tool — agent proposes a next action; VEYN returns whether current physiology is congruent (e.g. flag a complex code review during `Fatigue`)
- [ ] `context_degraded` SSE event — emitted when `intent_confidence` drops below threshold mid-session; consuming agents fall back to neutral behavior; document the event shape in OpenAPI
- [ ] Ollama reference agent (`sdk/examples/body_aware_agent.py`) — Gemma 4 loop that adapts response verbosity based on live `CognitiveLoad`; runnable out of the box against a mock daemon

### 12.3 Ambient state broadcast

- [ ] `GET /v1/context/broadcast` — lightweight SSE endpoint, loopback-only (OS-enforced), no auth required; enables zero-config integration for trusted local apps
- [ ] App registration manifest — optional `veyn-app.toml` a local app ships to declare its name, tier, and filter preferences; daemon picks it up on connect

---

## 13. 🔴 Critical: Environment response layer

> Delivers: **environment that responds to state, not a schedule**

### 13.1 MQTT rules engine

- [ ] `[mqtt_output]` section in `rules.toml` — rules map `intent_code` transitions to MQTT topic + payload pairs
  - Example: `StressResponse` → publish `veyn/home/scene` = `"calm"`
  - Example: `Recovery` → publish `veyn/home/scene` = `"energise"`
  - Example: `Fatigue` persisting > 20 min → publish `veyn/home/lights` = `"dim"`
- [ ] Transition debounce per rule — rules fire only after `intent_code` is stable for N seconds (configurable per rule); prevents flapping on noisy signal
- [ ] `GET /v1/rules/status` — list active rules, last-fired timestamps, current MQTT connection state

### 13.2 Home Assistant integration

- [ ] Integration guide (`docs/integrations/home-assistant.md`) — MQTT broker config, recommended automation YAML, scene mapping conventions
- [ ] Mock MQTT broker in `docker-compose.yml` — log all published messages; rules verified in CI without live hardware

### 13.3 Environment feedback loop tooling

- [ ] `POST /v1/rules/simulate` — inject a synthetic `intent_code` transition, return what MQTT messages would be published; for user testing without live hardware
- [ ] Rule hot-reload for MQTT section — `rules.toml` changes propagate to MQTT rules within 30 s, alongside existing compression rule reload (no restart required)

---

## 14. 🟡 High: Longitudinal analysis pipeline

> Delivers: **accuracy that compounds** · research use cases

### 14.1 Batch export API

- [ ] `GET /v1/export?since=&until=&format=csv|jsonl` — all events (or session-scoped) for a time window; bypasses compression; full resolution
- [ ] `GET /v1/sessions/compare?ids=id1,id2` — aligned multi-channel timelines + summary stats delta between two sessions; for "this vs last time I faced a similar decision"

### 14.2 Baseline intelligence

- [ ] Baseline drift alert — `BaselineEngine` emits a `baseline_drift` SSE event when a metric's 7-day mean deviates > 1.5 σ from its 30-day mean; surfaced in Intero dashboard and queryable via MCP
- [ ] `GET /v1/baseline/summary` — cross-metric overview: which metrics have sufficient data, which are drifting, which are stable; useful as a health check

### 14.3 Research notebooks

- [ ] `sdk/examples/notebooks/hrv_longitudinal.ipynb` — load batch export, compute 30-day baseline trends, visualise intent distribution over time
- [ ] `sdk/examples/notebooks/session_retrospective.ipynb` — load a session, overlay intent on signal timeline, compare to baseline, export figures

---

## 15. 🟡 High: WASM plugin ecosystem

> Delivers: compounding signal breadth — more plugins = more signal types = richer intent classification over time

### 15.1 Plugin registry

- [ ] `registry.toml` schema — name, version, author, description, verified flag, `.wasm` URL, sha256; ship example at `plugins/registry.toml`
- [ ] `veyn plugin search <keyword>` — reads local + optional remote registry URL (configurable); lists matching plugins with verified status
- [ ] `veyn plugin verify <path>` — checks sha256 + signature without installing; for user trust inspection before committing

### 15.2 Plugin authoring

- [ ] Third-party authoring guide (`docs/plugins/authoring.md`) — scaffold with SDK, device descriptor declaration, host imports, registry publication; Garmin example as the worked case
- [ ] Capability declaration in `plugin.toml` — `[capabilities]` section listing which host functions the plugin uses; daemon rejects plugins claiming undeclared capabilities at install time

### 15.3 Signal breadth

- [ ] Audio level adapter (`cpal`) — RMS/peak from default input device; emit as `VeynEvent` with `source = "audio"`; useful for voice/environment sensing
- [ ] OSC output adapter — send OSC downstream to DAW/VJ software; complement to existing OSC input adapter; configurable target host:port in `veyn.toml`
- [ ] Go SDK (`sdk/go/`) — idiomatic Go client; HTTP + SSE + WebSocket; typed structs matching `veyn-schemas`; target: server-side agent integrations

---

## 16. 🟢 Platform hardening

These don't unlock new possibilities — they make the existing ones production-grade.
Can run in parallel with any phase above.

- [ ] Mutual TLS (mTLS) — `[tls]` section in `veyn.toml`; cert generation documented in `INSTALL.md`
- [ ] Structured log output — `VEYN_LOG_FORMAT=json`; `tracing` spans emitted as JSON for log aggregation pipelines
- [ ] WASM guest resource limits — cap memory + CPU time via `wasmtime::Config`; prevent runaway plugins from starving the event loop
- [ ] Expanded Prometheus metrics at `GET /metrics` — per-adapter event rates, compression engine latency, memory store write latency, WebSocket client count
- [ ] `veyn benchmark` CLI subcommand — inject N mock events/s; report compression throughput, p99 latency, memory usage; for sizing deployments
- [ ] Cross-platform CI matrix — macOS + Linux + Windows smoke tests in GitHub Actions; flag platform-specific adapter regressions

---

## Build path summary

```
11 Intero app              → front door; decisions with receipts; first proof of compounding accuracy
12 Body-aware layer        → every app reads the bus; AI already knows
13 Environment layer       → physical world responds to state, not schedules
14 Longitudinal pipeline   → accuracy compounds; research unlocked; baseline intelligence
15 Plugin ecosystem        → signal breadth flywheel; third-party contribution model
16 Hardening               → production-grade reliability (runs in parallel throughout)
```

Phase 11 alone is a complete, shippable product.
Phases 12–15 each extend what "body state as ambient infrastructure" means in practice.
The unified possibility — the physiological OS — is fully realised when all five are live.
