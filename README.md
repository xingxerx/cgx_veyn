# VEYN

**A Sensory Nervous System for Software**

VEYN is a local-first daemon that normalizes signals from any connected device into a single open event stream — exposed as a standard local API any application, script, or AI agent can consume.

No cloud. No accounts. No data leaving your machine.

```
HID / MIDI / Serial / BLE / Network / Filesystem / HealthKit / EEG
                              ↓
                        VEYN Daemon
                              ↓
              REST :7700  ·  WebSocket :7700/stream
```

[![License: ELv2](https://img.shields.io/badge/License-ELv2-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)

-----

## What It Does

VEYN ingests signals from any connected device (keyboards, mice, BLE wearables, EEG, MIDI, serial sensors) and normalizes them into a unified `VeynEvent` schema. It provides:

- **Real-time streaming** via WebSocket with automatic history replay
- **REST API** for querying devices, metrics, and presence status
- **Local-first persistence** with append-only JSONL event logs
- **Presence detection** tracking device activity with configurable timeouts
- **WASM plugin system** for extending device support without recompiling

## How It Works

1. **Adapters** poll or subscribe to devices (BLE, HID, MIDI, Serial, etc.)
2. Events are normalized into `VeynEvent` schema and sent to an **Event Bus**
3. **Dispatcher** persists events to JSONL and broadcasts to WebSocket subscribers
4. **Presence Tracker** monitors activity and emits presence/absence transitions
5. **REST API** serves state queries and device management

## Quick Start

```bash
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn
VEYN_MOCK=true cargo run --release -p veyn-core
```

Test the API:

```bash
curl http://localhost:7700/health
curl http://localhost:7700/events/recent
curl http://localhost:7700/devices
```

See [INSTALL.md](./INSTALL.md) for detailed setup and [`.env.example`](./.env.example) for configuration options.

-----

## What You Can Build

- **AI Agents** with awareness of user activity and context switches
- **Smart Automation** triggered by biometric or motion events
- **Universal Input Layers** mapping any device to custom actions
- **Biometric Dashboards** streaming heart rate, EEG, or health metrics
- **Accessibility Tools** converting gestures into computer interactions

-----

## API Overview

Base URL: `http://localhost:7700`

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Daemon status and metrics |
| `GET /events/recent` | Last 100 buffered events |
| `GET /metrics/:metric` | Latest value for a metric |
| `GET /devices` | Connected devices list |
| `GET /presence` | Device presence status |
| `GET /plugins` | Loaded WASM plugins |
| `POST /notify` | Send notification to device |
| `WS /stream` | Live event stream |

See [INSTALL.md](./INSTALL.md) for full API documentation.

-----

## Project Structure

```
cgx-veyn/
├── veyn-schemas/     # Shared types: VeynEvent, VeynMetric
├── veyn-adapters/    # Device integrations: BLE, HID, Mock, MQTT
├── veyn-core/        # Main daemon: API, dispatcher, presence
├── veyn-plugins/     # WASM runtime and plugin loader
├── sdk/              # Client libraries
└── plugins/          # Example plugins (Garmin, Whoop)
```

-----

## Roadmap

| Phase | Focus | Status |
|-------|-------|--------|
| 0 | Foundation — workspace, schemas, adapters, mock | ✅ |
| 1 | Mobile health bridge MVP | ✅ |
| 2 | BLE universal wearable | ✅ |
| 3 | Live streaming — WebSocket broadcast | ✅ |
| 4 | Plugin system — WASM runtime | ✅ |
| 5 | Cross-device communication | ✅ |
| 6 | Universal device expansion (HID, MIDI, Serial) | 🚧 |
| 7 | AI context layer | 🔜 |

See [TODO.md](./TODO.md) for detailed tasks and [ROADMAP.md](./ROADMAP.md) for long-term vision.

-----

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup and guidelines.

```bash
cargo build
cargo test
VEYN_MOCK=true cargo run -p veyn-core
```

-----

## License

**Elastic License 2.0 (ELv2)** — See [`LICENSE`](./LICENSE)