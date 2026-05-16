# CGX VEYN

**A Sensory Nervous System for Software **

VEYN is a local-first daemon that normalizes signals from any connected device into a single open event stream — exposed as a standard local API any application, script, or AI agent can consume.

No cloud. No accounts. No data leaving your machine.

```
HID / MIDI / Serial / BLE / Network / Filesystem
                    ↓
              VEYN Daemon
                    ↓
        REST :7700  ·  WebSocket :7700/stream
```

-----

## What It Does

- Ingests signals from **any connected device** — controllers, MIDI hardware, serial sensors, BLE peripherals, filesystem events, network presence
- Normalizes everything into a single **`VeynEvent` schema** regardless of source protocol
- Persists state locally with sub-millisecond reads, queryable history, and a full audit log
- Streams live events to any subscriber over WebSocket
- Routes commands **back to hardware** — triggers, haptics, notifications
- Exposes a **`/context/current`** endpoint: a structured world-state snapshot designed for AI agent consumption
- Extensible via a **WASM plugin system** — add any device source without touching core

-----

## What You Can Build

- AI agents with live awareness of their physical environment
- Universal input layers for games and creative tools
- Hardware-driven automation and smart environment triggers
- Multi-device signal recording platforms
- Edge IoT event routers on embedded Linux

-----

## Quick Start

```bash
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn
VEYN_MOCK=true cargo run -p veyn-core
```

```bash
curl http://localhost:7700/health
curl http://localhost:7700/events/recent
curl http://localhost:7700/context/current
```

-----

## Roadmap

|Phase|Focus                                                              |Status|
|-----|-------------------------------------------------------------------|------|
|0    |Foundation — scaffold, schemas, adapters, mock                     |✅     |
|1    |Mobile health bridge MVP                                           |✅     |
|2    |BLE universal wearable — GATT scan, connect, decode                |✅     |
|3    |Live streaming — WebSocket broadcast, web dashboard                |✅     |
|4    |Plugin system — WASM adapters, community extensions                |✅     |
|5    |Cross-device communication — notifications, haptics, gestures      |✅     |
|6    |Universal device expansion — HID, MIDI, Serial, Filesystem, Network|✅     |
|7    |AI context layer — `/context/current`, agent SDK                   |✅     |
|8    |Plugin registry — community adapters, discovery, versioning        |✅     |     |

-----

## License

Elastic License 2.0 (ELv2) © XINGXERX / CGX

Free to use for personal, research, and internal business purposes.
Commercial hosting or resale of VEYN as a managed service requires a separate commercial license.
See [`LICENSE`](./LICENSE) for full terms.
