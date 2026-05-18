pub mod manifest;
pub mod runtime;

use anyhow::Result;
use async_trait::async_trait;
use std::io::Read as _;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use veyn_schemas::VeynEvent;

pub use manifest::{
    load_manifest, sha256_file, verify_signature, PluginManifest, PluginMeta, PluginSignature,
};
use runtime::PluginRuntime;

/// Wraps a WASM plugin so it can be driven as a `VeynAdapter`.
pub struct PluginAdapter {
    pub manifest: PluginManifest,
    /// Whether unsigned plugins are permitted (from daemon config).
    pub allow_unsigned: bool,
}

impl PluginAdapter {
    pub fn new(manifest: PluginManifest) -> Self {
        Self {
            manifest,
            allow_unsigned: true,
        }
    }

    pub fn with_signature_policy(mut self, allow_unsigned: bool) -> Self {
        self.allow_unsigned = allow_unsigned;
        self
    }
}

#[async_trait]
impl veyn_adapters::VeynAdapter for PluginAdapter {
    fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        let manifest = self.manifest.clone();
        let allow_unsigned = self.allow_unsigned;
        let poll_interval_secs = manifest
            .config
            .get("poll_interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        let name = manifest.plugin.name.clone();

        // Load the WASM module on a blocking thread (wasmtime is synchronous).
        let mut runtime = tokio::task::spawn_blocking(move || {
            // Verify signature before instantiation.
            verify_signature(&manifest, allow_unsigned)?;
            PluginRuntime::load(manifest)
        })
        .await??;

        // Spawn device-proxy threads for each declared device descriptor.
        let device_queues = Arc::clone(&runtime.device_queues);
        for desc in &self.manifest.devices {
            spawn_device_proxy(desc, Arc::clone(&device_queues));
        }

        info!(plugin = %name, interval_secs = poll_interval_secs, "plugin adapter started");

        loop {
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

/// Spawn a blocking thread that reads from a device and pushes chunks into the
/// plugin's device queue.  Only `hidraw` and `serial` are supported on Linux;
/// BLE is a no-op placeholder (BLE devices are already served by the BLE adapter).
fn spawn_device_proxy(
    desc: &manifest::DeviceDescriptor,
    queues: Arc<Mutex<std::collections::HashMap<String, runtime::DeviceQueue>>>,
) {
    let kind = desc.kind.clone();
    let path = desc.path.clone();
    let id = desc.id.clone();

    std::thread::spawn(move || {
        device_proxy_loop(kind, path, id, queues);
    });
}

fn device_proxy_loop(
    kind: String,
    path: Option<String>,
    handle_id: String,
    queues: Arc<Mutex<std::collections::HashMap<String, runtime::DeviceQueue>>>,
) {
    let Some(device_path) = path else {
        warn!(kind = %kind, handle = %handle_id, "device descriptor has no path — proxy idle");
        return;
    };

    // Resolve glob — use first match.
    let resolved = if device_path.contains('*') {
        match glob::glob(&device_path) {
            Ok(mut paths) => match paths.next() {
                Some(Ok(p)) => p.to_string_lossy().to_string(),
                _ => {
                    warn!(pattern = %device_path, "no device matched glob — proxy idle");
                    return;
                }
            },
            Err(_) => device_path.clone(),
        }
    } else {
        device_path.clone()
    };

    info!(kind = %kind, path = %resolved, handle = %handle_id, "device proxy starting");

    let mut file = match std::fs::OpenOptions::new().read(true).open(&resolved) {
        Ok(f) => f,
        Err(e) => {
            warn!(path = %resolved, "device proxy cannot open device: {}", e);
            return;
        }
    };

    let mut buf = [0u8; 4096];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                warn!(path = %resolved, "device proxy read error: {}", e);
                break;
            }
        };
        let chunk = buf[..n].to_vec();
        let map = queues.lock().unwrap();
        if let Some(queue) = map.get(&handle_id) {
            let mut q = queue.lock().unwrap();
            q.push_back(chunk);
            // Bound queue to 256 chunks to prevent unbounded growth.
            while q.len() > 256 {
                q.pop_front();
            }
        }
    }
    info!(path = %resolved, handle = %handle_id, "device proxy stopped");
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
                warn!(
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
