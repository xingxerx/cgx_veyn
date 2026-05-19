//! 13.1 — Direct Inference Hyperparameter Modulation
//!
//! Monitors the context broadcast channel and dynamically scales Ollama's
//! `temperature` and `top_k` down to `0.0` / `1` during `CognitiveLoad`
//! or `StressResponse` states, enforcing deterministic outputs.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde_json::json;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};
use veyn_schemas::{ContextSnapshot, IntentCode};

// ── State ─────────────────────────────────────────────────────────────────────

/// Current inference parameter set sent to Ollama.
#[derive(Debug, Clone, PartialEq)]
pub struct InferenceParams {
    pub temperature: f64,
    pub top_k: u32,
    pub intent_code: String,
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_k: 40,
            intent_code: "neutral".to_string(),
        }
    }
}

/// Shared inference state; AppState holds an `Arc<Mutex<InferenceState>>` for
/// `/v1/inference/params` debug reads.
#[derive(Debug, Default)]
pub struct InferenceState {
    pub current: InferenceParams,
    pub modulation_active: bool,
    pub modulation_count: u64,
}

// ── Hyperparameter resolution ─────────────────────────────────────────────────

fn params_for_intent(code: &IntentCode) -> InferenceParams {
    match code {
        IntentCode::CognitiveLoad | IntentCode::StressResponse => InferenceParams {
            temperature: 0.0,
            top_k: 1,
            intent_code: match code {
                IntentCode::CognitiveLoad => "cognitive_load",
                _ => "stress_response",
            }
            .to_string(),
        },
        IntentCode::Fatigue => InferenceParams {
            temperature: 0.1,
            top_k: 5,
            intent_code: "fatigue".to_string(),
        },
        IntentCode::Recovery => InferenceParams {
            temperature: 0.5,
            top_k: 20,
            intent_code: "recovery".to_string(),
        },
        _ => InferenceParams::default(),
    }
}

// ── Ollama push ───────────────────────────────────────────────────────────────

async fn push_to_ollama(ollama_url: &str, params: &InferenceParams) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    // Ollama doesn't have a live-param endpoint; we store the active params so
    // every outgoing /api/generate call (from SDK/agents) can be intercepted.
    // Here we POST the resolved params to the optional veyn-ollama-proxy sidecar.
    let proxy_url = format!("{}/veyn/params", ollama_url);
    let payload = json!({
        "temperature": params.temperature,
        "top_k":       params.top_k,
        "intent_code": params.intent_code,
    });
    match client.post(&proxy_url).json(&payload).send().await {
        Ok(resp) => {
            debug!(status = %resp.status(), "pushed inference params to Ollama proxy");
        }
        Err(e) => {
            // Proxy not running — this is expected in non-LLM setups.
            debug!("Ollama proxy not reachable (expected if not running): {e}");
        }
    }
    Ok(())
}

// ── Background task ───────────────────────────────────────────────────────────

/// Spawnable background task.  Watches the context broadcast channel and
/// updates `InferenceState` + pushes to the Ollama proxy whenever the intent
/// code changes.
pub async fn run_modulator(
    mut rx: broadcast::Receiver<ContextSnapshot>,
    state: Arc<Mutex<InferenceState>>,
    ollama_url: String,
) {
    let mut last_code = IntentCode::Neutral;

    loop {
        let snap = match rx.recv().await {
            Ok(s) => s,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!("inference modulator lagged {n} snapshots");
                continue;
            }
            Err(_) => break,
        };

        if snap.intent_code == last_code {
            continue; // no change — skip
        }
        last_code = snap.intent_code.clone();

        let params = params_for_intent(&snap.intent_code);
        let modulation_active = params.temperature < 0.7;

        {
            let mut s = state.lock().unwrap();
            let changed = s.current != params;
            s.current = params.clone();
            s.modulation_active = modulation_active;
            if changed {
                s.modulation_count += 1;
                info!(
                    intent   = %params.intent_code,
                    temp     = params.temperature,
                    top_k    = params.top_k,
                    active   = modulation_active,
                    "inference hyperparameters updated"
                );
            }
        }

        if let Err(e) = push_to_ollama(&ollama_url, &params).await {
            warn!("failed to push params to Ollama: {e}");
        }
    }
}
