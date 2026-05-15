# CGX VEYN — Phase Roadmap

## Phase 0 — Foundation ✅ (scaffold complete)

- [x] Workspace structure (core / adapters / schemas)
- [x] Unified `VeynEvent` schema
- [x] `VeynAdapter` trait + registry
- [x] Mock adapter (dev/testing)
- [x] HealthKit relay adapter (TCP listener)
- [x] BLE adapter stub (btleplug)
- [x] Event bus (tokio mpsc)
- [x] Dispatcher (log + JSONL persist)
- [x] REST API skeleton (Axum)
- [x] iOS Companion App skeleton (Swift / HealthKit)
- [x] Architecture diagram + docs

## Phase 1 — HealthKit Bridge (MVP) ✅

- [x] iOS companion app: full HealthKit query + background delivery
- [x] iOS companion app: auto-discover daemon on LAN (mDNS/Bonjour)
- [x] iOS companion app: SwiftUI status screen
- [x] Daemon: LMDB state layer (latest value per metric)
- [x] Daemon: SQLite migration runner + history queries
- [x] REST: `GET /metrics/:metric` returns real LMDB value
- [x] REST: `GET /devices` tracks active companion sessions
- [x] README: Getting Started guide

## Phase 2 — BLE Universal Wearable ✅

- [x] BLE adapter: scan + connect to GATT Heart Rate profile
- [x] BLE adapter: decode HR measurement characteristic
- [x] BLE adapter: Battery level monitoring
- [x] BLE adapter: device persistence (known devices list)
- [x] Schema: `VeynDevice` LMDB registry

## Phase 3 — Live Streaming ✅

- [x] WebSocket endpoint `GET /stream`
- [x] Broadcast channel (tokio broadcast) wired to dispatcher
- [x] Client reconnect + event replay (last N)
- [x] Simple web dashboard (HTML/JS, served by daemon)

## Phase 4 — Plugin System ✅

- [x] WASM plugin runtime (wasmtime)
- [x] Plugin manifest format (TOML)
- [x] Plugin SDK (Rust + WASM target)
- [x] Example plugin: Garmin Connect (OAuth pull)
- [x] Example plugin: Whoop API

## Phase 5 — Cross-Device Communication ✅

- [x] Notification routing: PC → Apple Watch (via companion)
- [x] Gesture/input forwarding: Watch crown/tap → desktop events
- [x] Presence detection: watch heartbeat → PC automation trigger
- [x] Smart home bridge: MQTT output adapter