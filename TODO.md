# VEYN — Master TODO

> Roadmap to a perfect, runnable-out-of-the-box build.
> Priority: 
🔴 Critical (blocks launch) → 
🟡 High (needed for safety/correctness) → 
🟢 Nice-to-have

-----

## 0. Repo Hygiene (Do First)

- [ ] 🔴 Add root `Cargo.toml` workspace manifest listing all crates (`veyn-core`, `veyn-adapters`, `veyn-plugins`, `sdk`)
- [ ] 🔴 Add `.cargo/config.toml` with target defaults so `cargo build` just works cross-platform
- [ ] 🔴 Add `README.md` — what VEYN is, how to install, how to run, supported platforms
- [ ] 🔴 Add `INSTALL.md` — prerequisites (Rust toolchain version, OS libs like `libudev`, `libdbus`, etc.)
- [ ] 🟡 Add `.env.example` with all required environment variables documented
- [ ] 🟡 Add `CHANGELOG.md`
- [ ] 🟡 Add `CONTRIBUTING.md`
- [ ] 🟢 Add `LICENSE` file (missing from repo)

-----

## 1. 🔴 Critical: Semantic Compression Engine (The Signal Firehose Fix)

The biggest architectural gap. Raw HID/BLE/MIDI data at 100–1000 Hz is completely unusable by any AI agent.

- [ ] 🔴 Build a **State Reduction Layer** inside `veyn-core` — sits between the raw event bus and `/context/current`
  - Implement **delta filtering**: only emit a new context snapshot when meaningful state changes, not every raw event
  - Implement **temporal debounce** per device class (HID mouse: 50ms window, MIDI CC: 20ms, BLE accel: 100ms)
  - Implement **magnitude thresholds** — ignore micro-jitter below configurable epsilon values
- [ ] 🔴 Build a **Semantic Synthesis Engine** — translates filtered events into human-readable intent strings
  - Rule-based first pass: define mappings like `{ rapid_scroll: "user scanning content" }`, `{ input_burst + app_change: "context switch" }`
  - Output format: structured JSON with `intent`, `confidence`, `raw_delta`, `timestamp_ms`
  - Make rules hot-reloadable from a `rules.toml` config file (no daemon restart needed)
- [ ] 🟡 Add a **context snapshot ring buffer** in `veyn-core` — keep last N semantic snapshots (configurable, default 32)
- [ ] 🟡 Expose `/context/history?n=10` endpoint returning last N snapshots for agent catch-up
- [ ] 🟡 Add a `compression_ratio` field to the health endpoint so operators can observe how aggressively the firehose is being filtered
- [ ] 🟢 Optional: lightweight local SLM integration (e.g. `llama.cpp` via FFI or subprocess) as a secondary synthesis pass for ambiguous intent classification

-----

## 2. 🔴 Critical: Security — Kill the Local Keylogger Attack Surface

An unauthenticated WS server on `:7700` exposing raw HID is spyware-grade risk.

- [ ] 🔴 Implement **token-based local auth**
  - On first run, generate a cryptographically random 256-bit token
  - Store token in `$VEYN_RUNTIME_DIR` (e.g. `~/.local/share/veyn/token`) with `chmod 600`
  - All REST and WebSocket connections must present token via `Authorization: Bearer <token>` header
  - Reject unauthenticated connections immediately (no grace period, no fallback)
- [ ] 🔴 Implement **strict CORS policy**
  - Default: deny all cross-origin requests
  - Allowlist configurable in `veyn.toml` (e.g. `cors_origins = ["http://localhost:3000"]`)
  - Add `Host` header validation — reject requests with unexpected Host values (blocks DNS rebinding)
- [ ] 🔴 Add **WebSocket origin validation** — explicitly check `Origin` header on upgrade; reject non-localhost origins unless in allowlist
- [ ] 🟡 Add **scope-limited tokens** — a token can be created with read-only or specific device-class access (e.g. MIDI only, no HID)
- [ ] 🟡 Add **rate limiting** on the REST endpoints (e.g. max 100 req/s per client IP using a token bucket)
- [ ] 🟡 Add **audit log** — every auth failure, new connection, and token generation written to `~/.local/share/veyn/audit.log`
- [ ] 🟡 Strip raw keystroke content from `/context/current` by default — only expose semantic intent, not literal keystrokes
  - Add `raw_hid_passthrough = false` config opt-in flag for power users who explicitly need raw data
- [ ] 🟢 Consider adding mutual TLS (mTLS) option for high-security deployments

-----

## 3. 🔴 Critical: WASM Plugin Architecture Fix

WASM is sandboxed — it cannot touch host hardware without WASI host functions.

- [ ] 🔴 Define a **VEYN Host ABI** — a stable, versioned set of WASI-style host functions that plugins call
  - `veyn_read_bytes(device_handle: u32, buf: *mut u8, len: usize) -> i32`
  - `veyn_emit_event(event_json: *const u8, len: usize) -> i32`
  - `veyn_log(level: u8, msg: *const u8, len: usize)`
  - `veyn_get_config(key: *const u8, key_len: usize, out: *mut u8, out_len: usize) -> i32`
- [ ] 🔴 Add a **Device Proxy Layer** — plugins register a device descriptor (VID/PID, BLE UUID, serial pattern); the core daemon handles actual OS-level device open and passes byte buffers into the WASM sandbox
- [ ] 🔴 Write the WASM runtime host in `veyn-core` using `wasmtime` crate with explicit capability grants per plugin
- [ ] 🟡 Add **plugin manifest format** (`plugin.toml`) — name, version, author, required capabilities, device descriptors
- [ ] 🟡 Add **plugin signature verification** — each plugin WASM binary must be signed; core validates on load
- [ ] 🟡 Write a `veyn-sdk` crate with Rust macros that generate the ABI bindings so plugin authors don’t write raw FFI
- [ ] 🟡 Add a `veyn plugin install <path>` CLI subcommand
- [ ] 🟢 Publish example plugin: `veyn-plugin-midi-launchpad` as reference implementation

-----

## 4. 🟡 High: `veyn-core` Daemon Foundation

- [ ] 🔴 Implement graceful shutdown on SIGTERM/SIGINT — drain event bus, flush state, close device handles cleanly
- [ ] 🔴 Add `veyn.toml` config file with full schema documentation in comments
  - Port, auth settings, CORS origins, device class enables/disables, compression thresholds, log level
- [ ] 🔴 Add `--config <path>` and `--port <port>` CLI flags
- [ ] 🟡 Add structured logging (use `tracing` crate) with log levels: ERROR, WARN, INFO, DEBUG, TRACE
- [ ] 🟡 Add a `/health` REST endpoint returning `{ status, uptime_s, connected_devices, event_rate_hz, compression_ratio }`
- [ ] 🟡 Add a `/devices` REST endpoint listing all currently connected devices with type, ID, and status
- [ ] 🟡 Implement device hot-plug/unplug detection — daemon must not crash when a device disconnects mid-session
- [ ] 🟡 Add event bus backpressure handling — if consumers are slow, drop oldest events rather than OOM-ing
- [ ] 🟢 Add Prometheus metrics endpoint `/metrics` for integration with Grafana/monitoring stacks

-----

## 5. 🟡 High: `veyn-adapters` — Platform Coverage

- [ ] 🔴 Linux: `evdev` HID adapter — keyboard, mouse, gamepad via `/dev/input/event*`
- [ ] 🔴 Linux: `hidraw` adapter for raw USB HID
- [ ] 🟡 macOS: `IOKit`/`IOHIDManager` adapter
- [ ] 🟡 Windows: `WinUSB`/`RawInput` adapter
- [ ] 🟡 BLE adapter (cross-platform via `btleplug` crate) — scan, connect, notify subscriptions
- [ ] 🟡 MIDI adapter (`midir` crate) — CC events, note on/off, clock
- [ ] 🟡 Serial/UART adapter (`serialport` crate) — configurable baud, parity, stop bits
- [ ] 🟡 Filesystem watcher adapter (`notify` crate) — emit events on file create/modify/delete for specified paths
- [ ] 🟢 OSC (Open Sound Control) input adapter — for DAW/VJ software integration
- [ ] 🟢 Audio level adapter — RMS/peak metering from default input device (via `cpal`)

-----

## 6. 🟡 High: `/context/current` API Contract

- [ ] 🔴 Define and document the canonical JSON schema for the context snapshot
  
  ```json
  {
    "timestamp_ms": 1234567890,
    "session_id": "uuid",
    "intent": "user is actively coding",
    "confidence": 0.91,
    "active_devices": ["HID:keyboard", "HID:mouse"],
    "state_deltas": [ ... ],
    "raw_snapshot": { ... }  // only if raw_hid_passthrough = true
  }
  ```
- [ ] 🔴 Version the API — all endpoints prefixed `/v1/`
- [ ] 🟡 Add OpenAPI 3.0 spec (`openapi.yaml`) — auto-generate or hand-write, keep in sync
- [ ] 🟡 WebSocket stream: add a `ping/pong` keepalive with configurable interval
- [ ] 🟡 WebSocket stream: support subscribe filtering — clients can request only specific device classes or intent categories
- [ ] 🟢 Add a Server-Sent Events (SSE) alternative to WebSocket for simpler HTTP-only clients

-----

## 7. 🟡 High: SDK (`/sdk`)

- [ ] 🔴 Publish `veyn-sdk` as a proper Rust crate with `cargo add veyn-sdk` support
- [ ] 🟡 Add TypeScript/Node.js SDK — connect, auth, subscribe to context stream, typed interfaces
- [ ] 🟡 Add Python SDK — `pip install veyn-sdk`, async context manager, typed dataclasses
- [ ] 🟡 Add SDK usage examples for: connecting an LLM agent, building a plugin, reading context history
- [ ] 🟢 Add Go SDK

-----

## 8. 🟢 Developer Experience

- [ ] Add `cargo xtask` or `Makefile` with: `build`, `test`, `run-dev`, `lint`, `fmt`, `package`
- [ ] Add `docker-compose.yml` for a local dev stack (daemon + example consumer agent)
- [ ] Add GitHub Actions CI: lint + test on push for Linux/macOS/Windows
- [ ] Add integration test suite — spin up daemon in test mode, connect mock devices, assert context snapshots
- [ ] Add a `veyn doctor` CLI command — checks prerequisites, permissions, device access, token validity
- [ ] Add a minimal web UI (`localhost:7700/ui`) — live context feed, connected devices, log tail (HTML/JS, no framework)

-----

## 9. 🟢 Phase 7+ AI Integration Prep

- [ ] Design and document the **Agent Handshake Protocol** — how an AI agent authenticates, declares its capability needs, and subscribes to the right context tier
- [ ] Add a `/context/subscribe` endpoint with declarative filter DSL (e.g. `{ "intents": ["idle", "context_switch"], "min_confidence": 0.7 }`)
- [ ] Add `context_tier` config: `raw` | `filtered` | `semantic` — agents get the tier they’re authorized for
- [ ] Document recommended integration patterns for: Claude (via MCP), local Ollama agents, OpenAI function calling

-----

## Quick Reference: Launch Blockers (Must Fix Before Any Public Use)

|#|Item                         |Why It Blocks                      |
|-|-----------------------------|-----------------------------------|
|1|Workspace `Cargo.toml`       |Repo won’t build at all            |
|2|Token auth on `:7700`        |Active security vulnerability      |
|3|CORS + Host header validation|Browser-based keylogger attack     |
|4|Semantic compression layer   |AI context endpoint is unusable raw|
|5|WASM Host ABI                |Plugin system is non-functional    |
|6|Graceful shutdown            |Data loss / device handle leaks    |
|7|`veyn.toml` config schema    |No way to configure the daemon     |
|8|`README.md` + `INSTALL.md`   |Nobody can run it                  |