mod api;
mod auth;
mod compression;
mod config;
mod dispatcher;
mod presence;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use veyn_adapters::{
    ble::BleAdapter, eeg::EegAdapter, healthkit::HealthKitAdapter, mock::MockAdapter, VeynAdapter,
};
use veyn_schemas::VeynEvent;

use api::state::{AppState, PluginInfo};

#[derive(Parser, Debug)]
#[command(
    name    = "veyn-core",
    version = env!("CARGO_PKG_VERSION"),
    about   = "VEYN daemon — sensory nervous system for software"
)]
struct Cli {
    /// Path to veyn.toml configuration file.
    #[arg(short, long, value_name = "PATH")]
    config: Option<String>,

    /// Override the API port (also overrides VEYN_PORT env and config file).
    #[arg(short, long, value_name = "PORT")]
    port: Option<u16>,

    /// Disable token authentication (development only — do not use in production).
    #[arg(long, default_value_t = false)]
    no_auth: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let cfg = config::load(cli.config.as_deref(), cli.port, cli.no_auth)?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cfg.log_level.as_str().into()),
        )
        .init();

    info!(
        api_port              = cfg.api_port,
        healthkit_port        = cfg.healthkit_port,
        mock_mode             = cfg.mock_mode,
        ble_enabled           = cfg.ble_enabled,
        eeg_enabled           = cfg.eeg_enabled,
        plugins_dir           = %cfg.plugins_dir,
        mqtt_enabled          = cfg.mqtt_url.is_some(),
        presence_timeout_secs = cfg.presence_timeout_secs,
        require_auth          = cfg.require_auth,
        rules_path            = %cfg.rules_path,
        "VEYN daemon starting"
    );

    // Load or generate the bearer token.
    let token = auth::load_or_create_token(cfg.token_path.as_deref())?;
    let scoped_tokens = auth::load_scoped_tokens();
    if cfg.require_auth {
        info!("Auth enabled — token path: {:?}", auth::token_path());
        if !scoped_tokens.is_empty() {
            info!(count = scoped_tokens.len(), "loaded scope-limited tokens");
        }
    } else {
        warn!("Auth DISABLED — do not use in production");
    }

    let (event_tx, event_rx) = mpsc::channel::<VeynEvent>(1_024);
    let state = AppState::new(token, scoped_tokens, cfg.clone());

    // Dispatcher.
    {
        let state = state.clone();
        let path = cfg.jsonl_path.clone();
        tokio::spawn(async move {
            dispatcher::run(event_rx, state, path).await;
        });
    }

    // Presence detection.
    {
        let state = state.clone();
        let tx = event_tx.clone();
        let timeout_ms = (cfg.presence_timeout_secs * 1_000) as i64;
        tokio::spawn(async move {
            presence::run(state, tx, timeout_ms).await;
        });
    }

    // Mock adapter.
    if cfg.mock_mode {
        spawn_adapter(MockAdapter, event_tx.clone());
    }

    // HealthKit TCP relay.
    spawn_adapter(
        HealthKitAdapter::new(cfg.healthkit_port, state.notification_tx.clone()),
        event_tx.clone(),
    );

    // BLE adapter.
    if cfg.ble_enabled {
        spawn_adapter(BleAdapter, event_tx.clone());
    }

    // EEG/OSC adapter.
    if cfg.eeg_enabled {
        spawn_adapter(EegAdapter::new(cfg.osc_port), event_tx.clone());
    }

    // Platform-specific adapters.
    #[cfg(target_os = "linux")]
    {
        if cfg.evdev_enabled {
            spawn_adapter(veyn_adapters::evdev_adapter::EvdevAdapter::new(), event_tx.clone());
        }
        if cfg.hidraw_enabled {
            spawn_adapter(veyn_adapters::hidraw::HidrawAdapter::new(), event_tx.clone());
        }
    }

    // MIDI adapter.
    if cfg.midi_enabled {
        spawn_adapter(veyn_adapters::midi::MidiAdapter::new(), event_tx.clone());
    }

    // Serial adapter.
    if let Some(ref serial_port) = cfg.serial_port {
        spawn_adapter(
            veyn_adapters::serial_adapter::SerialAdapter::new(
                serial_port.clone(),
                cfg.serial_baud,
            ),
            event_tx.clone(),
        );
    }

    // Filesystem watcher.
    if !cfg.fs_watch_paths.is_empty() {
        spawn_adapter(
            veyn_adapters::fs_watcher::FsWatcherAdapter::new(cfg.fs_watch_paths.clone()),
            event_tx.clone(),
        );
    }

    // WASM plugin adapters.
    let plugin_adapters = veyn_plugins::discover_adapters(&cfg.plugins_dir);
    if plugin_adapters.is_empty() {
        info!(plugins_dir = %cfg.plugins_dir, "no WASM plugins found");
    }
    for plugin in plugin_adapters {
        state.register_plugin(PluginInfo {
            name: plugin.manifest.plugin.name.clone(),
            version: plugin.manifest.plugin.version.clone(),
            description: plugin.manifest.plugin.description.clone(),
        });
        spawn_adapter(plugin, event_tx.clone());
    }

    // Smart home MQTT bridge.
    if let Some(mqtt_url) = cfg.mqtt_url.clone() {
        let rx = state.broadcast_tx.subscribe();
        tokio::spawn(async move {
            if let Err(e) = veyn_adapters::mqtt::run(rx, mqtt_url).await {
                error!("MQTT bridge error: {}", e);
            }
        });
    }

    // Graceful shutdown signal.
    let shutdown = async {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT — shutting down gracefully");
            }
            _ = sigterm() => {
                info!("received SIGTERM — shutting down gracefully");
            }
        }
    };

    // REST + WebSocket API — blocks until the server exits or a shutdown signal arrives.
    let port = cfg.api_port;
    if let Err(e) = api::serve(state, port, shutdown).await {
        error!("API server error: {}", e);
    }

    info!("VEYN daemon stopped");
    Ok(())
}

/// Spawn an adapter with automatic hot-plug retry using exponential backoff.
/// The adapter restarts on error; a clean `Ok(())` exit stops the loop.
fn spawn_adapter<A: VeynAdapter + 'static>(adapter: A, tx: mpsc::Sender<VeynEvent>) {
    let name = adapter.name().to_owned();
    tokio::spawn(async move {
        let mut delay = Duration::from_secs(1);
        loop {
            if tx.is_closed() {
                break;
            }
            match adapter.start(tx.clone()).await {
                Ok(()) => break, // graceful exit
                Err(e) => {
                    warn!(
                        adapter = %name,
                        delay_ms = delay.as_millis(),
                        "adapter error — retrying: {}",
                        e
                    );
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(60));
                }
            }
        }
    });
}

async fn sigterm() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut s) = signal(SignalKind::terminate()) {
            s.recv().await;
        } else {
            std::future::pending::<()>().await;
        }
    }
    #[cfg(not(unix))]
    std::future::pending::<()>().await
}
