use serde::Deserialize;
use serde_json::Value;
use veyn_plugin_sdk::{http_get, log_error, log_info, today_date, VeynEvent, VeynPlugin};

pub struct GarminPlugin {
    access_token: String,
    user_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DailySummary {
    total_steps: Option<u64>,
    total_kilocalories: Option<f64>,
    average_heart_rate_in_beats_per_minute: Option<u32>,
    resting_heart_rate_in_beats_per_minute: Option<u32>,
    max_heart_rate_in_beats_per_minute: Option<u32>,
    #[serde(rename = "averageSpO2")]
    average_spo2: Option<f64>,
    total_distance_meters: Option<f64>,
    active_kilocalories: Option<f64>,
}

impl VeynPlugin for GarminPlugin {
    fn init(config: Value) -> Result<Self, String> {
        let access_token = config["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("garmin-connect: missing or empty 'access_token' in config")?
            .to_owned();
        let user_id = config["user_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("garmin-connect: missing or empty 'user_id' in config")?
            .to_owned();

        log_info(&format!("garmin-connect plugin initialised for user {}", user_id));
        Ok(Self { access_token, user_id })
    }

    fn poll(&mut self) -> Vec<VeynEvent> {
        let date = today_date();
        let url = format!(
            "https://connect.garmin.com/modern/proxy/usersummary-service/usersummary/daily/{}?calendarDate={}",
            self.user_id, date
        );

        log_info(&format!("garmin-connect: fetching daily summary for {}", date));

        let body = match http_get(&url, Some(&self.access_token)) {
            Ok(b) => b,
            Err(e) => {
                log_error(&format!("garmin-connect: http_get failed: {}", e));
                return vec![];
            }
        };

        let summary: DailySummary = match serde_json::from_slice(&body) {
            Ok(s) => s,
            Err(e) => {
                log_error(&format!("garmin-connect: parse error: {}", e));
                return vec![];
            }
        };

        let device_id = format!("garmin-{}", self.user_id);
        let mut events = Vec::new();

        if let Some(v) = summary.total_steps {
            events.push(VeynEvent::new(&device_id, "garmin", "steps", v as f64, "steps"));
        }
        if let Some(v) = summary.total_kilocalories {
            events.push(VeynEvent::new(&device_id, "garmin", "calories_total", v, "kcal"));
        }
        if let Some(v) = summary.active_kilocalories {
            events.push(VeynEvent::new(&device_id, "garmin", "calories_active", v, "kcal"));
        }
        if let Some(v) = summary.average_heart_rate_in_beats_per_minute {
            events.push(VeynEvent::new(&device_id, "garmin", "heart_rate_avg", v as f64, "bpm"));
        }
        if let Some(v) = summary.resting_heart_rate_in_beats_per_minute {
            events.push(VeynEvent::new(&device_id, "garmin", "heart_rate_resting", v as f64, "bpm"));
        }
        if let Some(v) = summary.max_heart_rate_in_beats_per_minute {
            events.push(VeynEvent::new(&device_id, "garmin", "heart_rate_max", v as f64, "bpm"));
        }
        if let Some(v) = summary.average_spo2 {
            events.push(VeynEvent::new(&device_id, "garmin", "spo2", v, "%"));
        }
        if let Some(v) = summary.total_distance_meters {
            events.push(VeynEvent::new(&device_id, "garmin", "distance", v, "m"));
        }

        log_info(&format!("garmin-connect: emitting {} events", events.len()));
        events
    }
}

veyn_plugin_sdk::veyn_register_plugin!(GarminPlugin);
