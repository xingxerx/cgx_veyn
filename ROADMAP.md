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

## Phase 1 — HealthKit Bridge (MVP)

- [ ] iOS companion app: full HealthKit query + background delivery
- [ ] iOS companion app: auto-discover daemon on LAN (mDNS/Bonjour)
- [ ] iOS companion app: SwiftUI status screen
- [ ] Daemon: LMDB state layer (latest value per metric)
- [ ] Daemon: SQLite migration runner + history queries
- [ ] REST: `GET /metrics/:metric` returns real LMDB value
- [ ] REST: `GET /devices` tracks active companion sessions
- [ ] README: Getting Started guide

## Phase 2 — BLE Universal Wearable

- [ ] BLE adapter: scan + connect to GATT Heart Rate profile
- [ ] BLE adapter: decode HR measurement characteristic
- [ ] BLE adapter: Battery level monitoring
- [ ] BLE adapter: device persistence (known devices list)
- [ ] Schema: `VeynDevice` LMDB registry

## Phase 3 — Live Streaming

- [ ] WebSocket endpoint `GET /stream`
- [ ] Broadcast channel (tokio broadcast) wired to dispatcher
- [ ] Client reconnect + event replay (last N)
- [ ] Simple web dashboard (HTML/JS, served by daemon)

## Phase 4 — Plugin System

- [ ] WASM plugin runtime (wasmtime)
- [ ] Plugin manifest format (TOML)
- [ ] Plugin SDK (Rust + WASM target)
- [ ] Example plugin: Garmin Connect (OAuth pull)
- [ ] Example plugin: Whoop API

## Phase 5 — Cross-Device Communication

- [ ] Notification routing: PC → Apple Watch (via companion)
- [ ] Gesture/input forwarding: Watch crown/tap → desktop events
- [ ] Presence detection: watch heartbeat → PC automation trigger
- [ ] Smart home bridge: MQTT output adapter