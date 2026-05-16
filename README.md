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
[![Build Status](https://img.shields.io/github/actions/workflow/status/xingxerx/cgx-veyn/ci.yml?branch=main)](https://github.com/xingxerx/cgx-veyn/actions)

-----

## Table of Contents

- [What It Does](#what-it-does)
- [What You Can Build](#what-you-can-build)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Configuration](#configuration)
- [API Reference](#api-reference)
- [Architecture](#architecture)
- [Plugins](#plugins)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)

-----

## What It Does

- **Universal Device Ingestion**: Accepts signals from any connected device — keyboards, mice, gamepads, MIDI hardware, serial sensors, BLE wearables, EEG/BCI devices, filesystem watchers, network presence detectors
- **Unified Event Schema**: Normalizes all inputs into a single `VeynEvent` format regardless of source protocol
- **Local-First Persistence**: Append-only JSONL event log with sub-millisecond reads and full audit trail
- **Real-Time Streaming**: Broadcast live events to unlimited WebSocket subscribers with automatic history replay
- **Bidirectional Communication**: Route commands back to hardware — notifications, haptics, LED triggers
- **AI-Ready Context**: Structured `/context/current` endpoint providing semantic state snapshots for agent consumption
- **Presence Detection**: Track device activity and emit presence/absence events with configurable timeouts
- **Extensible Architecture**: WASM plugin system allows community extensions without modifying core

-----

## What You Can Build

- **AI Agents with Physical Awareness**: LLMs that understand user activity, context switches, and environmental state
- **Universal Input Layers**: Map any controller, MIDI device, or sensor to game controls or creative tool shortcuts
- **Smart Automation**: Trigger HomeKit scenes, MQTT messages, or IFTTT actions based on biometric or motion events
- **Biometric Monitoring**: Stream heart rate, HRV, EEG bands, or other health metrics to dashboards or training apps
- **Multi-Device Recorders**: Capture synchronized signals from multiple sources for research or analysis
- **Edge IoT Routers**: Deploy on Raspberry Pi or embedded Linux to bridge sensors to cloud services
- **Accessibility Tools**: Convert subtle gestures or biosignals into computer interactions for users with limited mobility

-----

## Quick Start

### Prerequisites

- Rust 1.70+ ([install via rustup](https://rustup.rs/))
- Linux, macOS, or Windows 10+

### Run in 30 Seconds

```bash
# Clone repository
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn

# Run with mock data generator (no hardware required)
VEYN_MOCK=true cargo run --release -p veyn-core
```

### Test the API

In another terminal:

```bash
# Check daemon health
curl http://localhost:7700/health

# Get recent events
curl http://localhost:7700/events/recent

# List connected devices
curl http://localhost:7700/devices

# View live event stream (Ctrl+C to exit)
websocat ws://localhost:7700/stream

# Open live dashboard in browser
open http://localhost:7700  # macOS
xdg-open http://localhost:7700  # Linux
start http://localhost:7700  # Windows
```

-----

## Installation

### From Source (Recommended)

```bash
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn
cargo build --release

# Binary will be at ./target/release/veyn-core
```

### System Packages (When Available)

```bash
# Ubuntu/Debian
sudo apt install veyn

# macOS (Homebrew)
brew install veyn

# Arch Linux
yay -S veyn
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/xingxerx/cgx-veyn/releases) for your platform.

📖 **Full installation instructions**: See [INSTALL.md](./INSTALL.md) for detailed setup guides, system requirements, and troubleshooting.

-----

## Configuration

### Environment Variables

VEYN is configured via environment variables. Copy `.env.example` to get started:

```bash
cp .env.example .env
```

#### Core Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `VEYN_PORT` | 7700 | REST/WebSocket API port |
| `VEYN_HK_PORT` | 7701 | HealthKit TCP relay port |
| `VEYN_LOG` | `veyn-events.jsonl` | Path to append-only event log |
| `VEYN_PLUGINS_DIR` | `plugins` | Directory for WASM plugins |

#### Feature Flags

| Variable | Default | Description |
|----------|---------|-------------|
| `VEYN_MOCK` | `false` | Enable mock data generator for testing |
| `VEYN_BLE` | `false` | Enable Bluetooth Low Energy adapter |
| `VEYN_EEG` | `false` | Enable EEG/OSC brain-computer interface |
| `VEYN_OSC_PORT` | 9000 | UDP port for OSC/EEG input |

#### Integrations

| Variable | Default | Description |
|----------|---------|-------------|
| `VEYN_MQTT_URL` | (unset) | MQTT broker URL (e.g., `mqtt://localhost:1883`) |
| `VEYN_PRESENCE_TIMEOUT` | 30 | Seconds before device marked absent |

#### Logging

```bash
# Set log level (error, warn, info, debug, trace)
export RUST_LOG=info

# Or filter by module
export RUST_LOG=veyn_core=debug,veyn_adapters=info
```

### Example: Full Configuration

```bash
export VEYN_PORT=7700
export VEYN_MOCK=false
export VEYN_BLE=true
export VEYN_EEG=false
export VEYN_LOG=/var/log/veyn/events.jsonl
export VEYN_MQTT_URL=mqtt://homeassistant.local:1883
export VEYN_PRESENCE_TIMEOUT=60
export RUST_LOG=info

veyn-core
```

📖 **Complete configuration guide**: See [INSTALL.md](./INSTALL.md) for systemd services, launchd configs, and permission setup.

-----

## API Reference

Base URL: `http://localhost:7700`

### REST Endpoints

#### `GET /health`

Returns daemon status and basic metrics.

```json
{
  "status": "ok",
  "version": "0.2.0",
  "uptime_seconds": 3600,
  "events_total": 15234
}
```

#### `GET /events/recent`

Returns buffered recent events (default: last 100).

```json
{
  "count": 5,
  "events": [
    {
      "id": "uuid-string",
      "ts": 1234567890,
      "source": "ble",
      "device_id": "polar-h10-abc123",
      "metric": "heart_rate",
      "value": 72.0,
      "unit": "bpm"
    }
  ]
}
```

#### `GET /metrics/:metric`

Get latest value for a specific metric.

```bash
curl http://localhost:7700/metrics/heart_rate
```

```json
{
  "metric": "heart_rate",
  "value": 72.0,
  "unit": "bpm",
  "ts": 1234567890,
  "device_id": "polar-h10-abc123",
  "source": "ble"
}
```

#### `GET /devices`

List all currently connected devices.

```json
{
  "count": 3,
  "devices": [
    {
      "id": "ble:polar-h10-abc123",
      "type": "ble",
      "name": "Polar H10",
      "connected_at": 1234567800,
      "last_seen": 1234567890
    }
  ]
}
```

#### `GET /plugins`

List loaded WASM plugins.

```json
{
  "count": 2,
  "plugins": [
    {
      "name": "garmin-connect",
      "version": "1.0.0",
      "description": "Sync Garmin health data"
    }
  ]
}
```

#### `GET /presence`

Get current presence status for all tracked devices.

```json
{
  "count": 2,
  "presence": [
    {
      "device_id": "ble:polar-h10-abc123",
      "present": true,
      "last_event_ts": 1234567890
    },
    {
      "device_id": "hid:keyboard-usb",
      "present": false,
      "last_event_ts": 1234567800
    }
  ]
}
```

#### `GET /gestures/recent`

Return recent gesture events from companion app.

```json
{
  "count": 3,
  "gestures": [
    {
      "ts": 1234567890,
      "source": "companion",
      "metric": "gesture_swipe_left",
      "confidence": 0.95
    }
  ]
}
```

#### `POST /notify`

Send a notification to a target device.

```bash
curl -X POST http://localhost:7700/notify \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Meeting Reminder",
    "body": "Standup in 5 minutes",
    "target_device": "apple-watch-xyz"
  }'
```

```json
{
  "id": "notif-uuid",
  "status": "queued"
}
```

### WebSocket Endpoints

#### `WS /stream`

Connect to live event stream. Automatically replays recent history on connect.

**Example (Node.js):**

```javascript
const WebSocket = require('ws');
const ws = new WebSocket('ws://localhost:7700/stream');

ws.on('message', (data) => {
  const event = JSON.parse(data);
  console.log(`[${event.source}] ${event.metric}: ${event.value}`);
});

ws.on('error', (err) => console.error(err));
```

**Example (Python):**

```python
import asyncio
import websockets

async def listen():
    async with websockets.connect("ws://localhost:7700/stream") as ws:
        async for message in ws:
            event = json.loads(message)
            print(f"[{event['source']}] {event['metric']}: {event['value']}")

asyncio.run(listen())
```

-----

## Architecture

### Crate Structure

```
cgx-veyn/
├── veyn-schemas/     # Shared types: VeynEvent, VeynMetric, VeynNotification
├── veyn-adapters/    # Device integrations: BLE, HealthKit, EEG, Mock, MQTT
├── veyn-core/        # Main daemon: API server, dispatcher, presence detection
├── veyn-plugins/     # WASM runtime and plugin loader
└── sdk/              # Client libraries (Rust, TypeScript, Python)
```

### Data Flow

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│   Devices   │────▶│   Adapters   │────▶│ Event Bus   │
│ (BLE,HID,…) │     │ (normalize)  │     │ (mpsc chan) │
└─────────────┘     └──────────────┘     └──────┬──────┘
                                                │
                    ┌───────────────────────────┼───────────────────────────┐
                    │                           │                           │
           ┌────────▼────────┐       ┌──────────▼─────────┐      ┌─────────▼────────┐
           │  Presence Tracker│       │  Dispatcher        │      │  Broadcast Channel│
           │  (timeout logic) │       │  (state + persist) │      │  (WebSocket subs) │
           └────────┬────────┘       └──────────┬─────────┘      └─────────┬────────┘
                    │                           │                           │
                    │                  ┌────────▼─────────┐      ┌─────────▼────────┐
                    │                  │  State Store     │      │  WebSocket Clients│
                    │                  │  (recent events) │      │  (live stream)    │
                    │                  └──────────────────┘      └──────────────────┘
                    │                           │
                    ▼                           ▼
           ┌────────────────┐         ┌──────────────────┐
           │ Presence Events│         │  REST API Server │
           │ (emit absence) │         │  (:7700)         │
           └────────────────┘         └──────────────────┘
```

### Key Components

- **Adapters**: Platform-specific device handlers implementing `VeynAdapter` trait
- **Event Bus**: Tokio mpsc channel carrying `VeynEvent` messages
- **Dispatcher**: Central processor updating state, persisting to JSONL, broadcasting
- **Presence Tracker**: Monitors event timestamps, emits presence/absence transitions
- **Broadcast Channel**: Tokio broadcast channel for fan-out to WebSocket clients
- **REST API**: Axum-based HTTP server with WebSocket upgrade support

-----

## Plugins

VEYN supports WASM plugins for extending device support without recompiling the core daemon.

### Plugin Structure

```toml
# plugin.toml
[plugin]
name = "my-custom-device"
version = "1.0.0"
author = "Your Name"
description = "Support for MyDevice Pro"

[capabilities]
requires = ["device_access", "network"]
```

### Installing Plugins

```bash
# Place compiled .wasm file in plugins directory
mkdir -p plugins
cp my-plugin.wasm plugins/

# Restart daemon or send SIGHUP to reload
kill -HUP $(pgrep veyn-core)
```

### Creating a Plugin

See the example plugins in `plugins/garmin-connect/` and `plugins/whoop/`.

📖 **Plugin development guide**: Coming soon in `docs/plugins.md`

-----

## Roadmap

| Phase | Focus                                                              | Status |
|-------|--------------------------------------------------------------------|--------|
| 0     | Foundation — scaffold, schemas, adapters, mock                     | ✅     |
| 1     | Mobile health bridge MVP                                           | ✅     |
| 2     | BLE universal wearable — GATT scan, connect, decode                | ✅     |
| 3     | Live streaming — WebSocket broadcast, web dashboard                | ✅     |
| 4     | Plugin system — WASM adapters, community extensions                | ✅     |
| 5     | Cross-device communication — notifications, haptics, gestures      | ✅     |
| 6     | Universal device expansion — HID, MIDI, Serial, Filesystem, Network| 🚧     |
| 7     | AI context layer — `/context/current`, agent SDK                   | 🔜     |
| 8     | Plugin registry — community adapters, discovery, versioning        | 🔜     |

See [TODO.md](./TODO.md) for detailed task tracking and [ROADMAP.md](./ROADMAP.md) for long-term vision.

-----

## Contributing

We welcome contributions! Here's how to get started:

1. **Check existing issues** or create a new one describing your idea/bug
2. **Fork the repository** and create a feature branch
3. **Make your changes** following our code style (run `cargo fmt` and `cargo clippy`)
4. **Write tests** for new functionality
5. **Submit a pull request** with a clear description of changes

📖 **Full contributing guide**: See [CONTRIBUTING.md](./CONTRIBUTING.md)

### Development Quick Start

```bash
# Clone and enter repo
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn

# Build and run tests
cargo build
cargo test

# Run with mock data
VEYN_MOCK=true cargo run -p veyn-core

# Format and lint
cargo fmt
cargo clippy --all-targets -- -D warnings
```

-----

## Community

- **GitHub Issues**: [Report bugs or request features](https://github.com/xingxerx/cgx-veyn/issues)
- **Discussions**: [Ask questions and share ideas](https://github.com/xingxerx/cgx-veyn/discussions)

-----

## License

**Elastic License 2.0 (ELv2)** © XINGXERX / CGX

Free to use for personal, research, and internal business purposes. Commercial hosting or resale of VEYN as a managed service requires a separate commercial license.

See [`LICENSE`](./LICENSE) for full terms.

-----

## Acknowledgments

- Built with [Tokio](https://tokio.rs/) for async runtime
- HTTP server powered by [Axum](https://github.com/tokio-rs/axum)
- BLE support via [btleplug](https://github.com/btleplug/btleplug)
- Inspired by universal input systems and edge computing architectures