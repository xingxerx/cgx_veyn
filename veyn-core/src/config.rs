use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

// ── TOML file schema ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct TomlFile {
    #[serde(default)]
    server: TomlServer,
    #[serde(default)]
    security: TomlSecurity,
    #[serde(default)]
    adapters: TomlAdapters,
    #[serde(default)]
    logging: TomlLogging,
    #[serde(default)]
    plugins: TomlPlugins,
    #[serde(default)]
    mqtt: TomlMqtt,
    #[serde(default)]
    presence: TomlPresence,
    #[serde(default)]
    compression: TomlCompression,
}

#[derive(Debug, Deserialize, Default)]
struct TomlServer {
    port: Option<u16>,
    healthkit_port: Option<u16>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlSecurity {
    require_auth: Option<bool>,
    token_path: Option<String>,
    cors_origins: Option<Vec<String>>,
    audit_log_path: Option<String>,
    strip_raw_hid: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlAdapters {
    mock: Option<bool>,
    ble: Option<bool>,
    eeg: Option<bool>,
    osc_port: Option<u16>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlLogging {
    level: Option<String>,
    jsonl_path: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlPlugins {
    dir: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlMqtt {
    url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlPresence {
    timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlCompression {
    rules_path: Option<String>,
    context_history_size: Option<usize>,
    debounce_ms: Option<HashMap<String, u64>>,
    epsilons: Option<HashMap<String, f64>>,
}

// ── Public Config ─────────────────────────────────────────────────────────────

/// Resolved runtime configuration.
/// Priority: CLI flags > env vars > veyn.toml > defaults.
#[derive(Debug, Clone)]
pub struct Config {
    pub api_port: u16,
    pub healthkit_port: u16,
    pub mock_mode: bool,
    pub ble_enabled: bool,
    pub eeg_enabled: bool,
    pub osc_port: u16,
    pub jsonl_path: String,
    pub plugins_dir: String,
    pub mqtt_url: Option<String>,
    pub presence_timeout_secs: u64,
    pub require_auth: bool,
    pub token_path: Option<String>,
    pub cors_origins: Vec<String>,
    pub audit_log_path: Option<String>,
    pub strip_raw_hid: bool,
    pub rules_path: String,
    pub context_history_size: usize,
    pub debounce_ms: HashMap<String, u64>,
    pub epsilons: HashMap<String, f64>,
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_port: 7700,
            healthkit_port: 7701,
            mock_mode: false,
            ble_enabled: false,
            eeg_enabled: false,
            osc_port: 9000,
            jsonl_path: "veyn-events.jsonl".to_string(),
            plugins_dir: "plugins".to_string(),
            mqtt_url: None,
            presence_timeout_secs: 30,
            require_auth: true,
            token_path: None,
            cors_origins: vec![],
            audit_log_path: None,
            strip_raw_hid: true,
            rules_path: "rules.toml".to_string(),
            context_history_size: 32,
            debounce_ms: default_debounce_ms(),
            epsilons: default_epsilons(),
            log_level: "info".to_string(),
        }
    }
}

fn default_debounce_ms() -> HashMap<String, u64> {
    [
        ("heart_rate", 1_000u64),
        ("hrv", 2_000),
        ("spo2", 5_000),
        ("steps", 500),
        ("respiratory_rate", 3_000),
        ("skin_temperature", 10_000),
        ("battery", 60_000),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

fn default_epsilons() -> HashMap<String, f64> {
    [
        ("heart_rate", 1.0f64),
        ("hrv", 2.0),
        ("spo2", 0.5),
        ("steps", 10.0),
        ("respiratory_rate", 0.5),
        ("skin_temperature", 0.1),
        ("battery", 1.0),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub fn load(toml_path: Option<&str>, cli_port: Option<u16>, cli_no_auth: bool) -> Result<Config> {
    let file = load_toml_file(toml_path)?;
    let mut cfg = Config::default();

    // Apply TOML values.
    if let Some(p) = file.server.port {
        cfg.api_port = p;
    }
    if let Some(p) = file.server.healthkit_port {
        cfg.healthkit_port = p;
    }
    if let Some(v) = file.security.require_auth {
        cfg.require_auth = v;
    }
    if let Some(v) = file.security.token_path {
        cfg.token_path = Some(v);
    }
    if let Some(v) = file.security.cors_origins {
        cfg.cors_origins = v;
    }
    if let Some(v) = file.security.audit_log_path {
        cfg.audit_log_path = Some(v);
    }
    if let Some(v) = file.security.strip_raw_hid {
        cfg.strip_raw_hid = v;
    }
    if let Some(v) = file.adapters.mock {
        cfg.mock_mode = v;
    }
    if let Some(v) = file.adapters.ble {
        cfg.ble_enabled = v;
    }
    if let Some(v) = file.adapters.eeg {
        cfg.eeg_enabled = v;
    }
    if let Some(v) = file.adapters.osc_port {
        cfg.osc_port = v;
    }
    if let Some(v) = file.logging.jsonl_path {
        cfg.jsonl_path = v;
    }
    if let Some(v) = file.logging.level {
        cfg.log_level = v;
    }
    if let Some(v) = file.plugins.dir {
        cfg.plugins_dir = v;
    }
    if let Some(v) = file.mqtt.url {
        cfg.mqtt_url = Some(v);
    }
    if let Some(v) = file.presence.timeout_secs {
        cfg.presence_timeout_secs = v;
    }
    if let Some(v) = file.compression.rules_path {
        cfg.rules_path = v;
    }
    if let Some(v) = file.compression.context_history_size {
        cfg.context_history_size = v;
    }
    if let Some(v) = file.compression.debounce_ms {
        cfg.debounce_ms.extend(v);
    }
    if let Some(v) = file.compression.epsilons {
        cfg.epsilons.extend(v);
    }

    // Overlay environment variables.
    if let Some(p) = env_u16("VEYN_PORT") {
        cfg.api_port = p;
    }
    if let Some(p) = env_u16("VEYN_HK_PORT") {
        cfg.healthkit_port = p;
    }
    if env_bool("VEYN_MOCK") {
        cfg.mock_mode = true;
    }
    if env_bool("VEYN_BLE") {
        cfg.ble_enabled = true;
    }
    if env_bool("VEYN_EEG") {
        cfg.eeg_enabled = true;
    }
    if let Some(p) = env_u16("VEYN_OSC_PORT") {
        cfg.osc_port = p;
    }
    if let Ok(v) = std::env::var("VEYN_LOG") {
        cfg.jsonl_path = v;
    }
    if let Ok(v) = std::env::var("VEYN_PLUGINS_DIR") {
        cfg.plugins_dir = v;
    }
    if let Ok(v) = std::env::var("VEYN_MQTT_URL") {
        cfg.mqtt_url = Some(v);
    }
    if let Some(v) = env_u64("VEYN_PRESENCE_TIMEOUT") {
        cfg.presence_timeout_secs = v;
    }
    if env_bool("VEYN_NO_AUTH") {
        cfg.require_auth = false;
    }

    // CLI overrides (highest priority).
    if let Some(p) = cli_port {
        cfg.api_port = p;
    }
    if cli_no_auth {
        cfg.require_auth = false;
    }

    Ok(cfg)
}

fn load_toml_file(path: Option<&str>) -> Result<TomlFile> {
    if let Some(p) = path {
        let content = std::fs::read_to_string(p)?;
        return Ok(toml::from_str(&content)?);
    }
    for candidate in default_config_paths() {
        if candidate.exists() {
            let content = std::fs::read_to_string(&candidate)?;
            return Ok(toml::from_str(&content)?);
        }
    }
    Ok(TomlFile::default())
}

fn default_config_paths() -> Vec<std::path::PathBuf> {
    let mut paths = vec![Path::new("veyn.toml").to_path_buf()];
    if let Ok(home) = std::env::var("HOME") {
        paths.push(
            Path::new(&home)
                .join(".config")
                .join("veyn")
                .join("veyn.toml"),
        );
    }
    paths
}

fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

fn env_u16(key: &str) -> Option<u16> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.parse().ok()
}
