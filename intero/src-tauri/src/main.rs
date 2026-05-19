use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use dirs::home_dir;
use notify_rust::Notification;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tracing::{error, info, warn};

// ── Schema types (minimal subset for interop) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub timestamp_ms: i64,
    pub session_id: String,
    pub intent: String,
    #[serde(default)]
    pub intent_code: String,
    pub confidence: f64,
    #[serde(default)]
    pub intent_confidence: f32,
    pub active_devices: Vec<String>,
    pub state_deltas: Vec<StateDelta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_delta: Option<std::collections::HashMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    pub device_id: String,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub ts: i64,
    #[serde(default)]
    pub source_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub label: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub active_device_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub timestamp_ms: i64,
    pub session_id: String,
    pub topic: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_at_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_at_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hrv_at_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hr_at_time: Option<f64>,
}

// ── Daemon client ────────────────────────────────────────────────────────────

const DAEMON_BASE_URL: &str = "http://localhost:7890";

pub struct DaemonClient {
    client: Client,
    base_url: String,
}

impl DaemonClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: DAEMON_BASE_URL.to_string(),
        }
    }

    pub async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/v1/health", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    pub async fn start_session(&self, label: &str) -> Result<Session> {
        let resp = self
            .client
            .post(format!("{}/v1/session/start", self.base_url))
            .json(&serde_json::json!({ "label": label }))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to start session: {}", resp.status());
        }
        Ok(resp.json::<Session>().await?)
    }

    pub async fn stop_session(&self) -> Result<Option<Session>> {
        let resp = self
            .client
            .post(format!("{}/v1/session/stop", self.base_url))
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::CONFLICT {
            return Ok(None);
        }
        if !resp.status().is_success() {
            anyhow::bail!("Failed to stop session: {}", resp.status());
        }
        Ok(Some(resp.json::<Session>().await?))
    }

    pub async fn get_sessions(
        &self,
        since: Option<i64>,
        until: Option<i64>,
    ) -> Result<Vec<Session>> {
        let mut req = self.client.get(format!("{}/v1/sessions", self.base_url));
        if let Some(s) = since {
            req = req.query(&[("since", &s)]);
        }
        if let Some(u) = until {
            req = req.query(&[("until", &u)]);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to get sessions: {}", resp.status());
        }
        let data: serde_json::Value = resp.json().await?;
        Ok(data["sessions"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
            .unwrap_or_default())
    }

    pub async fn get_session_replay(&self, id: &str) -> Result<Vec<StateDelta>> {
        let resp = self
            .client
            .get(format!("{}/v1/session/{}/replay", self.base_url, id))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to get replay: {}", resp.status());
        }
        let data: serde_json::Value = resp.json().await?;
        Ok(data["events"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
            .unwrap_or_default())
    }

    pub async fn get_session_export_csv(&self, id: &str) -> Result<String> {
        let resp = self
            .client
            .get(format!(
                "{}/v1/session/{}/export?format=csv",
                self.base_url, id
            ))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to export session: {}", resp.status());
        }
        Ok(resp.text().await?)
    }

    pub async fn query_memory(&self, topic: Option<&str>) -> Result<Vec<MemoryRecord>> {
        let mut req = self.client.get(format!("{}/v1/memory", self.base_url));
        if let Some(t) = topic {
            req = req.query(&[("topic", t)]);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to query memory: {}", resp.status());
        }
        let data: serde_json::Value = resp.json().await?;
        Ok(data["records"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
            .unwrap_or_default())
    }
}

// ── Token handoff ────────────────────────────────────────────────────────────

pub fn read_auth_token() -> Option<String> {
    let token_path = home_dir()?
        .join(".local")
        .join("share")
        .join("veyn")
        .join("token");
    std::fs::read_to_string(token_path).ok()
}

// ── Tauri app state ──────────────────────────────────────────────────────────

pub struct InteroState {
    pub daemon_client: DaemonClient,
    pub is_daemon_running: AtomicBool,
    pub current_session: Arc<parking_lot::Mutex<Option<Session>>>,
    pub last_digest_sent: Arc<parking_lot::Mutex<Option<DateTime<Utc>>>>,
}

impl InteroState {
    pub fn new() -> Self {
        Self {
            daemon_client: DaemonClient::new(),
            is_daemon_running: AtomicBool::new(false),
            current_session: Arc::new(parking_lot::Mutex::new(None)),
            last_digest_sent: Arc::new(parking_lot::Mutex::new(None)),
        }
    }
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
async fn check_daemon_health(state: tauri::State<'_, InteroState>) -> bool {
    let healthy = state.daemon_client.health_check().await;
    state.is_daemon_running.store(healthy, Ordering::SeqCst);
    healthy
}

#[tauri::command]
async fn start_daemon_if_needed(state: tauri::State<'_, InteroState>) -> Result<bool, String> {
    if state.is_daemon_running.load(Ordering::SeqCst) {
        return Ok(true);
    }

    // Try to start veyn-core daemon via CLI if available
    let daemon_cmd = std::process::Command::new("veyn-core")
        .arg("--daemon")
        .spawn();

    match daemon_cmd {
        Ok(_) => {
            // Give it a moment to start
            tokio::time::sleep(StdDuration::from_secs(2)).await;
            let healthy = state.daemon_client.health_check().await;
            state.is_daemon_running.store(healthy, Ordering::SeqCst);
            Ok(healthy)
        }
        Err(e) => {
            warn!("Could not auto-start daemon: {}", e);
            Err(format!("Daemon not running and could not be started: {}", e))
        }
    }
}

#[tauri::command]
async fn get_auth_token() -> Option<String> {
    read_auth_token()
}

#[tauri::command]
async fn start_session(
    label: String,
    state: tauri::State<'_, InteroState>,
) -> Result<Session, String> {
    state
        .daemon_client
        .start_session(&label)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_session(state: tauri::State<'_, InteroState>) -> Result<Option<Session>, String> {
    state
        .daemon_client
        .stop_session()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_sessions(
    since: Option<i64>,
    until: Option<i64>,
    state: tauri::State<'_, InteroState>,
) -> Result<Vec<Session>, String> {
    state
        .daemon_client
        .get_sessions(since, until)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_session_replay(
    session_id: String,
    state: tauri::State<'_, InteroState>,
) -> Result<Vec<StateDelta>, String> {
    state
        .daemon_client
        .get_session_replay(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_session_csv(
    session_id: String,
    state: tauri::State<'_, InteroState>,
) -> Result<String, String> {
    state
        .daemon_client
        .get_session_export_csv(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn query_memory(
    topic: Option<String>,
    state: tauri::State<'_, InteroState>,
) -> Result<Vec<MemoryRecord>, String> {
    state
        .daemon_client
        .query_memory(topic.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_daily_digest(app: AppHandle, state: tauri::State<'_, InteroState>) -> Result<(), String> {
    // Check if we already sent today
    {
        let last_sent = state.last_digest_sent.lock();
        if let Some(last) = *last_sent {
            let now = Utc::now();
            if now.signed_duration_since(last) < Duration::days(1) {
                return Ok(());
            }
        }
    }

    // Fetch yesterday's dominant intent and HRV trend
    let now = Utc::now();
    let yesterday = now - Duration::days(1);
    
    // Simplified digest - in production would aggregate from memory records
    let title = "VEYN Daily Digest";
    let body = "Yesterday: Predominantly neutral state. HRV stable vs baseline.";

    Notification::new()
        .summary(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())?;

    *state.last_digest_sent.lock() = Some(now);
    Ok(())
}

#[tauri::command]
async fn send_fatigue_nudge(app: AppHandle) -> Result<(), String> {
    Notification::new()
        .summary("Fatigue Detected")
        .body("Your body shows signs of fatigue. Consider taking a break.")
        .show()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Main entry point ─────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(InteroState::new())
        .invoke_handler(tauri::generate_handler![
            check_daemon_health,
            start_daemon_if_needed,
            get_auth_token,
            start_session,
            stop_session,
            list_sessions,
            get_session_replay,
            export_session_csv,
            query_memory,
            send_daily_digest,
            send_fatigue_nudge,
        ])
        .setup(|app| {
            // Setup system tray with placeholder icon
            let icon_bytes = include_bytes!("../icons/icon.png");
            let icon = tauri::image::Image::from_bytes(icon_bytes).unwrap();
            
            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .tooltip("VEYN Intero")
                .build(app)?;

            // Auto-check daemon health on startup
            let handle = app.handle().clone();
            tokio::spawn(async move {
                let state = handle.state::<InteroState>();
                if !state.daemon_client.health_check().await {
                    info!("Daemon not running, attempting to start...");
                    let _ = start_daemon_if_needed(state).await;
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Intero");
}
