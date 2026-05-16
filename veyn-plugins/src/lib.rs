pub mod manifest;
pub mod runtime;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{error, info};
use veyn_schemas::VeynEvent;

pub use manifest::{PluginManifest, PluginMeta};
use runtime::PluginRuntime;

/// Wraps a WASM plugin so it can be driven as a `VeynAdapter`.
pub struct PluginAdapter {
    pub manifest: PluginManifest,
}

impl PluginAdapter {
    pub fn new(manifest: PluginManifest) -> Self {
        Self { manifest }
    }
}

#[async_trait]
impl veyn_adapters::VeynAdapter for PluginAdapter {
    fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        let manifest = self.manifest.clone();
        let poll_interval_secs = manifest
            .config
            .get("poll_interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        let name = manifest.plugin.name.clone();

        // Load the WASM module on a blocking thread (wasmtime is synchronous).
        let mut runtime =
            tokio::task::spawn_blocking(move || PluginRuntime::load(manifest)).await??;

        info!(plugin = %name, interval_secs = poll_interval_secs, "plugin adapter started");

        loop {
            // Poll on a blocking thread, then move the runtime back.
            let (rt, result) = tokio::task::spawn_blocking(move || {
                let result = runtime.poll_events();
                (runtime, result)
            })
            .await?;
            runtime = rt;

            match result {
                Ok(events) => {
                    for event in events {
                        if tx.send(event).await.is_err() {
                            return Ok(());
                        }
                    }
                }
                Err(e) => error!(plugin = %name, "poll error: {}", e),
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval_secs)).await;
        }
    }
}

/// Scan `plugins_dir` for `plugin.toml` manifests, return an adapter for each
/// plugin whose `.wasm` file is present on disk.
pub fn discover_adapters(plugins_dir: &str) -> Vec<PluginAdapter> {
    manifest::discover_plugins(plugins_dir)
        .into_iter()
        .filter(|m| {
            if m.wasm_path.exists() {
                true
            } else {
                tracing::warn!(
                    plugin = %m.plugin.name,
                    path   = ?m.wasm_path,
                    "wasm file not found — skipping plugin"
                );
                false
            }
        })
        .map(PluginAdapter::new)
        .collect()
}
