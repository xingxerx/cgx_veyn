use serde::Deserialize;
use serde_json::Value;
use veyn_plugin_sdk::{http_get, log_error, log_info, VeynEvent, VeynPlugin};

const WHOOP_API: &str = "https://api.prod.whoop.com/developer/v1";

pub struct WhoopPlugin {
    access_token: String,
}

// ── API response types ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RecoveryCollection {
    records: Vec<RecoveryRecord>,
}

#[derive(Deserialize)]
struct RecoveryRecord {
    score: Option<RecoveryScore>,
    user_id: u64,
}

#[derive(Deserialize)]
struct RecoveryScore {
    recovery_score: f64,
    resting_heart_rate: f64,
    hrv_rmssd_milli: f64,
    spo2_percentage: Option<f64>,
    skin_temp_celsius: Option<f64>,
}

#[derive(Deserialize)]
struct SleepCollection {
    records: Vec<SleepRecord>,
}

#[derive(Deserialize)]
struct SleepRecord {
    score: Option<SleepScore>,
    user_id: u64,
}

#[derive(Deserialize)]
struct SleepScore {
    sleep_performance_percentage: f64,
    sleep_consistency_percentage: f64,
    sleep_efficiency_percentage: f64,
}

#[derive(Deserialize)]
struct CycleCollection {
    records: Vec<CycleRecord>,
}

#[derive(Deserialize)]
struct CycleRecord {
    score: Option<CycleScore>,
    user_id: u64,
}

#[derive(Deserialize)]
struct CycleScore {
    strain: f64,
    average_heart_rate: u32,
    max_heart_rate: u32,
    kilojoule: f64,
}

// ── Plugin implementation ─────────────────────────────────────────────────────

impl VeynPlugin for WhoopPlugin {
    fn init(config: Value) -> Result<Self, String> {
        let access_token = config["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("whoop: missing or empty 'access_token' in config")?
            .to_owned();

        log_info("whoop plugin initialised");
        Ok(Self { access_token })
    }

    fn poll(&mut self) -> Vec<VeynEvent> {
        let mut events = Vec::new();

        events.extend(self.fetch_recovery());
        events.extend(self.fetch_sleep());
        events.extend(self.fetch_strain());

        log_info(&format!("whoop: emitting {} events", events.len()));
        events
    }
}

impl WhoopPlugin {
    fn fetch_recovery(&self) -> Vec<VeynEvent> {
        let url = format!("{}/recovery?limit=1", WHOOP_API);
        let body = match http_get(&url, Some(&self.access_token)) {
            Ok(b) => b,
            Err(e) => { log_error(&format!("whoop recovery: {}", e)); return vec![]; }
        };

        let col: RecoveryCollection = match serde_json::from_slice(&body) {
            Ok(c) => c,
            Err(e) => { log_error(&format!("whoop recovery parse: {}", e)); return vec![]; }
        };

        col.records
            .into_iter()
            .filter_map(|r| r.score.map(|s| (r.user_id, s)))
            .flat_map(|(uid, s)| {
                let dev = format!("whoop-{}", uid);
                let mut evs = vec![
                    VeynEvent::new(&dev, "whoop", "recovery_score", s.recovery_score, "%"),
                    VeynEvent::new(&dev, "whoop", "heart_rate_resting", s.resting_heart_rate, "bpm"),
                    VeynEvent::new(&dev, "whoop", "hrv", s.hrv_rmssd_milli, "ms"),
                ];
                if let Some(v) = s.spo2_percentage {
                    evs.push(VeynEvent::new(&dev, "whoop", "spo2", v, "%"));
                }
                if let Some(v) = s.skin_temp_celsius {
                    evs.push(VeynEvent::new(&dev, "whoop", "skin_temp", v, "°C"));
                }
                evs
            })
            .collect()
    }

    fn fetch_sleep(&self) -> Vec<VeynEvent> {
        let url = format!("{}/activity/sleep?limit=1", WHOOP_API);
        let body = match http_get(&url, Some(&self.access_token)) {
            Ok(b) => b,
            Err(e) => { log_error(&format!("whoop sleep: {}", e)); return vec![]; }
        };

        let col: SleepCollection = match serde_json::from_slice(&body) {
            Ok(c) => c,
            Err(e) => { log_error(&format!("whoop sleep parse: {}", e)); return vec![]; }
        };

        col.records
            .into_iter()
            .filter_map(|r| r.score.map(|s| (r.user_id, s)))
            .flat_map(|(uid, s)| {
                let dev = format!("whoop-{}", uid);
                vec![
                    VeynEvent::new(&dev, "whoop", "sleep_performance", s.sleep_performance_percentage, "%"),
                    VeynEvent::new(&dev, "whoop", "sleep_consistency", s.sleep_consistency_percentage, "%"),
                    VeynEvent::new(&dev, "whoop", "sleep_efficiency", s.sleep_efficiency_percentage, "%"),
                ]
            })
            .collect()
    }

    fn fetch_strain(&self) -> Vec<VeynEvent> {
        let url = format!("{}/cycle?limit=1", WHOOP_API);
        let body = match http_get(&url, Some(&self.access_token)) {
            Ok(b) => b,
            Err(e) => { log_error(&format!("whoop cycle: {}", e)); return vec![]; }
        };

        let col: CycleCollection = match serde_json::from_slice(&body) {
            Ok(c) => c,
            Err(e) => { log_error(&format!("whoop cycle parse: {}", e)); return vec![]; }
        };

        col.records
            .into_iter()
            .filter_map(|r| r.score.map(|s| (r.user_id, s)))
            .flat_map(|(uid, s)| {
                let dev = format!("whoop-{}", uid);
                vec![
                    VeynEvent::new(&dev, "whoop", "strain", s.strain, ""),
                    VeynEvent::new(&dev, "whoop", "heart_rate_avg", s.average_heart_rate as f64, "bpm"),
                    VeynEvent::new(&dev, "whoop", "heart_rate_max", s.max_heart_rate as f64, "bpm"),
                    VeynEvent::new(&dev, "whoop", "kilojoules", s.kilojoule, "kJ"),
                ]
            })
            .collect()
    }
}

veyn_plugin_sdk::veyn_register_plugin!(WhoopPlugin);
