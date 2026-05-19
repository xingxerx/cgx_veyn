use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ── ContextTier ───────────────────────────────────────────────────────────────

/// Controls which layer of data a token (or the daemon default) exposes.
///
/// - `Raw`      — full `VeynEvent` stream, unfiltered
/// - `Filtered` — compression-filtered events only (delta + debounce applied)
/// - `Semantic` — only `ContextSnapshot`; raw events are not exposed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ContextTier {
    Raw,
    Filtered,
    #[default]
    Semantic,
}

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
    #[serde(default)]
    memory: TomlMemory,
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
    rate_limit_rps: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlAdapters {
    mock: Option<bool>,
    ble: Option<bool>,
    eeg: Option<bool>,
    osc_port: Option<u16>,
    evdev: Option<bool>,
    hidraw: Option<bool>,
    midi: Option<bool>,
    serial_port: Option<String>,
    serial_baud: Option<u32>,
    fs_watch_paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlLogging {
    level: Option<String>,
    jsonl_path: Option<String>,
    db_path: Option<String>,
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
    context_tier: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlMemory {
    enabled: Option<bool>,
    ambient_interval_secs: Option<u64>,
    max_records: Option<usize>,
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
    pub evdev_enabled: bool,
    pub hidraw_enabled: bool,
    pub midi_enabled: bool,
    pub serial_port: Option<String>,
    pub serial_baud: u32,
    pub fs_watch_paths: Vec<String>,
    pub jsonl_path: String,
    pub plugins_dir: String,
    pub mqtt_url: Option<String>,
    pub presence_timeout_secs: u64,
    pub require_auth: bool,
    pub token_path: Option<String>,
    pub cors_origins: Vec<String>,
    pub audit_log_path: Option<String>,
    pub strip_raw_hid: bool,
    /// Max requests per second per client IP; None means no rate limit.
    pub rate_limit_rps: Option<u32>,
    pub rules_path: String,
    pub context_history_size: usize,
    pub debounce_ms: HashMap<String, u64>,
    pub epsilons: HashMap<String, f64>,
    pub log_level: String,
    /// Path to SQLite database for session and baseline persistence.
    pub db_path: String,
    /// Default context tier for the daemon; tokens may declare a ceiling equal
    /// to or below this level.
    pub context_tier: ContextTier,

    // ── Memory layer ──────────────────────────────────────────────────────────
    /// Enable the biometric memory layer (ambient writer + REST endpoints).
    pub memory_enabled: bool,
    /// How often the ambient writer fires (seconds).
    pub memory_ambient_interval_secs: u64,
    /// Maximum number of Ambient records to retain; oldest are pruned when exceeded.
    /// Semantic records are never auto-pruned.
    pub memory_max_records: usize,
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
            evdev_enabled: false,
            hidraw_enabled: false,
            midi_enabled: false,
            serial_port: None,
            serial_baud: 115200,
            fs_watch_paths: Vec::new(),
            jsonl_path: "veyn-events.jsonl".to_string(),
            plugins_dir: "plugins".to_string(),
            mqtt_url: None,
            presence_timeout_secs: 30,
            require_auth: true,
            token_path: None,
            cors_origins: vec![],
            audit_log_path: None,
            strip_raw_hid: true,
            rate_limit_rps: Some(100),
            rules_path: "rules.toml".to_string(),
            context_history_size: 32,
            debounce_ms: default_debounce_ms(),
            epsilons: default_epsilons(),
            log_level: "info".to_string(),
            db_path: "veyn.db".to_string(),
            context_tier: ContextTier::Semantic,
            memory_enabled: true,
            memory_ambient_interval_secs: 900,
            memory_max_records: 10_000,
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
    if let Some(v) = file.security.rate_limit_rps {
        cfg.rate_limit_rps = if v == 0 { None } else { Some(v) };
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
    if let Some(v) = file.adapters.evdev {
        cfg.evdev_enabled = v;
    }
    if let Some(v) = file.adapters.hidraw {
        cfg.hidraw_enabled = v;
    }
    if let Some(v) = file.adapters.midi {
        cfg.midi_enabled = v;
    }
    if let Some(v) = file.adapters.serial_port {
        cfg.serial_port = Some(v);
    }
    if let Some(v) = file.adapters.serial_baud {
        cfg.serial_baud = v;
    }
    if let Some(v) = file.adapters.fs_watch_paths {
        cfg.fs_watch_paths = v;
    }
    if let Some(v) = file.logging.jsonl_path {
        cfg.jsonl_path = v;
    }
    if let Some(v) = file.logging.db_path {
        cfg.db_path = v;
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
    if let Some(v) = file.compression.context_tier {
        cfg.context_tier = parse_context_tier(&v);
    }
    if let Some(v) = file.memory.enabled {
        cfg.memory_enabled = v;
    }
    if let Some(v) = file.memory.ambient_interval_secs {
        cfg.memory_ambient_interval_secs = v;
    }
    if let Some(v) = file.memory.max_records {
        cfg.memory_max_records = v;
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
    if let Ok(v) = std::env::var("VEYN_CONTEXT_TIER") {
        cfg.context_tier = parse_context_tier(&v);
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

pub fn parse_context_tier(s: &str) -> ContextTier {
    match s.trim().to_lowercase().as_str() {
        "raw" => ContextTier::Raw,
        "filtered" => ContextTier::Filtered,
        _ => ContextTier::Semantic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::io::Write;
    use std::sync::Mutex;
    use tempfile::NamedTempFile;

    // Environment variables are global state and tests run concurrently by default.
    // Instead of using global env modifications which lead to flaky tests, we use
    // a mock lock to serialize env-var testing.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn run_isolated<F>(test: F)
    where
        F: FnOnce(),
    {
        let _guard = ENV_LOCK.lock().unwrap();

        // Clean environment
        let keys = vec![
            "VEYN_PORT",
            "VEYN_HK_PORT",
            "VEYN_MOCK",
            "VEYN_BLE",
            "VEYN_EEG",
            "VEYN_OSC_PORT",
            "VEYN_LOG",
            "VEYN_PLUGINS_DIR",
            "VEYN_MQTT_URL",
            "VEYN_PRESENCE_TIMEOUT",
            "VEYN_NO_AUTH",
            "VEYN_CONTEXT_TIER",
        ];
        for k in &keys {
            env::remove_var(k);
        }
        test();
        for k in &keys {
            env::remove_var(k);
        }
    }

    #[test]
    fn test_default_config() {
        run_isolated(|| {
            let config = load(None, None, false).unwrap();
            assert_eq!(config.api_port, 7700);
            assert_eq!(config.healthkit_port, 7701);
            assert!(!config.mock_mode);
            assert!(!config.ble_enabled);
            assert!(config.require_auth);
            assert_eq!(config.context_tier, ContextTier::Semantic);
            assert!(config.memory_enabled);
        });
    }

    #[test]
    fn test_load_from_toml() {
        run_isolated(|| {
            let mut file = NamedTempFile::new().unwrap();
            writeln!(
                file,
                r#"
[server]
port = 8800
healthkit_port = 8801

[security]
require_auth = false
token_path = "/tmp/token"
cors_origins = ["http://localhost:3000"]
audit_log_path = "/tmp/audit.log"
strip_raw_hid = false
rate_limit_rps = 200

[adapters]
mock = true
ble = true
eeg = true
osc_port = 9001
evdev = true
hidraw = true
midi = true
serial_port = "/dev/ttyUSB0"
serial_baud = 9600
fs_watch_paths = ["/tmp/watch"]

[logging]
jsonl_path = "/tmp/logs.jsonl"
db_path = "/tmp/veyn.db"
level = "debug"

[plugins]
dir = "/tmp/plugins"

[mqtt]
url = "mqtt://localhost:1883"

[presence]
timeout_secs = 60

[compression]
rules_path = "/tmp/rules.toml"
context_history_size = 64
debounce_ms = {{ heart_rate = 2000 }}
epsilons = {{ heart_rate = 0.5 }}
context_tier = "filtered"

[memory]
enabled = false
ambient_interval_secs = 600
max_records = 5000
"#
            )
            .unwrap();

            let config = load(Some(file.path().to_str().unwrap()), None, false).unwrap();

            assert_eq!(config.api_port, 8800);
            assert_eq!(config.healthkit_port, 8801);
            assert!(!config.require_auth);
            assert_eq!(config.token_path, Some("/tmp/token".to_string()));
            assert_eq!(
                config.cors_origins,
                vec!["http://localhost:3000".to_string()]
            );
            assert_eq!(config.audit_log_path, Some("/tmp/audit.log".to_string()));
            assert!(!config.strip_raw_hid);
            assert_eq!(config.rate_limit_rps, Some(200));

            assert!(config.mock_mode);
            assert!(config.ble_enabled);
            assert!(config.eeg_enabled);
            assert_eq!(config.osc_port, 9001);
            assert!(config.evdev_enabled);
            assert!(config.hidraw_enabled);
            assert!(config.midi_enabled);
            assert_eq!(config.serial_port, Some("/dev/ttyUSB0".to_string()));
            assert_eq!(config.serial_baud, 9600);
            assert_eq!(config.fs_watch_paths, vec!["/tmp/watch".to_string()]);

            assert_eq!(config.jsonl_path, "/tmp/logs.jsonl".to_string());
            assert_eq!(config.db_path, "/tmp/veyn.db".to_string());
            assert_eq!(config.log_level, "debug".to_string());

            assert_eq!(config.plugins_dir, "/tmp/plugins".to_string());
            assert_eq!(config.mqtt_url, Some("mqtt://localhost:1883".to_string()));
            assert_eq!(config.presence_timeout_secs, 60);

            assert_eq!(config.rules_path, "/tmp/rules.toml".to_string());
            assert_eq!(config.context_history_size, 64);
            assert_eq!(config.debounce_ms.get("heart_rate"), Some(&2000));
            assert_eq!(config.epsilons.get("heart_rate"), Some(&0.5));
            assert_eq!(config.context_tier, ContextTier::Filtered);

            assert!(!config.memory_enabled);
            assert_eq!(config.memory_ambient_interval_secs, 600);
            assert_eq!(config.memory_max_records, 5000);
        });
    }

    #[test]
    fn test_env_vars_override() {
        run_isolated(|| {
            env::set_var("VEYN_PORT", "9900");
            env::set_var("VEYN_HK_PORT", "9901");
            env::set_var("VEYN_MOCK", "true");
            env::set_var("VEYN_BLE", "1");
            env::set_var("VEYN_EEG", "yes");
            env::set_var("VEYN_OSC_PORT", "9902");
            env::set_var("VEYN_LOG", "/env/logs.jsonl");
            env::set_var("VEYN_PLUGINS_DIR", "/env/plugins");
            env::set_var("VEYN_MQTT_URL", "mqtt://env");
            env::set_var("VEYN_PRESENCE_TIMEOUT", "120");
            env::set_var("VEYN_NO_AUTH", "true");
            env::set_var("VEYN_CONTEXT_TIER", "raw");

            let config = load(None, None, false).unwrap();

            assert_eq!(config.api_port, 9900);
            assert_eq!(config.healthkit_port, 9901);
            assert!(config.mock_mode);
            assert!(config.ble_enabled);
            assert!(config.eeg_enabled);
            assert_eq!(config.osc_port, 9902);
            assert_eq!(config.jsonl_path, "/env/logs.jsonl".to_string());
            assert_eq!(config.plugins_dir, "/env/plugins".to_string());
            assert_eq!(config.mqtt_url, Some("mqtt://env".to_string()));
            assert_eq!(config.presence_timeout_secs, 120);
            assert!(!config.require_auth);
            assert_eq!(config.context_tier, ContextTier::Raw);
        });
    }

    #[test]
    fn test_cli_args_override() {
        run_isolated(|| {
            env::set_var("VEYN_PORT", "9900");
            env::set_var("VEYN_NO_AUTH", "false");

            let mut file = NamedTempFile::new().unwrap();
            writeln!(
                file,
                r#"
[server]
port = 8800
[security]
require_auth = true
"#
            )
            .unwrap();

            // CLI port 10000 should override env 9900 and toml 8800
            // CLI no_auth true should override env false and toml true
            let config = load(Some(file.path().to_str().unwrap()), Some(10000), true).unwrap();

            assert_eq!(config.api_port, 10000);
            assert!(!config.require_auth);
        });
    }

    #[test]
    fn test_parse_context_tier() {
        assert_eq!(parse_context_tier("raw"), ContextTier::Raw);
        assert_eq!(parse_context_tier("RAW"), ContextTier::Raw);
        assert_eq!(parse_context_tier("filtered"), ContextTier::Filtered);
        assert_eq!(parse_context_tier("FiLtErEd"), ContextTier::Filtered);
        assert_eq!(parse_context_tier("semantic"), ContextTier::Semantic);
        assert_eq!(parse_context_tier("unknown"), ContextTier::Semantic); // default
        assert_eq!(parse_context_tier(""), ContextTier::Semantic); // default
    }
}
