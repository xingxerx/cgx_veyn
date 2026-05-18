# Changelog

All notable changes to VEYN are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased]

### Added

- **Biometric Memory Layer** (Phase 9 — all crates)
  - New `MemoryRecord` and `MemoryQuery` types in `veyn-schemas` with `MemoryKind` enum
    (`Ambient` | `Semantic`)
  - SQLite migration adds `veyn_memory` table indexed on `timestamp_ms`, `topic`, and `kind`
  - `memory.rs` module in `veyn-core`: `MemoryStore` wrapping the shared SQLite connection,
    `write` / `query` / `summarize_window` operations, and `summarize_snapshots` helper
  - Ambient writer background task (`memory::ambient_writer`) fires every 15 min (configurable),
    reads the in-memory context ring buffer, collapses N snapshots into a statistical summary
    (avg HR, avg HRV, dominant intent, duration), and writes one `Ambient` MemoryRecord;
    skips idle/no-signal windows; oldest ambient records pruned when `max_records` is exceeded
  - REST endpoints protected by existing auth middleware:
    - `POST /v1/memory` — writes a `Semantic` MemoryRecord, auto-attaches current
      physiological state (HR, HRV, intent, confidence) from latest context
    - `GET /v1/memory?topic=&since=&until=&kind=&limit=` — returns matching records
  - `[memory]` config section in `veyn.toml` / `veyn.toml.example`:
    `enabled` (default true), `ambient_interval_secs` (default 900),
    `max_records` (default 10 000)
  - Two new MCP tools in `veyn-mcp`:
    - `veyn_write_memory` (topic, summary) — POSTs to `/v1/memory`; biometric state
      is attached automatically by the daemon
    - `veyn_recall_memory` (topic?, since?, kind?) — GETs from `/v1/memory`; accepts
      human-readable `since` strings like `"7d"`, `"24h"`, `"30d"`
  - MCP session bootstrap: on every `initialize` handshake the server auto-recalls the
    last 24 h of memory (limit 5) and embeds the result in `serverInfo.context`, so AI
    sessions start pre-loaded with recent biometric memory without the agent needing to ask
  - Unit tests for `summarize_window`, write/query round-trip, kind filtering, ambient
    pruning, and the ambient writer firing; integration tests for `MemoryRecord`
    serialisation and `MemoryKind` snake-case encoding

- **Semantic Compression Engine** (`veyn-core/src/compression.rs`)
  - State Reduction Layer: delta filtering, temporal debounce per device class,
    magnitude threshold (epsilon) filtering to suppress micro-jitter
  - Semantic Synthesis Engine: rule-based intent classification with
    hot-reloadable `rules.toml` (no daemon restart required)
  - Output: structured `ContextSnapshot` with `intent`, `confidence`,
    `active_devices`, `state_deltas`, `timestamp_ms`, `session_id`

- **Security hardening**
  - Token-based local auth: 256-bit random token generated on first run,
    stored at `~/.local/share/veyn/token` (chmod 600)
  - All REST and WebSocket connections require `Authorization: Bearer <token>`
    header (or `?token=<token>` query param for WebSocket)
  - Strict CORS policy: deny all cross-origin by default, configurable
    allowlist in `veyn.toml`
  - Host header validation: blocks DNS-rebinding attacks
  - Audit log at `~/.local/share/veyn/audit.log` (auth failures, token events)
  - `strip_raw_hid = true` default: only semantic intent exposed, not raw input

- **Versioned API** — all endpoints duplicated under `/v1/` prefix
  - `GET /v1/context/current` — current semantic context snapshot
  - `GET /v1/context/history?n=10` — last N context snapshots
  - `GET /v1/health` — enhanced with `compression_ratio`, `event_rate_hz`,
    `events_raw`, `session_id`
  - WebSocket ping/pong keepalive (30 s interval)

- **Config file** (`veyn.toml`) — full schema with all daemon settings;
  priority: CLI flags > env vars > veyn.toml > defaults
  - See `veyn.toml.example` for annotated reference

- **CLI flags** for `veyn-core` binary
  - `--config <path>` — explicit config file path
  - `--port <port>` — override API port
  - `--no-auth` — disable auth (development only)

- **Graceful shutdown** — SIGTERM and SIGINT drain the event bus and close
  device handles cleanly before exit

- **Context snapshot ring buffer** — configurable history size
  (default 32, via `compression.context_history_size` in `veyn.toml`)

- **Repo hygiene**
  - `INSTALL.md` — prerequisites and build instructions for all platforms
  - `.env.example` — documented environment variables
  - `veyn.toml.example` — annotated configuration reference
  - `.cargo/config.toml` — workspace build defaults and release profile
  - `rules.toml` — default semantic synthesis rules
  - `CONTRIBUTING.md` — contribution guidelines
  - `CHANGELOG.md` — this file

- **Makefile** — `build`, `run-dev`, `test`, `lint`, `fmt`, `clean` targets

### Changed

- `AppState::new()` now requires `auth_token` and `Config` arguments
- `api::serve()` now takes a graceful-shutdown future
- `dispatcher::run()` passes every event through `CompressionEngine` before
  ingesting; updates `compression_ratio` and `ContextSnapshot` on each pass
- CORS changed from `CorsLayer::permissive()` to strict same-origin-only default
- `Config` struct rewritten to support TOML file loading, env vars, and CLI

### Fixed

- Event bus backpressure: bounded channel (1024) drops oldest events if
  consumers are slow, preventing OOM under high-frequency adapters

---

## [0.5.0] — Phase 5: Cross-device Communication

### Added
- `POST /notify` — send haptic/notification to companion device
- `GET /presence` — per-device presence state
- `GET /gestures/recent` — recent gesture events from companion
- HealthKit bidirectional TCP relay (notifications back to Apple Watch)

## [0.4.0] — Phase 4: WASM Plugin System

### Added
- `veyn-plugins` crate with wasmtime runtime host
- `veyn-plugin-sdk` guest-side SDK with ABI macros
- Plugin manifest (`plugin.toml`) discovery and loading
- `garmin-connect` and `whoop` example plugin stubs

## [0.3.0] — Phase 3: Live Streaming

### Added
- WebSocket broadcast endpoint `/stream`
- Web dashboard at `/`
- Recent-event ring buffer (1000 events)

## [0.2.0] — Phase 2: BLE Wearable

### Added
- `BleAdapter` — BLE Heart Rate Profile GATT scanner and decoder
- Known-device persistence (`veyn-ble-devices.json`)

## [0.1.0] — Phase 1: Foundation

### Added
- `veyn-schemas` — unified `VeynEvent` schema
- `veyn-adapters` — `MockAdapter`, `EegAdapter`
- `veyn-core` — REST API, dispatcher, JSONL audit log
- `veyn-plugins` — WASM plugin runtime skeleton
