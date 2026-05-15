mod api;
mod config;
mod dispatcher;

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

    info!(
        api_port        = cfg.api_port,
        healthkit_port  = cfg.healthkit_port,
        mock_mode       = cfg.mock_mode,
        ble_enabled     = cfg.ble_enabled,
        eeg_enabled     = cfg.eeg_enabled,
        plugins_dir     = %cfg.plugins_dir,
        "VEYN daemon starting"
    );

    let (event_tx, event_rx) = mpsc::channel::<VeynEvent>(1_024);
    let state = AppState::new();

    // Dispatcher
    {
        let state = state.clone();
        let path = cfg.jsonl_path.clone();
        tokio::spawn(async move {
            dispatcher::run(event_rx, state, path).await;
        });
    }

    // Mock adapter (VEYN_MOCK=true)
    if cfg.mock_mode {
        spawn_adapter(MockAdapter, event_tx.clone());
    }

    // HealthKit TCP relay (always running; harmless when companion is absent)
    spawn_adapter(HealthKitAdapter::new(cfg.healthkit_port), event_tx.clone());

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
