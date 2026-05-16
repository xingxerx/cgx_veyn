mod api;
mod auth;
mod config;
mod dispatcher;
mod presence;

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{error, info};
use veyn_adapters::{
    ble::BleAdapter, eeg::EegAdapter, healthkit::HealthKitAdapter, mock::MockAdapter, VeynAdapter,
};
use veyn_schemas::VeynEvent;

use api::state::{AppState, PluginInfo};
use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = Config::default();

    let token = auth::load_or_create_token()?;
    info!(
        token_path = %auth::token_path().display(),
        "API token ready"
    );

    info!(
        api_port        = cfg.api_port,
        healthkit_port  = cfg.healthkit_port,
        mock_mode       = cfg.mock_mode,
        ble_enabled     = cfg.ble_enabled,
        eeg_enabled     = cfg.eeg_enabled,
        plugins_dir     = %cfg.plugins_dir,
        mqtt_enabled    = cfg.mqtt_url.is_some(),
        presence_timeout_secs = cfg.presence_timeout_secs,
        "VEYN daemon starting"
    );

    let (event_tx, event_rx) = mpsc::channel::<VeynEvent>(1_024);
    let state = AppState::new(token);

    // Dispatcher
    {
        let state = state.clone();
        let path = cfg.jsonl_path.clone();
        tokio::spawn(async move {
            dispatcher::run(event_rx, state, path).await;
        });
    }

    // Presence detection task
    {
        let state = state.clone();
        let tx = event_tx.clone();
        let timeout_ms = (cfg.presence_timeout_secs * 1_000) as i64;
        tokio::spawn(async move {
            presence::run(state, tx, timeout_ms).await;
        });
    }

    // Mock adapter (VEYN_MOCK=true)
    if cfg.mock_mode {
        spawn_adapter(MockAdapter, event_tx.clone());
    }

    // HealthKit TCP relay — bidirectional: receives health+gesture events from
    // the companion and routes notification frames back to it.
    spawn_adapter(
        HealthKitAdapter::new(cfg.healthkit_port, state.notification_tx.clone()),
        event_tx.clone(),
    );

    // BLE adapter (VEYN_BLE=true)
    if cfg.ble_enabled {
        spawn_adapter(BleAdapter, event_tx.clone());
    }

    // EEG/OSC adapter (VEYN_EEG=true)
    if cfg.eeg_enabled {
        spawn_adapter(EegAdapter::new(cfg.osc_port), event_tx.clone());
    }

    // WASM plugin adapters (VEYN_PLUGINS_DIR)
    let plugin_adapters = veyn_plugins::discover_adapters(&cfg.plugins_dir);
    if plugin_adapters.is_empty() {
        info!(plugins_dir = %cfg.plugins_dir, "no WASM plugins found");
    }
    for plugin in plugin_adapters {
        state.register_plugin(PluginInfo {
            name:        plugin.manifest.plugin.name.clone(),
            version:     plugin.manifest.plugin.version.clone(),
            description: plugin.manifest.plugin.description.clone(),
        });
        spawn_adapter(plugin, event_tx.clone());
    }

    // Smart home MQTT bridge (VEYN_MQTT_URL)
    if let Some(mqtt_url) = cfg.mqtt_url {
        let rx = state.broadcast_tx.subscribe();
        tokio::spawn(async move {
            if let Err(e) = veyn_adapters::mqtt::run(rx, mqtt_url).await {
                error!("MQTT bridge error: {}", e);
            }
        });
    }

    // REST + WebSocket API — blocks until the server exits
    api::serve(state, cfg.api_port).await
}

fn spawn_adapter<A: VeynAdapter + 'static>(adapter: A, tx: mpsc::Sender<VeynEvent>) {
    let name = adapter.name().to_owned();
    tokio::spawn(async move {
        if let Err(e) = adapter.start(tx).await {
            error!(adapter = %name, "adapter error: {}", e);
        }
    });
}
