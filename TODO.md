# VEYN — Master TODO (v2.0 - Somatic OS Expansion)

> **North star:** Body state becomes ambient infrastructure — as available to software as location or voice, local, personal, compounding, and actionable. 
>
> Priority: 🔴 Critical (blocks launch) → 🟡 High → 🟢 Nice-to-have
> Build order within each section is top-to-bottom unless noted.

---

## ✅ Completed Foundation (Phases 1–10)
*The substrate is active. The daemon securely ingests multi-channel telemetry, normalizes it via the Semantic Compression Engine, enforces 256-bit token security, and persists biometric memory to SQLite.*
* **Compression & Intent:** `rules.toml` classification, z-score generation, `IntentCode` mapping.
* **Adapters:** BLE, OSC/EEG, evdev, hidraw, IOKit, WinUSB, MIDI, filesystem.
* **Memory Layer:** `MemoryStore`, ambient writer, semantic MCP auto-recall.
* **AI Handshake:** MCP tools, declarative SSE filter DSLs, tier scopes.

---

## What we're building toward

> **The physiological OS.** Five expressions of the same shift:
>
> 1. **Body-aware computing** — every app reads the context bus.
> 2. **Decisions with receipts** — somatic record of every choice.
> 3. **AI that already knows** — biometric memory pre-loaded.
> 4. **Environment that responds** — state-driven automation.
> 5. **Somatic Execution (NEW)** — the system actively throttles, schedules, and governs software execution based on the operator's biological limits.

Build order: **11 (Intero) → 12 (Somatic Shell) → 13 (Inference Gov) → 14 (Body-Aware) → 15 (Environment) → 16 (Longitudinal) → 17 (Hardening)**

---

## 11. 🔴 Critical: Intero App (The Anchor Product)
> Delivers: decisions with receipts · accuracy that compounds

- [ ] **11.1 Desktop shell (`intero/`)**: Scaffold Tauri app (Rust backend + WebView). Auto-launch VEYN daemon. Secure local token handoff. System tray intent indicators.
- [ ] **11.2 Live biometric dashboard**: SSE subscription to `/v1/context/subscribe`. Render `intent_code`, confidence, per-metric baseline z-scores, and scrolling signal strip charts.
- [ ] **11.3 Decision session UX**: Start/stop session flow with decision annotations. In-session live multi-channel strips. Session replay with intent timeline overlay and flat CSV exports.
- [ ] **11.4 Somatic feedback layer**: Approach/Avoidance indicator. Confidence-gated display (suppress UI when `confidence < 0.4`). Historical pattern panel matching past memory topics.
- [ ] **11.5 Ambient background mode**: Background daily digests (HRV trends vs baseline). Configurable Fatigue/Recovery nudges.

---

## 12. 🔴 Critical: The Somatic Shell & Execution Environment
> Delivers: biometric command gating · dynamic input tuning · human core scheduling

- [ ] **12.1 The Somatic CLI (Interpreter)**: Build a native terminal emulator/REPL in the TypeScript or Rust SDK that wraps standard shell executions. Hook into `/v1/context/current`.
- [ ] **12.2 Biometric Command Gating**: Implement interception logic. Block high-risk commands (e.g., `git push`, `cargo publish`, `rm -rf`) if the active z-scores trigger `Fatigue` or acute `StressResponse` thresholds, demanding a biological or manual override.
- [ ] **12.3 Biological Process Scheduler (Biometric `cron`)**: Implement a background task manager. Defer heavy background compilations, autonomous code generations, or system updates until the operator's context history ring buffer stabilizes into an `Approach` or `Recovery` vector.
- [ ] **12.4 Dynamic Input Micro-Jitter Adaptation**: Map `evdev`/`hidraw` keyboard debounce timings dynamically. When `CognitiveLoad` spikes, automatically increase mechanical debounce limits to filter stress-induced mis-types, and shift shell autocomplete rankings to prioritize low-complexity syntax.

---

## 13. 🔴 Critical: Inference Modulation & Governance (DGK-IES)
> Delivers: deterministic AI safety · real-time hyperparameter scaling

- [x] **13.1 Direct Inference Hyperparameter Modulation**: Build a direct server-to-server connection to local inference engines (Ollama `/api/generate`). Dynamically scale model `temperature` and `top_k` down to `0.0` during `CognitiveLoad` or `StressResponse` states to enforce strictly factual, deterministic outputs.
- [x] **13.2 Memory Pruning Endpoints**: Implement `MemoryStore::get` and `MemoryStore::delete` in `veyn-core/src/memory.rs`. Expose `GET /v1/memory/{id}` and `DELETE /v1/memory/{id}` in `routes.rs` to allow agents to aggressively forget conflicting or hallucinated semantic states.
- [x] **13.3 Cryptographic Invariant Enforcement (DGK-IES)**: Implement the Coherence (κ) density audit hook. Continuously monitor semantic memory logs; if system entropy pushes κ below 0.92, trigger a Mandatory Logical Reset (MLR) that drops agent execution privileges immediately.

---

## 14. 🟡 High: Body-Aware Computing Layer
> Delivers: multi-app ambient context

- [ ] **14.1 Multi-client subscription layer**: Add `client_id` tracking. Namespaced filter DSLs for concurrent local apps (e.g., DAW plugin + focus timer). `GET /v1/clients` debug endpoint.
- [ ] **14.2 Adaptive AI agent integration**: `veyn_suggest_action` MCP tool (validates proposed actions against current physiology). `context_degraded` SSE fallback events.
- [ ] **14.3 Ambient state broadcast**: Lightweight, loopback-only, auth-less `GET /v1/context/broadcast` SSE endpoint for zero-config trusted local apps.

---

## 15. 🟡 High: Environment Response Layer
> Delivers: smart environments that respond to state, not schedules

- [ ] **15.1 MQTT rules engine**: `[mqtt_output]` block in `rules.toml` mapping `intent_code` transitions to MQTT topic/payload pairs (e.g., `StressResponse` -> scene="calm"). Add rule debounce timers.
- [ ] **15.2 Multimedia Sinks**: Compile the `cpal` audio adapter for ambient RMS/peak ingestion. Develop the OSC output adapter to push live somatic z-scores down to DAW/VJ software.
- [ ] **15.3 Feedback loop tooling**: `POST /v1/rules/simulate` endpoint for rule testing. `rules.toml` MQTT hot-reload. Home Assistant integration guide.

---

## 16. 🟢 Nice-to-have: Longitudinal Analysis Pipeline
> Delivers: empirical RnD tools · compounding accuracy

- [ ] **16.1 Batch export API**: `GET /v1/export` full-resolution windows. `GET /v1/sessions/compare` multi-channel timelines for A/B testing past decisions.
- [ ] **16.2 Baseline intelligence**: `baseline_drift` SSE events when 7-day averages deviate > 1.5σ from 30-day norms. `GET /v1/baseline/summary` endpoint.
- [ ] **16.3 Research notebooks**: Jupyter files for HRV longitudinal mapping (`hrv_longitudinal.ipynb`) and session retrospective overlays.

---

## 17. 🟢 Nice-to-have: Ecosystem & Platform Hardening
> Delivers: multi-node trust boundaries · production telemetry

- [ ] **17.1 Mutual TLS (mTLS)**: Enforce Zero-Trust across multi-machine clusters via signed certificates in `veyn.toml`.
- [ ] **17.2 Plugin registry & WASM limits**: `registry.toml` schema for third-party `.wasm` modules. Cap memory/CPU time via `wasmtime::Config`. 
- [ ] **17.3 Infrastructure upgrades**: Structured `json` logs. Expanded Prometheus metrics (`GET /metrics`). Cross-platform GitHub Actions CI matrices. Go Language SDK (`sdk/go/`).
