# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What VEYN Is

VEYN is a local Rust daemon that ingests physiological and HID sensor data (BLE heart rate, EEG, MIDI, HID, serial, filesystem) and synthesises it into a semantic `ContextSnapshot` for AI agents. It exposes a secured REST/WebSocket/SSE API at `:7700`.

## Build and Development Commands

```bash
# Debug build (veyn-core only — fastest iteration)
cargo build -p veyn-core

# Build all crates
cargo build --workspace

# Run daemon in mock mode (no hardware required, auth disabled)
VEYN_MOCK=true cargo run -p veyn-core -- --no-auth

# Run all tests
cargo test --workspace

# Run a single test by name
cargo test -p veyn-core baseline_stats_roundtrips

# Run integration tests (includes ignored tests)
VEYN_MOCK=true cargo test --workspace -- --include-ignored

# Lint (CI requires zero warnings)
cargo clippy --workspace -- -D warnings

# Format (CI enforces this)
cargo fmt --all

# Format check without modifying files (CI mode)
cargo fmt --all -- --check

# Release binary + tarball
make package
```

The `Makefile` has convenience targets: `make build`, `make release`, `make run-dev`, `make test`, `make lint`, `make fmt`, `make check`.

## Crate Architecture

Six-crate Cargo workspace with a strict layering — lower crates must not import higher ones:

```
veyn-schemas          ← shared types only; no business logic
veyn-adapters         ← hardware/protocol adapters; imports schemas
veyn-plugins          ← WASM plugin host (wasmtime); imports adapters
veyn-core             ← daemon binary; imports all of the above
veyn-mcp              ← standalone MCP stdio server; talks to veyn-core over HTTP
sdk/                  ← TypeScript SDK (sdk/ts/), Python SDK (sdk/py/), Rust guest-plugin SDK (sdk/src/)
```

### Event Flow

Every sensor fires through the same pipeline:

```
VeynAdapter::start() → mpsc::Sender<VeynEvent> (cap 1 024)
  → dispatcher::run()
      → CompressionEngine::should_emit()   (debounce + epsilon filter)
      → BaselineEngine::update()           (rolling 30-day personal baseline)
      → SQLite persist (if session open)
      → CompressionEngine::synthesize()    (z-score Intero → rule-based → "neutral")
      → AppState::update_context()         (ring buffer + broadcast)
          → REST /v1/context/current
          → WebSocket /stream
          → SSE /v1/stream/sse
```

### Key Types (`veyn-schemas/src/lib.rs`)

- **`VeynEvent`** — raw sensor reading: `device_id`, `source`, `metric`, `value`, `unit`, `meta`
- **`ContextSnapshot`** — AI-ready output: `intent`, `intent_code` (`IntentCode` enum), `intent_confidence`, `baseline_delta` (z-scores), `state_deltas`, `active_devices`
- **`IntentCode`** — enum serialised as snake_case strings: `neutral`, `stress_response`, `cognitive_load`, `fatigue`, `recovery`, `approach`, `avoidance`, `Other(String)` for rule-defined codes
- **`BaselineStats`** — `mean`, `stddev`, `p10`, `p90`, `sample_count`; not emitted until 7 days of samples (MIN_SAMPLES) exist
- **`Session`** — named recording session; all events during a session are persisted to SQLite for full-resolution `GET /v1/session/{id}/replay`

### AppState (`veyn-core/src/api/state.rs`)

`AppState` is a cheaply-cloneable `Arc`-wrapped struct shared across all Tokio tasks and Axum route handlers. It holds:
- `latest_metrics` — last `VeynEvent` per metric name
- `context_history` — ring buffer of `ContextSnapshot` (default cap 32)
- `baseline_engine` — `Mutex<BaselineEngine>`
- `session_manager` — `Mutex<SessionManager>` for Intero recording sessions
- `db` — `Option<Arc<Mutex<rusqlite::Connection>>>` (SQLite for event/baseline persistence)
- `broadcast_tx` / `context_broadcast_tx` — `tokio::sync::broadcast` channels for WS/SSE

### CompressionEngine (`veyn-core/src/compression.rs`)

Runs inside `dispatcher::run()`. Two-stage filter:
1. **Epsilon magnitude threshold** — drops events whose delta from previous value is below `compression.epsilons.<metric>` (default 0.5).
2. **Temporal debounce** — drops events arriving faster than `compression.debounce_ms.<metric>` (default 200 ms).

Intent synthesis priority: z-score Intero classification (when baseline sufficient) → rule-based (`rules.toml`) → `"neutral"`.

`rules.toml` is hot-reloaded every 30 s — no daemon restart needed when tuning rules.

### MCP Server (`veyn-mcp/src/main.rs`)

`veyn-mcp` is a standalone binary that speaks JSON-RPC 2.0 over stdio (MCP protocol). It proxies requests to the running daemon over HTTP. Tracing goes to **stderr** only — stdout is the MCP channel and must stay clean JSON. Available MCP tools: `veyn_get_context`, `veyn_get_context_history`, `veyn_get_metric`, `veyn_list_devices`, `veyn_get_presence`, `veyn_send_notification`, `veyn_get_health`, `veyn_list_plugins`, `veyn_get_recent_events`, `veyn_get_gestures`.

## Adding a New Adapter

1. Create `veyn-adapters/src/<name>.rs` and implement the `VeynAdapter` trait (`name()` + async `start(tx)`).
2. Export it from `veyn-adapters/src/lib.rs`.
3. Wire into `veyn-core/src/main.rs` behind a config/env flag using `spawn_adapter()` (handles exponential-backoff retry automatically).
4. Add the enable flag to `veyn.toml.example` and `.env.example`.
5. Document system prerequisites in `INSTALL.md`.

## Adding a WASM Plugin

Plugins run sandboxed in wasmtime with no direct OS access. The Device Proxy Layer reads hardware and passes byte buffers into the sandbox via host-function imports (`veyn::read_device`, `veyn::http_get`, `veyn::time_ms`, `veyn::log`).

Plugin requirements:
- Export `veyn_init`, `veyn_poll`, `veyn_alloc`, `veyn_free` from the WASM binary.
- Use the `veyn_register_plugin!(MyPlugin)` macro from the Rust guest SDK (`sdk/`).
- Ship a `plugin.toml` manifest next to the `.wasm` file (see `plugins/garmin-connect/plugin.toml`).
- SHA-256 of the `.wasm` must match `[signature] sha256` in `plugin.toml` unless `plugins.allow_unsigned = true`.

Install: `veyn-core plugin install /path/to/plugin-dir/`

## Configuration

Copy `veyn.toml.example` to `veyn.toml`. Key env var overrides:

```bash
VEYN_MOCK=true       # use synthetic events (dev/CI)
VEYN_PORT=7700
VEYN_BLE=true
VEYN_EEG=true
VEYN_NO_AUTH=false   # never disable in production
```

Auth token is auto-generated at first launch and stored at `~/.local/share/veyn/token` (mode `0o600`). Scope-limited tokens (read-only, source-scoped) are configured in `~/.local/share/veyn/tokens.json`.

## Code Conventions

- **No `unwrap()` in non-test code** — use `?` with `anyhow::Context` or log and handle the error.
- **No unnecessary `clone()`** — prefer `Arc` for shared state.
- **Commit messages** follow [Conventional Commits](https://www.conventionalcommits.org/): `feat(compression):`, `fix(auth):`, etc. Scope is the crate or subsystem: `core`, `adapters`, `plugins`, `schemas`, `sdk`, `auth`, `compression`, `api`.
- **`CHANGELOG.md`** must be updated under `[Unreleased]` in every PR.
- Platform-specific adapter code (`evdev`, `hidraw`) is gated with `#[cfg(target_os = "linux")]`.

## System Prerequisites (Linux CI)

```bash
sudo apt-get install -y libdbus-1-dev pkg-config libasound2-dev libudev-dev
```

macOS builds do not need these. The Rust toolchain is pinned to `stable` via `rust-toolchain.toml` with `rustfmt` and `clippy` components.
