# CGX_VEYN

**A universal signal bus for software.**

VEYN is a local-first daemon that ingests signals from any source, normalizes them into a single typed event schema, and exposes a live stream any application, agent, or AI host can consume.

The signal source is irrelevant. Hardware, APIs, files, network events, custom adapters — everything collapses into `VeynEvent` and flows downstream identically.

```
Any Source
(BLE / OSC / TCP / MQTT / WASM Plugin / Mock)
                 ↓
           VEYN Daemon
                 ↓
   REST :7700  ·  WebSocket :7700/stream
```

---

## What It Does

- Accepts signals from **any connected source** via a pluggable adapter system
- Normalizes all input into a single `VeynEvent` schema regardless of origin protocol
- Persists state in LMDB (sub-millisecond reads) with queryable SQLite history and a full JSONL audit log
- Broadcasts live events to all subscribers over WebSocket
- Routes commands **back to connected hardware** — notifications, triggers, haptics
- Exposes `/context/current` — a structured world-state snapshot designed for AI agent consumption
- Extensible via **WASM plugins** — add any signal source at runtime without recompiling core

---

## Signal Adapters

| Adapter | Protocol | Description |
|---|---|---|
| Mock | Internal | Synthetic event generator for development and CI |
| BLE | GATT / btleplug | Scan, connect, and decode any BLE peripheral |
| OSC / EEG | UDP | OSC input stream (Mind Monitor, custom sources) |
| TCP Relay | TCP | Mobile companion bridge (mDNS auto-discovery) |
| MQTT | MQTT | IoT device and smart environment integration |
| WASM Plugins | Custom ABI | Drop-in adapters — Garmin, Whoop, or any custom source |

Adapters are hot-pluggable and auto-restart on failure with exponential backoff. The daemon runs fully in mock mode with no hardware attached.

---

## Event Schema

Every adapter emits the same type regardless of source:

```rust
pub struct VeynEvent {
    pub id: String,          // UUID v4
    pub ts: i64,             // Unix timestamp (ms)
    pub device_id: String,   // Source device identifier
    pub source: String,      // Adapter name: "ble" | "osc" | "mqtt" | "mock" | ...
    pub metric: String,      // Signal name: any string
    pub value: f64,          // Normalized scalar value
    pub unit: String,        // SI or domain unit string
    pub meta: HashMap<String, Value>, // Adapter-specific key/value pairs
}
```

The schema is intentionally generic. Any signal from any source fits.

---

## API

| Endpoint | Method | Description |
|---|---|---|
| `/health` | GET | Liveness check — no auth required |
| `/v1/events/recent` | GET | Last ~1000 events from the ring buffer |
| `/v1/metrics/:metric` | GET | Latest LMDB value for a named metric |
| `/v1/devices` | GET | All registered devices and connection state |
| `/v1/presence` | GET | Per-device present/absent state |
| `/v1/context/current` | GET | Synthesized world-state snapshot |
| `/v1/context/history` | GET | Last N context snapshots |
| `/v1/context/subscribe` | GET (SSE) | Declarative DSL-filtered live context stream |
| `/notify` | POST | Push notification to a connected device |
| `/stream` | GET (WS) | Raw live event WebSocket stream |

All authenticated endpoints require `Authorization: Bearer <token>`. Token is auto-generated at first launch and stored at `~/.local/share/veyn/token` (mode `0o600`).

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

[mqtt]
# url = "mqtt://localhost:1883"

[compression]
rules_path = "rules.toml"
context_history_size = 32

[compression.debounce_ms]
# Per-metric debounce windows (ms) — events faster than this are dropped
heart_rate = 1000
hrv = 2000

[compression.epsilons]
# Per-metric magnitude thresholds — changes smaller than epsilon are treated as noise
heart_rate = 1.0
hrv = 2.0

[logging]
level = "info"
jsonl_path = "veyn-events.jsonl"

[plugins]
dir = "plugins"
```

Environment variable overrides (highest priority):

```bash
VEYN_PORT=7700
VEYN_MOCK=true           # enable mock adapter
VEYN_BLE=true            # enable BLE adapter
VEYN_EEG=true            # enable OSC/EEG adapter
VEYN_MQTT_URL=mqtt://localhost:1883
VEYN_NO_AUTH=false       # never disable in production
```

---

## Quick Start

```bash
# Mock mode — no hardware needed
VEYN_MOCK=true cargo run -p veyn-core

# Verify
curl http://localhost:7700/health
curl http://localhost:7700/events/recent
curl http://localhost:7700/context/current
```

---

## WASM Plugin System

Drop a plugin manifest (`plugin.toml`) into the `plugins/` directory. VEYN discovers and loads it at startup with no recompilation required.

```toml
[plugin]
name = "my-adapter"
version = "0.1.0"
description = "Custom signal source"
```

The plugin SDK exposes a simple Rust ABI — implement the `VeynAdapter` trait, compile to `wasm32-wasi`, and VEYN handles the rest. See `veyn-plugins/` for the SDK and example adapters (Garmin Connect, Whoop).

---

## Roadmap

| Phase | Focus | Status |
|---|---|---|
| 0 | Foundation — schemas, adapters, mock, REST skeleton | ✅ |
| 1 | Mobile companion bridge — TCP relay, mDNS discovery | ✅ |
| 2 | BLE universal wearable — GATT scan, connect, decode | ✅ |
| 3 | Live streaming — WebSocket broadcast, web dashboard | ✅ |
| 4 | WASM plugin system — runtime, SDK, example adapters | ✅ |
| 5 | Cross-device communication — notifications, gestures, presence | ✅ |
| 6 | Universal device expansion — HID, MIDI, Serial, Filesystem, Network | ✅ |
| 7 | AI context layer — `/context/current`, agent SDK | ✅ |
| 8 | Plugin registry — community adapters, discovery, versioning | ✅ |

---

## License

Elastic License 2.0 (ELv2) © XINGXERX / CGX

Free to use for personal, research, and internal business purposes. Hosting or reselling VEYN as a managed service requires a separate commercial license. See [`LICENSE`](./LICENSE) for full terms.
