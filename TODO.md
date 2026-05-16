# VEYN — Master TODO

> Roadmap to a perfect, runnable-out-of-the-box build.
> Priority: 

🔴 Critical (blocks launch) → 
🟡 High (needed for safety/correctness) → 
🟢 Nice-to-have

-----

## 1. 🔴 Critical: Semantic Compression Engine (The Signal Firehose Fix)

The biggest architectural gap. Raw HID/BLE/MIDI data at 100–1000 Hz is completely unusable by any AI agent.

- [ ] 🟢 Optional: lightweight local SLM integration (e.g. `llama.cpp` via FFI or subprocess) as a secondary synthesis pass for ambiguous intent classification

-----

## 2. 🔴 Critical: Security — Kill the Local Keylogger Attack Surface

An unauthenticated WS server on `:7700` exposing raw HID is spyware-grade risk.

- [ ] 🟡 Add **scope-limited tokens** — a token can be created with read-only or specific device-class access (e.g. MIDI only, no HID)
- [ ] 🟡 Add **rate limiting** on the REST endpoints (e.g. max 100 req/s per client IP using a token bucket)
- [ ] 🟢 Consider adding mutual TLS (mTLS) option for high-security deployments

-----

## 3. 🔴 Critical: WASM Plugin Architecture Fix

WASM is sandboxed — it cannot touch host hardware without WASI host functions.

- [ ] 🔴 Add a **Device Proxy Layer** — plugins register a device descriptor (VID/PID, BLE UUID, serial pattern); the core daemon handles actual OS-level device open and passes byte buffers into the WASM sandbox
- [ ] 🟡 Add **plugin signature verification** — each plugin WASM binary must be signed; core validates on load
- [ ] 🟡 Add a `veyn plugin install <path>` CLI subcommand
- [ ] 🟢 Publish example plugin: `veyn-plugin-midi-launchpad` as reference implementation

-----

## 4. 🟡 High: `veyn-core` Daemon Foundation

- [ ] 🟡 Implement device hot-plug/unplug detection — daemon must not crash when a device disconnects mid-session
- [ ] 🟢 Add Prometheus metrics endpoint `/metrics` for integration with Grafana/monitoring stacks

-----

## 5. 🟡 High: `veyn-adapters` — Platform Coverage

- [ ] 🔴 Linux: `evdev` HID adapter — keyboard, mouse, gamepad via `/dev/input/event*`
- [ ] 🔴 Linux: `hidraw` adapter for raw USB HID
- [ ] 🟡 macOS: `IOKit`/`IOHIDManager` adapter
- [ ] 🟡 Windows: `WinUSB`/`RawInput` adapter
- [ ] 🟡 MIDI adapter (`midir` crate) — CC events, note on/off, clock
- [ ] 🟡 Serial/UART adapter (`serialport` crate) — configurable baud, parity, stop bits
- [ ] 🟡 Filesystem watcher adapter (`notify` crate) — emit events on file create/modify/delete for specified paths
- [ ] 🟢 OSC (Open Sound Control) input adapter — for DAW/VJ software integration
- [ ] 🟢 Audio level adapter — RMS/peak metering from default input device (via `cpal`)

-----

## 6. 🟡 High: `/context/current` API Contract

- [ ] 🟡 Add OpenAPI 3.0 spec (`openapi.yaml`) — auto-generate or hand-write, keep in sync
- [ ] 🟡 WebSocket stream: support subscribe filtering — clients can request only specific device classes or intent categories
- [ ] 🟢 Add a Server-Sent Events (SSE) alternative to WebSocket for simpler HTTP-only clients

-----

## 7. 🟡 High: SDK (`/sdk`)

- [ ] 🟡 Add TypeScript/Node.js SDK — connect, auth, subscribe to context stream, typed interfaces
- [ ] 🟡 Add Python SDK — `pip install veyn-sdk`, async context manager, typed dataclasses
- [ ] 🟡 Add SDK usage examples for: connecting an LLM agent, building a plugin, reading context history
- [ ] 🟢 Add Go SDK

-----

## 8. 🟢 Developer Experience

- [ ] Add `docker-compose.yml` for a local dev stack (daemon + example consumer agent)
- [ ] Add integration test suite — spin up daemon in test mode, connect mock devices, assert context snapshots
- [ ] Add a `veyn doctor` CLI command — checks prerequisites, permissions, device access, token validity
- [ ] Add a minimal web UI (`localhost:7700/ui`) — live context feed, connected devices, log tail (HTML/JS, no framework)

-----

## 9. 🟢 Phase 7+ AI Integration Prep

- [ ] Design and document the **Agent Handshake Protocol** — how an AI agent authenticates, declares its capability needs, and subscribes to the right context tier
- [ ] Add a `/context/subscribe` endpoint with declarative filter DSL (e.g. `{ "intents": ["idle", "context_switch"], "min_confidence": 0.7 }`)
- [ ] Add `context_tier` config: `raw` | `filtered` | `semantic` — agents get the tier they're authorized for
- [ ] Document recommended integration patterns for: Claude (via MCP), local Ollama agents, OpenAI function calling
