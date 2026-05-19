//! OSC Output Adapter — sends OSC messages downstream to DAW/VJ software.
//!
//! Configured via `[osc_output]` section in `veyn.toml`:
//! ```toml
//! [osc_output]
//! enabled = true
//! target_host = "127.0.0.1"
//! target_port = 8000
//! # Optional: map intent codes to OSC addresses
//! # intent_map = { approach = "/veyn/approach", avoidance = "/veyn/avoidance" }
//! ```

use anyhow::{Context, Result};
use async_trait::async_trait;
use rosc::{OscMessage, OscPacket};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};

use veyn_schemas::{ContextSnapshot, IntentCode, VeynEvent};

use crate::VeynAdapter;

/// Configuration for the OSC output adapter.
#[derive(Debug, Clone, Deserialize)]
pub struct OscOutputConfig {
    /// Enable/disable the adapter.
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// Target host IP or hostname.
    #[serde(default = "default_host")]
    pub target_host: String,
    /// Target UDP port.
    #[serde(default = "default_port")]
    pub target_port: u16,
    /// Optional mapping from intent codes to OSC addresses.
    #[serde(default)]
    pub intent_map: HashMap<String, String>,
}

fn default_false() -> bool {
    false
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8000
}

impl Default for OscOutputConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_host: default_host(),
            target_port: default_port(),
            intent_map: HashMap::new(),
        }
    }
}

/// OSC output adapter that sends biometric events and context snapshots as OSC messages.
pub struct OscOutputAdapter {
    config: Arc<OscOutputConfig>,
    socket: Option<UdpSocket>,
    ctx_rx: Option<watch::Receiver<Option<ContextSnapshot>>>,
}

impl OscOutputAdapter {
    /// Create a new OSC output adapter.
    pub fn new(
        config: OscOutputConfig,
        ctx_rx: Option<watch::Receiver<Option<ContextSnapshot>>>,
    ) -> Result<Self> {
        if !config.enabled {
            return Ok(Self {
                config: Arc::new(config),
                socket: None,
                ctx_rx,
            });
        }

        let socket = UdpSocket::bind("0.0.0.0:0")
            .context("Failed to bind UDP socket for OSC output")?;
        socket
            .connect(format!("{}:{}", config.target_host, config.target_port))
            .context("Failed to connect OSC output socket")?;

        info!(
            "OSC output adapter configured: {}:{} → {}:{}",
            config.target_host, config.target_port, config.target_host, config.target_port
        );

        Ok(Self {
            config: Arc::new(config),
            socket: Some(socket),
            ctx_rx,
        })
    }

    /// Send an OSC message.
    fn send_osc(&self, addr: &str, args: Vec<OscMessage>) -> Result<()> {
        let Some(socket) = &self.socket else {
            return Ok(()); // Adapter disabled
        };

        let packet = OscPacket::Bundle(rosc::OscBundle {
            timetag: rosc::OscTime::from_system_time(std::time::SystemTime::now()),
            content: vec![rosc::OscMessage {
                addr: addr.to_string(),
                args,
            }],
        });

        let buf = rosc::encoder::encode(&packet)?;
        socket.send(&buf)?;
        debug!("OSC sent: {}", addr);
        Ok(())
    }

    /// Get OSC address for an intent code.
    fn intent_to_address(&self, intent: &IntentCode) -> String {
        let key = match intent {
            IntentCode::Neutral => "neutral",
            IntentCode::CognitiveLoad => "cognitive_load",
            IntentCode::StressResponse => "stress_response",
            IntentCode::Approach => "approach",
            IntentCode::Avoidance => "avoidance",
            IntentCode::Fatigue => "fatigue",
            IntentCode::Recovery => "recovery",
            IntentCode::Other(raw) => raw.as_str(),
        };

        self.config
            .intent_map
            .get(key)
            .cloned()
            .unwrap_or_else(|| format!("/veyn/{}", key))
    }

    /// Run the context snapshot watcher task.
    async fn run_context_watcher(
        mut rx: watch::Receiver<Option<ContextSnapshot>>,
        config: Arc<OscOutputConfig>,
        socket: Arc<UdpSocket>,
    ) {
        let mut last_intent: Option<IntentCode> = None;

        while rx.changed().await.is_ok() {
            let Some(snapshot) = rx.borrow().clone() else {
                continue;
            };

            // Send intent transition
            if last_intent.as_ref() != Some(&snapshot.intent_code) {
                let addr = format!("{}/intent", config.target_host);
                let msg = OscMessage {
                    addr: "/veyn/intent".to_string(),
                    args: vec![
                        OscMessage::new("s", snapshot.intent_code.to_string()).unwrap(),
                        OscMessage::new("f", snapshot.intent_confidence).unwrap(),
                    ],
                };

                let packet = OscPacket::Message(msg);
                if let Ok(buf) = rosc::encoder::encode(&packet) {
                    let _ = socket.send_to(&buf, format!("{}:{}", config.target_host, config.target_port));
                }

                // Also send on mapped address if configured
                let mapped_addr = match &snapshot.intent_code {
                    IntentCode::Neutral => config.intent_map.get("neutral"),
                    IntentCode::CognitiveLoad => config.intent_map.get("cognitive_load"),
                    IntentCode::StressResponse => config.intent_map.get("stress_response"),
                    IntentCode::Approach => config.intent_map.get("approach"),
                    IntentCode::Avoidance => config.intent_map.get("avoidance"),
                    IntentCode::Fatigue => config.intent_map.get("fatigue"),
                    IntentCode::Recovery => config.intent_map.get("recovery"),
                    IntentCode::Other(raw) => config.intent_map.get(raw.as_str()),
                };

                if let Some(mapped) = mapped_addr {
                    let msg = OscMessage {
                        addr: mapped.clone(),
                        args: vec![OscMessage::new("f", snapshot.intent_confidence).unwrap()],
                    };
                    let packet = OscPacket::Message(msg);
                    if let Ok(buf) = rosc::encoder::encode(&packet) {
                        let _ = socket.send_to(&buf, format!("{}:{}", config.target_host, config.target_port));
                    }
                }

                last_intent = Some(snapshot.intent_code.clone());
            }

            // Send baseline deltas if available
            if let Some(deltas) = &snapshot.baseline_delta {
                for (metric, zscore) in deltas {
                    let msg = OscMessage {
                        addr: format!("/veyn/baseline/{}", metric.replace('.', "_")),
                        args: vec![OscMessage::new("f", *zscore).unwrap()],
                    };
                    let packet = OscPacket::Message(msg);
                    if let Ok(buf) = rosc::encoder::encode(&packet) {
                        let _ = socket.send_to(&buf, format!("{}:{}", config.target_host, config.target_port));
                    }
                }
            }
        }
    }
}

#[async_trait]
impl VeynAdapter for OscOutputAdapter {
    fn name(&self) -> &str {
        "osc_output"
    }

    async fn start(&self, _tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        if !self.config.enabled {
            debug!("OSC output adapter disabled");
            return Ok(());
        }

        let Some(ctx_rx) = self.ctx_rx.clone() else {
            warn!("OSC output adapter has no context snapshot channel");
            return Ok(());
        };

        let socket = self.socket.clone().map(Arc::new).unwrap();
        let config = Arc::clone(&self.config);

        // Run context watcher in background
        tokio::spawn(async move {
            Self::run_context_watcher(ctx_rx, config, socket).await;
        });

        info!("OSC output adapter started");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OscOutputConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.target_host, "127.0.0.1");
        assert_eq!(config.target_port, 8000);
    }

    #[test]
    fn test_intent_to_address_default() {
        let config = OscOutputConfig::default();
        let adapter = OscOutputAdapter::new(config.clone(), None).unwrap();
        
        assert_eq!(adapter.intent_to_address(&IntentCode::Approach), "/veyn/approach");
        assert_eq!(adapter.intent_to_address(&IntentCode::Avoidance), "/veyn/avoidance");
    }

    #[test]
    fn test_intent_to_address_custom() {
        let mut intent_map = HashMap::new();
        intent_map.insert("approach".to_string(), "/custom/approach".to_string());
        
        let config = OscOutputConfig {
            enabled: true,
            intent_map,
            ..Default::default()
        };
        
        let adapter = OscOutputAdapter::new(config, None).unwrap();
        assert_eq!(adapter.intent_to_address(&IntentCode::Approach), "/custom/approach");
    }
}
