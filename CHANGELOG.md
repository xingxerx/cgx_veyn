# Changelog

All notable changes to VEYN are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased]

### Added

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
