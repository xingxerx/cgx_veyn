# VEYN

**The sensory nervous system for software.**

VEYN is a local daemon that reads physiological and HID sensor data — BLE heart rate, EEG, MIDI, HID, serial, filesystem — and synthesises it into a semantic `ContextSnapshot` for AI agents and applications. It maintains a personal baseline for each signal, classifies intent from z-score patterns, and exposes everything over a secured REST/WebSocket/SSE API at `:7700`.

```
BLE / EEG / MIDI / HID / Serial / Filesystem / WASM Plugins
                         ↓
                   VEYN Daemon (:7700)
          ┌──────────────┼──────────────┐
     REST API       WebSocket       SSE stream
    /v1/context    /stream        /v1/stream/sse
          ↓              ↓              ↓
   AI Agents    TypeScript SDK    Python SDK    MCP (Local)
```

---

## Architecture

VEYN is a 6-crate Cargo workspace:

| Crate | Role |
|---|---|
| `veyn-schemas` | Shared types: `VeynEvent`, `ContextSnapshot`, `IntentCode`, `MemoryRecord`, `BaselineStats` |
| `veyn-adapters` | Signal adapters: BLE, EEG/OSC, HealthKit relay, evdev, hidraw, MIDI, serial, filesystem, MQTT, mock |
| `veyn-core` | Daemon: event bus, `CompressionEngine`, `BaselineEngine`, `MemoryStore`, `SessionManager`, SQLite, REST/WS/SSE API |
| `veyn-plugins` | WASM plugin host (wasmtime), device proxy layer, signature verification |
| `veyn-mcp` | MCP server — exposes VEYN tools to local AI agents; session bootstrap with biometric memory |
| `sdk/` | TypeScript SDK (`sdk/ts/`), Python SDK (`sdk/py/`), Rust guest plugin SDK |

---

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Run in mock mode — no hardware needed
VEYN_MOCK=true cargo run -p veyn-core

# Verify
curl http://localhost:7700/health
TOKEN=$(cat ~/.local/share/veyn/token)
curl -H "Authorization: Bearer $TOKEN" http://localhost:7700/v1/context/current
```

---

## Key Features

### Intero Physiological Decision Support

VEYN tracks a personal baseline for every `(device, metric)` pair over a 30-day rolling window. All intent classification uses z-scores against that personal baseline, not population norms.

- **`BaselineEngine`** — background task recalculating mean, stddev, p10/p90 every 15 minutes from SQLite event history; baseline is marked `sufficient = false` until 7 days of data exist
- **`IntentCode` classification** — structured enum derived from z-score combinations:
  - `StressResponse` — HR z > 1.5, HRV z < -1.0
  - `CognitiveLoad` — EEG beta z > 1.0, alpha z < -0.5
  - `Fatigue` — HR z < -0.5, EEG theta z > 1.0
  - `Recovery` — HRV z > 1.0, HR z < 0.0
  - `Approach` — HR z 0.5–1.5, HRV z > 0.0
  - `Avoidance` — HR z > 0.5, HRV z < -0.5, skin_temp z > 0.5
  - `Neutral` — no threshold met
- **Named session recording** — `POST /v1/session/start` opens a named `InteroSession` with an optional annotation; all events during the session are written to SQLite with the session ID for full-resolution replay; `GET /v1/session/{id}/replay` returns the complete multi-channel timeline bypassing compression

### Biometric Memory Layer

VEYN maintains a persistent per-topic memory of physiological state across sessions, enabling AI agents to orient themselves at session start without explicit user prompting.

- **Ambient writer** — background task that fires every 15 minutes (configurable), reads the in-memory context ring buffer, computes avg HR, avg HRV, dominant intent, and session duration, then writes one `Ambient` MemoryRecord to SQLite. Idle/no-signal windows are skipped; oldest ambient records are pruned when `max_records` is exceeded; semantic records are never auto-pruned.
- **Semantic records** — AI agents write these explicitly via `POST /v1/memory` or the `veyn_write_memory` MCP tool. The daemon automatically attaches the current physiological state (HR, HRV, intent, confidence) at write time.
- **`GET /v1/memory`** — query records by `topic`, `since`, `until`, `kind`, and `limit`.
- **MCP session bootstrap** — on every `initialize` handshake, `veyn-mcp` auto-recalls the last 24 h of memory and embeds it in `serverInfo.context`. Every AI session starts pre-loaded with recent biometric history.
- **`veyn_recall_memory`** / **`veyn_write_memory`** — two new MCP tools; `since` accepts human-readable strings like `"7d"`, `"24h"`, `"30d"`.

### Semantic Compression Engine

Raw sensor data at 100–1000 Hz is unusable by agents directly. The `CompressionEngine` in `veyn-core` reduces it to a structured `ContextSnapshot`:

- Delta filtering, temporal debounce, and epsilon magnitude thresholds per metric
- Rule-based intent classification via hot-reloadable `rules.toml` (reloads every 30 s, no restart required)
- `ContextSnapshot` output: `intent`, `intent_code`, `intent_confidence`, `baseline_delta` map (z-scores), `active_devices`, `state_deltas`, `timestamp_ms`, `session_id`

### WASM Plugin System

Plugins run in a wasmtime sandbox with no direct OS access. The **Device Proxy Layer** bridges hardware into the sandbox safely:

- Plugins declare device descriptors (VID/PID, BLE UUID, serial pattern) in `plugin.toml`
- The daemon opens the device and passes byte buffers into WASM via host-function imports
- Host imports available to plugins: `veyn::read_device(handle)`, `veyn::http_get(url)`, `veyn::time_ms()`, `veyn::log(level, msg)`
- Every `.wasm` binary must be signed; `veyn-core` validates the signature on load and rejects unsigned plugins unless `plugins.allow_unsigned = true`
- Install a plugin: `veyn-core plugin install <path>`

### API

Full REST, WebSocket, and SSE API at `:7700` with `Authorization: Bearer <token>` on all non-health endpoints.

- Rate limiting: token bucket, 100 req/s per client IP
- CORS: deny cross-origin by default, configurable allowlist
- Scope-limited tokens: mint read-only or device-class-scoped tokens (e.g. MIDI only)
- Host header validation blocks DNS-rebinding attacks

### Signal Adapters

| Adapter | Description |
|---|---|
| Mock | Synthetic event generator — full dev/CI without hardware |
| BLE | GATT Heart Rate Profile scan, decode, battery monitoring, device persistence |
| EEG/OSC | UDP OSC input stream (Mind Monitor compatible) |
| HealthKit relay | iOS companion bridge with mDNS auto-discovery |
| MQTT | IoT and smart-home integration |
| evdev | Linux keyboard, mouse, gamepad via `/dev/input/event*` |
| hidraw | Linux raw USB HID for devices not covered by evdev |
| MIDI | CC events, note on/off, clock via `midir` |
| Serial/UART | Configurable baud/parity/stop for custom biometric hardware |
| Filesystem watcher | Emit events on file create/modify/delete via `notify` |

All adapters auto-restart on failure with exponential backoff.

---

## API Summary

| Endpoint | Method | Description |
|---|---|---|
| `/health` | GET | Liveness check — no auth required |
| `/v1/context/current` | GET | Current `ContextSnapshot` with `intent_code` and `baseline_delta` |
| `/v1/context/history` | GET | Last N snapshots (ring buffer, configurable, default 32) |
| `/v1/session/start` | POST | Start a named `InteroSession` |
| `/v1/session/stop` | POST | Stop the active session, returns final record |
| `/v1/sessions` | GET | Paginated session list ordered by `started_at DESC` |
| `/v1/session/{id}` | GET | Session metadata |
| `/v1/session/{id}/replay` | GET | Full-resolution event timeline from SQLite |
| `/v1/session/{id}/export` | GET | Flat CSV export (`?format=csv`) |
| `/v1/baseline/{device}/{metric}` | GET | `BaselineStats` including `sufficient` flag and z-score history |
| `/v1/memory` | POST | Write a `Semantic` MemoryRecord; biometric state auto-attached from latest context |
| `/v1/memory` | GET | Query MemoryRecords (`?topic=&since=&until=&kind=&limit=`) |
| `/metrics` | GET | Prometheus metrics (events_raw_total, compression_ratio, active_devices, …) |
| `/openapi.yaml` | GET | OpenAPI 3.0 specification |
| `/stream` | GET (WS) | Raw live event WebSocket stream |
| `/v1/stream/sse` | GET (SSE) | Server-Sent Events stream for HTTP-only clients |

Token is auto-generated at first launch and stored at `~/.local/share/veyn/token` (mode `0o600`).

---

## Configuration

Copy `veyn.toml.example` to `veyn.toml`. All keys are optional — unset keys fall back to defaults.

```toml
[server]
port = 7700
healthkit_port = 7701

[security]
require_auth = true
cors_origins = []

[adapters]
mock = false
ble = false
eeg = false
osc_port = 9000

[plugins]
dir = "plugins"
allow_unsigned = false

[memory]
enabled = true
ambient_interval_secs = 900
max_records = 10000

[compression]
rules_path = "rules.toml"
context_history_size = 32

[compression.debounce_ms]
heart_rate = 1000
hrv = 2000

[compression.epsilons]
heart_rate = 1.0
hrv = 2.0

[logging]
level = "info"
jsonl_path = "veyn-events.jsonl"
```

Key environment variable overrides:

```bash
VEYN_PORT=7700
VEYN_MOCK=true
VEYN_BLE=true
VEYN_EEG=true
VEYN_NO_AUTH=false   # never disable in production
```

---

## SDK Usage

### TypeScript

```typescript
import { VeynClient } from 'veyn-sdk';

const client = new VeynClient({ token: process.env.VEYN_TOKEN });
const snapshot = await client.getContext();
console.log(snapshot.intent_code, snapshot.intent_confidence);

client.subscribe((snapshot) => {
  if (snapshot.intent_code === 'stress_response') {
    // adapt agent behaviour
  }
});
```

### Python

```python
from veyn_sdk import VeynClient
import asyncio

async def main():
    async with VeynClient(token=os.environ["VEYN_TOKEN"]) as client:
        snapshot = await client.get_context()
        print(snapshot.intent_code, snapshot.intent_confidence)

asyncio.run(main())
```

---

## CLI Subcommands

```bash
# Check prerequisites, token validity, device access
veyn-core doctor

# Install a WASM plugin (validates manifest and signature)
veyn-core plugin install /path/to/plugin.wasm
```

---

## Development

```bash
# Run all tests (50 tests including integration suite)
cargo test --workspace

# Build release
cargo build --workspace --release

# Run integration tests with mock adapter
VEYN_MOCK=true cargo test --workspace -- --include-ignored
```

The integration test suite spins up the daemon in mock mode, injects known metric sequences, and asserts that `ContextSnapshot.intent_code` matches expected values without live hardware.

---

## MCP Integration

`veyn-mcp` exposes VEYN as an MCP server for local AI agents. Works with any MCP-compatible client (Open WebUI, Jan.ai, etc.) running a local model such as Gemma 4 via Ollama.

**Available tools:** `veyn_get_context`, `veyn_get_context_history`, `veyn_get_metric`, `veyn_list_devices`, `veyn_get_presence`, `veyn_send_notification`, `veyn_get_health`, `veyn_list_plugins`, `veyn_get_recent_events`, `veyn_get_gestures`, `veyn_start_session`, `veyn_stop_session`, `veyn_get_session`, `veyn_write_memory`, `veyn_recall_memory`

**Session bootstrap:** on every `initialize` handshake the MCP server automatically calls `veyn_recall_memory` (last 24 h, limit 5) and embeds the result in `serverInfo.context`. AI sessions start pre-loaded with recent biometric memory.

---

## License

Elastic License 2.0 (ELv2) © XINGXERX / CGX

Free to use for personal, research, and internal business purposes. Hosting or reselling VEYN as a managed service requires a separate commercial license. See [`LICENSE`](./LICENSE) for full terms.
