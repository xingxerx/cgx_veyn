# VEYN — Master TODO

> Roadmap to a perfect, runnable-out-of-the-box build.
> Priority: 🔴 Critical (blocks launch) → 🟡 High (needed for safety/correctness) → 🟢 Nice-to-have

-----

## 1. 🔴 Critical: Security Hardening (Remaining Gaps)

Auth middleware, CORS, host-guard, audit log, and `strip_raw_hid` are implemented.
The following gaps remain:

- [x] 🟡 Add **scope-limited tokens** — `tokens.json` supports per-token scopes: `"read"` (GET only) and `"source:<class>"` (filter by adapter type e.g. `"source:ble"`, `"source:midi"`)
- [x] 🟡 Add **rate limiting** on REST endpoints — token-bucket per client IP via `governor` crate (configurable via `rate_limit_rps` in `[security]`, default 100 req/s)
- [ ] 🟢 Consider **mutual TLS (mTLS)** option for high-security deployments

-----

## 2. 🔴 Critical: WASM Plugin Architecture — Device Proxy Layer

The WASM runtime loads plugins and exposes `veyn::log`, `veyn::time_ms`, and `veyn::http_get` host imports. Plugins that need direct hardware access (HID, BLE, Serial) have no path to it — the proxy layer is missing entirely.

- [ ] 🔴 Add a **Device Proxy Layer** — plugins register a device descriptor (VID/PID, BLE UUID, serial pattern); core handles OS-level device open and passes byte buffers into the WASM sandbox via a new `veyn::device_read` host import
- [ ] 🟡 Add **plugin signature verification** — each plugin `.wasm` binary must be signed; core validates on load before instantiation
- [ ] 🟡 Add a `veyn plugin install <path>` CLI subcommand
- [ ] 🟢 Publish reference plugin: `veyn-plugin-midi-launchpad`

-----

## 3. 🔴 Critical: `ContextSnapshot` Intent Schema

- [x] 🔴 Add a structured `intent_code` field — machine-readable enum (`"resting"`, `"active"`, `"stressed"`, `"idle"`, `"focus"`, `"recovery"`, `"health_concern"`, `"observing"`) so agents can branch without NLP
- [x] 🟡 Add a `source_class` field to `StateDelta` — lets agents filter deltas by adapter type (`ble`, `mqtt`, `plugin`, etc.) without inspecting `device_id`
- [x] 🟡 Ship a default `rules.toml` that covers non-biometric signals (MQTT, filesystem, MIDI) so intent synthesis works out-of-the-box beyond heart rate

-----

## 4. 🟡 High: `veyn-core` Daemon Stability

- [x] 🟡 Implement **device hot-plug/unplug detection** — `spawn_adapter` now wraps every adapter in an exponential-backoff retry loop (1s → 2s → … → 60s max); adapters self-recover on disconnect
- [x] 🟡 The dispatcher locks `latest_metrics` twice per event — consolidated into a single lock scope in `dispatcher.rs`
- [ ] 🟢 Add Prometheus metrics endpoint `GET /metrics` for Grafana integration

-----

## 5. 🟡 High: `veyn-adapters` — Platform Coverage

- [x] 🔴 Linux: `evdev` HID adapter — keyboard, mouse, gamepad via `/dev/input/event*`
- [x] 🔴 Linux: `hidraw` adapter — raw USB HID via `/dev/hidraw*`
- [ ] 🟡 macOS: `IOKit`/`IOHIDManager` adapter
- [ ] 🟡 Windows: `WinUSB`/`RawInput` adapter
- [x] 🟡 MIDI adapter (`midir` crate) — CC events, note on/off, clock
- [x] 🟡 Serial/UART adapter (`serialport` crate) — configurable baud, parity, stop bits
- [x] 🟡 Filesystem watcher adapter (`notify` crate) — emit events on file create/modify/delete for specified paths
- [ ] 🟢 Audio level adapter — RMS/peak metering from default input device (`cpal` crate)

-----

## 6. 🟡 High: API Contract

- [x] 🔴 Implement `GET /v1/context/subscribe` (SSE) — declarative filter DSL (`intents`, `min_confidence`, `source_class` query params) as documented
- [x] 🟡 WebSocket `/stream`: support client-sent subscribe messages to filter by device class or metric name — send `{"type":"subscribe","filter":{"device_class":["ble"],"metrics":["heart_rate"]}}` at runtime
- [x] 🟡 Add OpenAPI 3.0 spec (`openapi.yaml`) and keep it in sync with `routes.rs`

-----

## 7. 🟡 High: SDK

The Rust guest SDK (`/sdk`) is complete. No other language SDKs exist.

- [ ] 🟡 TypeScript/Node.js SDK — auth, typed `ContextSnapshot` interface, reconnecting WS subscriber
- [ ] 🟡 Python SDK — `pip install veyn-sdk`, async context manager, typed dataclasses
- [ ] 🟡 SDK usage examples: connecting an LLM agent, reading context history, building a plugin
- [ ] 🟢 Go SDK

-----

## 8. 🟡 High: Testing

- [x] 🟡 Add unit tests for `CompressionEngine` — debounce logic, epsilon filtering, rule matching, compression ratio calculation (12 tests in `compression.rs`)
- [ ] 🟡 Add integration test harness — spin up daemon in mock+no-auth mode, inject synthetic events via the mock adapter, assert `/v1/context/current` output matches expected intent
- [x] 🟡 Add auth middleware tests — token extraction from header/query, read-only scope enforcement, scoped token source filtering (6 tests in `auth.rs`)
- [ ] 🟢 Add CI step to run `cargo test --workspace`

-----

## 9. 🟢 Developer Experience

- [x] 🟢 Add `docker-compose.yml` for a local dev stack (daemon in mock mode + example consumer agent)
- [ ] 🟢 Add `veyn doctor` CLI subcommand — checks Rust toolchain, system deps, device permissions, token validity, daemon reachability
- [ ] 🟢 Dashboard: add intent history sparkline and compression ratio gauge to the existing live feed UI

-----

## 10. 🟢 AI Integration Prep

- [ ] 🟡 Add `context_tier` config: `raw` | `filtered` | `semantic` — agents receive only the tier their token is scoped to
- [ ] 🟢 Design and document the **Agent Handshake Protocol** — how an agent authenticates, declares capability needs, and subscribes to the correct context tier
- [ ] 🟢 Document integration patterns for: Claude via MCP (done), local Ollama agents, OpenAI function calling
- [ ] 🟢 Optional: lightweight local SLM integration (`llama.cpp` via FFI or subprocess) as a secondary synthesis pass for ambiguous intent when `rules.toml` confidence is below threshold

-----

## Completed (removed from active tracking)

The following items from the original TODO are implemented in the codebase and removed:

- ✅ Semantic Compression Engine — debounce, epsilon filtering, `rules.toml` hot-reload, `CompressionEngine`
- ✅ Token-based auth — 256-bit token, `require_bearer` middleware, audit log, `strip_raw_hid`
- ✅ Host header / DNS-rebinding guard — `host_guard` middleware
- ✅ CORS strict policy — deny-all default, configurable allowlist
- ✅ Versioned `/v1/` API — all endpoints duplicated
- ✅ `ContextSnapshot` synthesis — intent + confidence + state deltas + history ring buffer
- ✅ Web dashboard — served at `/`, WebSocket live feed, exponential-backoff reconnect
- ✅ WASM plugin runtime — `wasmtime` host, `veyn::log` / `veyn::time_ms` / `veyn::http_get` imports
- ✅ Plugin SDK — guest-side Rust ABI, `veyn_register_plugin!` macro
- ✅ `veyn-mcp` — stdio MCP server with full tool list
- ✅ Graceful shutdown — SIGTERM/SIGINT drain
- ✅ Config system — CLI > env > `veyn.toml` > defaults
- ✅ Scope-limited tokens — `tokens.json` with per-token scopes (`"read"`, `"source:<class>"`)
- ✅ Rate limiting — per-IP token bucket via `governor` crate, configurable `rate_limit_rps`
- ✅ `intent_code` field — machine-readable `IntentCode` enum on `ContextSnapshot`
- ✅ `source_class` field — on `StateDelta`, lets agents filter without NLP
- ✅ Non-biometric rules — MIDI, filesystem, keyboard, MQTT/occupancy rules in `rules.toml`
- ✅ Device hot-plug retry — exponential backoff in `spawn_adapter`
- ✅ Dispatcher lock contention fix — single lock scope for metric_state + deltas
- ✅ Linux evdev adapter — `/dev/input/event*` keyboard, mouse, gamepad
- ✅ Linux hidraw adapter — `/dev/hidraw*` raw USB HID
- ✅ MIDI adapter — `midir` CC/note/clock events
- ✅ Serial/UART adapter — `serialport` KEY=FLOAT line protocol
- ✅ Filesystem watcher adapter — `notify` create/modify/delete events
- ✅ `GET /v1/context/subscribe` SSE — declarative filter DSL
- ✅ WebSocket client-side subscribe filtering — runtime filter messages
- ✅ OpenAPI 3.0 spec — `openapi.yaml` covering all `/v1/` routes
- ✅ `docker-compose.yml` — local dev stack with mock mode
- ✅ 12 unit tests — `CompressionEngine` + auth middleware
