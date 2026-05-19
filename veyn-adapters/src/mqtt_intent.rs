//! 15.1 — MQTT Intent-to-Action Rules Bridge
//!
//! Watches the `ContextSnapshot` broadcast channel and publishes MQTT messages
//! when the `intent_code` transitions between states.  Rules are defined in
//! `rules.toml` under `[[mqtt_output]]` blocks:
//!
//! ```toml
//! [[mqtt_output]]
//! intent_code  = "stress_response"
//! topic        = "homeassistant/scene/activate"
//! payload      = '{"scene": "calm"}'
//! debounce_ms  = 30000
//!
//! [[mqtt_output]]
//! intent_code  = "fatigue"
//! topic        = "homeassistant/scene/activate"
//! payload      = '{"scene": "wind_down"}'
//! debounce_ms  = 60000
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::Deserialize;
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use veyn_schemas::ContextSnapshot;

// ── Rule schema ──────────────────────────────────────────────────────────────

/// A single MQTT output rule parsed from `rules.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct MqttOutputRule {
    /// Intent code that triggers this rule (e.g. "stress_response").
    pub intent_code: String,
    /// MQTT topic to publish to.
    pub topic: String,
    /// JSON payload string to publish.
    pub payload: String,
    /// Minimum milliseconds between repeated fires of the same rule.
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_debounce_ms() -> u64 {
    30_000
}

/// Top-level rules file including the optional `[[mqtt_output]]` array.
#[derive(Debug, Deserialize, Default)]
pub struct RulesFileWithMqtt {
    #[serde(default)]
    pub mqtt_output: Vec<MqttOutputRule>,
}

/// Load `[[mqtt_output]]` rules from a rules TOML file.
pub fn load_mqtt_rules(path: &str) -> Vec<MqttOutputRule> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            warn!("cannot read rules file {path}: {e}");
            return Vec::new();
        }
    };
    match toml::from_str::<RulesFileWithMqtt>(&content) {
        Ok(f) => f.mqtt_output,
        Err(e) => {
            warn!("cannot parse mqtt_output from {path}: {e}");
            Vec::new()
        }
    }
}

// ── Bridge ───────────────────────────────────────────────────────────────────

/// Parse an `mqtt://host:port` URL into (host, port).
fn parse_url(url: &str) -> Result<(String, u16)> {
    let stripped = url.strip_prefix("mqtt://").unwrap_or(url);
    let (host, port_str) = stripped.rsplit_once(':').unwrap_or((stripped, "1883"));
    let port: u16 = port_str
        .parse()
        .with_context(|| format!("invalid MQTT port in URL: {}", url))?;
    Ok((host.to_string(), port))
}

/// Run the MQTT intent bridge.  Watches context snapshots and fires matching
/// `[[mqtt_output]]` rules with per-rule debounce timers.
pub async fn run(
    mut rx: broadcast::Receiver<ContextSnapshot>,
    url: String,
    rules: Vec<MqttOutputRule>,
) -> Result<()> {
    if rules.is_empty() {
        info!("MQTT intent bridge: no [[mqtt_output]] rules — skipping");
        return Ok(());
    }

    let (host, port) = parse_url(&url)?;
    info!(broker = %url, rule_count = rules.len(), "MQTT intent bridge starting");

    let mut opts = MqttOptions::new("veyn-intent", &host, port);
    opts.set_keep_alive(Duration::from_secs(30));

    let (client, mut eventloop) = AsyncClient::new(opts, 64);

    // Drive the rumqttc event loop in a background task.
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(_) => {}
                Err(e) => {
                    error!("MQTT intent event loop: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });

    info!(broker = %url, "MQTT intent bridge connected");

    // Per-rule debounce tracker: rule index → last fire time.
    let mut last_fired: HashMap<usize, Instant> = HashMap::new();
    let mut prev_intent: Option<String> = None;

    loop {
        match rx.recv().await {
            Ok(snapshot) => {
                let current_intent = intent_code_to_string(&snapshot.intent_code);

                // Only act on transitions.
                let transitioned = prev_intent.as_deref() != Some(&current_intent);
                prev_intent = Some(current_intent.clone());

                if !transitioned {
                    continue;
                }

                for (i, rule) in rules.iter().enumerate() {
                    if rule.intent_code != current_intent {
                        continue;
                    }

                    // Debounce check.
                    let debounce = Duration::from_millis(rule.debounce_ms);
                    if let Some(last) = last_fired.get(&i) {
                        if last.elapsed() < debounce {
                            continue;
                        }
                    }

                    info!(
                        intent = %current_intent,
                        topic = %rule.topic,
                        "MQTT intent rule fired"
                    );

                    if let Err(e) = client
                        .publish(&rule.topic, QoS::AtMostOnce, false, rule.payload.as_bytes())
                        .await
                    {
                        error!("MQTT intent publish error: {}", e);
                    }

                    last_fired.insert(i, Instant::now());
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("MQTT intent bridge lagged {} snapshots", n);
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}

fn intent_code_to_string(code: &veyn_schemas::IntentCode) -> String {
    match code {
        veyn_schemas::IntentCode::Neutral => "neutral".to_string(),
        veyn_schemas::IntentCode::CognitiveLoad => "cognitive_load".to_string(),
        veyn_schemas::IntentCode::StressResponse => "stress_response".to_string(),
        veyn_schemas::IntentCode::Approach => "approach".to_string(),
        veyn_schemas::IntentCode::Avoidance => "avoidance".to_string(),
        veyn_schemas::IntentCode::Fatigue => "fatigue".to_string(),
        veyn_schemas::IntentCode::Recovery => "recovery".to_string(),
        veyn_schemas::IntentCode::Other(s) => s.clone(),
    }
}
