use std::collections::HashMap;

use chrono::Utc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration, MissedTickBehavior};
use tracing::info;
use veyn_schemas::{PresenceInfo, PresenceState, VeynDevice, VeynEvent};

use crate::api::state::AppState;

/// Poll the device registry every 5 seconds. When a device's `last_seen`
/// timestamp is older than `timeout_millis`, emit a presence-absent event and
/// update the shared presence map. Emit a presence-present event when the
/// device becomes active again.
pub async fn run(state: AppState, event_tx: mpsc::Sender<VeynEvent>, timeout_millis: i64) {
    let mut ticker = interval(Duration::from_secs(5));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // Local cache of presence states so we only emit on transitions.
    let mut local: HashMap<String, PresenceState> = HashMap::new();

    info!(
        timeout_ms = timeout_millis,
        "presence detection task started"
    );

    loop {
        ticker.tick().await;

        let now = Utc::now().timestamp_millis();
        let devices: Vec<VeynDevice> = state.devices.lock().unwrap().values().cloned().collect();

        for device in &devices {
            let new_state = if now - device.last_seen < timeout_millis {
                PresenceState::Present
            } else {
                PresenceState::Absent
            };

            let prev = local.get(&device.id);
            if prev == Some(&new_state) {
                // No transition — just refresh last_seen in the shared map.
                if let Some(info) = state.presence.lock().unwrap().get_mut(&device.id) {
                    info.last_seen = device.last_seen;
                }
                continue;
            }

            info!(
                device_id = %device.id,
                state = ?new_state,
                "presence state changed"
            );

            // Emit a VeynEvent so the transition appears in the stream.
            let value = if new_state == PresenceState::Present {
                1.0
            } else {
                0.0
            };
            let event = VeynEvent::new(&device.id, "presence", "presence", value, "").with_meta(
                "presence_state",
                serde_json::to_value(&new_state).unwrap_or_default(),
            );
            let _ = event_tx.send(event).await;

            state.presence.lock().unwrap().insert(
                device.id.clone(),
                PresenceInfo {
                    device_id: device.id.clone(),
                    state: new_state.clone(),
                    last_seen: device.last_seen,
                    since_ts: now,
                },
            );

            local.insert(device.id.clone(), new_state);
        }
    }
}
