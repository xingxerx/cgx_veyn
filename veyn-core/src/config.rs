/// Runtime configuration resolved from environment variables.
pub struct Config {
    /// Port for the REST / WebSocket API (default 7700)
    pub api_port: u16,
    /// Port on which the HealthKit TCP relay listens (default 7701)
    pub healthkit_port: u16,
    /// Emit synthetic events from the mock adapter (VEYN_MOCK=true)
    pub mock_mode: bool,
    /// Enable the BLE adapter (VEYN_BLE=true)
    pub ble_enabled: bool,
    /// Enable the EEG/OSC adapter (VEYN_EEG=true)
    pub eeg_enabled: bool,
    /// UDP port for OSC / EEG input (VEYN_OSC_PORT, default 9000)
    pub osc_port: u16,
    /// Path for the append-only JSONL event log (VEYN_LOG)
    pub jsonl_path: String,
}

fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

fn env_u16(key: &str, default: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_port:        env_u16("VEYN_PORT", 7700),
            healthkit_port:  env_u16("VEYN_HK_PORT", 7701),
            mock_mode:       env_bool("VEYN_MOCK"),
            ble_enabled:     env_bool("VEYN_BLE"),
            eeg_enabled:     env_bool("VEYN_EEG"),
            osc_port:        env_u16("VEYN_OSC_PORT", 9000),
            jsonl_path:      std::env::var("VEYN_LOG")
                                 .unwrap_or_else(|_| "veyn-events.jsonl".into()),
        }
    }
}
