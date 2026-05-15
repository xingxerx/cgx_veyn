# CGX VEYN

**Universal Open Access Bridge for Connected Devices**

VEYN is an open-source, local-first daemon that bridges the gap between closed consumer device ecosystems and the open software world. It reads data from wearables, health sensors, and mobile devices through their officially documented access points, normalizes everything into a single unified event stream, and exposes it as a standard local API that any application, script, or AI system can consume — with no cloud dependency, no third-party accounts, and no data leaving your machine.

```
Wearable Sensors ──► Mobile Companion App ──► VEYN Daemon ──► Your Software
EEG / BLE Devices ─────────────────────────────────────────► REST API  :7700
Any Connected Device ───────────────────────────────────────► WebSocket :7700/stream
```

-----

## The Problem VEYN Solves

Consumer wearables generate extraordinary amounts of physiological data — heart rate, HRV, sleep stages, oxygen saturation, EEG bands, movement, temperature. That data is yours. But by default it is locked inside proprietary mobile ecosystems with no path to the software you actually want to build.

VEYN is the lever. It uses every legitimate, documented access point those ecosystems provide — mobile health SDKs, Bluetooth LE GATT profiles, local network protocols — and routes the data to a unified open API on your own hardware. No walls are broken. No terms are violated. The data simply flows where you direct it.

-----

## Capabilities

### Data Ingestion

- **Mobile Health SDK Bridge** — reads biometric data (heart rate, HRV, SpO₂, steps, sleep stages, respiratory rate, skin temperature, active energy, VO₂ max) from a paired companion app running on a mobile device, streamed live over a local TCP connection
- **BLE Universal Adapter** — connects to any Bluetooth Low Energy wearable broadcasting standard GATT profiles; no proprietary driver required
- **EEG / OSC Adapter** — ingests real-time neural band data (Delta, Theta, Alpha, Beta) from EEG headsets via OSC (Open Sound Control) over UDP
- **Plugin Adapters** — extend ingestion to any source via the WASM plugin system (Phase 4)

### Data Normalization

- All sources — regardless of origin, format, or protocol — are normalized into a single `VeynEvent` schema
- Newline-delimited JSON throughout; every consumer speaks the same language
- Unified device registry with per-device metadata and connection state

### Storage

- **LMDB** — sub-millisecond key-value state store for latest values per metric per device
- **SQLite** — structured queryable history for long-term trend analysis and correlation
- **JSONL** — append-only audit log of every event, timestamped and immutable

### API Layer

- **REST** — `GET /events/recent`, `GET /metrics/:metric`, `GET /devices`, `GET /health`
- **WebSocket** — live event stream at `/stream`; any subscriber receives every event in real time
- **Local only** — bound to `127.0.0.1` by default; never touches an external server

### Cross-Device Communication (Phase 6)

- Route notifications and haptic triggers from any software on your machine back to paired wearable devices
- Use wearable gestures and inputs as desktop events
- Presence detection and behavioral automation via wearable sensor state

### AI & AGI Integration

- Event bus designed as a first-class live signal source for AI pipelines
- Any LLM, agent, or inference system with HTTP access can consume the full biometric stream

-----

## What You Can Build With VEYN

- **Closed-loop biofeedback systems** — detect a physiological state, trigger a response back to the body in real time
- **Personal health intelligence** — longitudinal biometric logging under your own keys, queryable with no subscription
- **Body-driven interfaces** — wearable gestures as desktop input, physiological state as application context
- **Smart environment automation** — biometric triggers routed to home automation systems via MQTT
- **Research platforms** — continuous multi-source physiological recording with full audit trail
- **AI with embodied context** — give your AI system a live feed of your physiological state

-----

## Quick Start

```bash
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn
VEYN_MOCK=true cargo run -p veyn-core
```

Daemon starts on `http://localhost:7700`.

```bash
# Health check
curl http://localhost:7700/health

# Recent events (mock data streams by default in dev mode)
curl http://localhost:7700/events/recent
```

-----

## Architecture

```
Adapters (Health SDK / BLE / EEG-OSC / Mock)
    ↓
Event Bus (async mpsc channel)
    ↓
Dispatcher
    ├── Storage (LMDB + SQLite + JSONL)
    └── API (REST + WebSocket — Axum :7700)
```

Full diagram: [`docs/architecture.mermaid`](docs/architecture.mermaid)
Full roadmap: [`docs/ROADMAP.md`](docs/ROADMAP.md)

-----

## Stack

|Layer               |Technology                     |
|--------------------|-------------------------------|
|Core daemon         |Rust (stable)                  |
|Async runtime       |Tokio                          |
|HTTP / WebSocket API|Axum                           |
|Bluetooth LE        |btleplug                       |
|Key-value state     |LMDB                           |
|Structured history  |SQLite                         |
|Audit log           |Append-only JSONL              |
|Mobile companion    |Swift (health SDK + TCP stream)|
|Plugin runtime      |WASM — wasmtime (Phase 4)      |

-----

## Roadmap

|Phase|Focus                                                        |
|-----|-------------------------------------------------------------|
|0    |Foundation — scaffold, schemas, adapters, mock ✅            |
|1    |Mobile health bridge MVP — live biometric stream to daemon   |
|2    |BLE universal wearable — GATT scan, connect, decode          |
|3    |Live streaming — WebSocket broadcast, web dashboard          |
|4    |Plugin system — WASM adapters, community extensions          |
|5    |Cross-device communication — notifications, haptics, gestures|

-----

## Disclaimer

VEYN uses only officially documented SDKs, publicly broadcast radio protocols (Bluetooth LE), and user-granted device permissions. No proprietary protocols are reverse-engineered. No third-party servers are accessed. Users are responsible for compliance with the terms of service of any platforms or APIs they connect to via VEYN adapters.

-----

## License

MIT © XINGXERX / CGX

*Part of the CGX project ecosystem.*

A open-source information bridge 
