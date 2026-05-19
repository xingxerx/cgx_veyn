use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{Caller, Engine, Linker, Module, Store};

use veyn_schemas::VeynEvent;

use crate::manifest::PluginManifest;

/// Size of the guest-side poll output buffer allocated at init time.
const POLL_BUF: u32 = 64 * 1024;

/// Pending bytes for a device handle — filled by the proxy thread,
/// drained by `veyn::read_device` host import.
pub type DeviceQueue = Arc<Mutex<std::collections::VecDeque<Vec<u8>>>>;

/// State passed into every wasmtime host function.
struct HostState {
    http_client: Arc<reqwest::blocking::Client>,
    /// Per-handle byte queues populated by device-proxy threads.
    #[allow(dead_code)]
    device_queues: Arc<Mutex<HashMap<String, DeviceQueue>>>,
}

/// A loaded, initialised WASM plugin ready to be polled.
pub struct PluginRuntime {
    name: String,
    store: Store<HostState>,
    poll: wasmtime::TypedFunc<(u32, u32), u32>,
    poll_buf_ptr: u32,
    memory: wasmtime::Memory,
    /// Queues exposed to the plugin via the read_device host import.
    pub device_queues: Arc<Mutex<HashMap<String, DeviceQueue>>>,
}

// Store<HostState> is Send because HostState: Send.
unsafe impl Send for PluginRuntime {}

impl PluginRuntime {
    /// Load and initialise a plugin from its manifest.
    pub fn load(manifest: PluginManifest) -> Result<Self> {
        let device_queues: Arc<Mutex<HashMap<String, DeviceQueue>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Pre-create queues for each declared device descriptor.
        {
            let mut map = device_queues
                .lock()
                .map_err(|e| anyhow::anyhow!("mutex poisoned: {}", e))?;
            for desc in &manifest.devices {
                map.insert(
                    desc.id.clone(),
                    Arc::new(Mutex::new(std::collections::VecDeque::new())),
                );
            }
        }

        let engine = Engine::default();
        let mut linker: Linker<HostState> = Linker::new(&engine);

        // ── host import: veyn::log(level, ptr, len) ──────────────────────────
        linker.func_wrap(
            "veyn",
            "log",
            |mut caller: Caller<'_, HostState>, level: u32, ptr: u32, len: u32| {
                let mem = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .expect("memory export");
                let data = mem.data(&caller);
                if let Some(slice) = data.get(ptr as usize..(ptr + len) as usize) {
                    if let Ok(s) = std::str::from_utf8(slice) {
                        let s = s.to_owned();
                        match level {
                            0 => tracing::debug!(source = "plugin", "{}", s),
                            1 => tracing::info!(source = "plugin", "{}", s),
                            2 => tracing::warn!(source = "plugin", "{}", s),
                            _ => tracing::error!(source = "plugin", "{}", s),
                        }
                    }
                }
            },
        )?;

        // ── host import: veyn::time_ms() -> u64 ──────────────────────────────
        linker.func_wrap("veyn", "time_ms", |_caller: Caller<'_, HostState>| -> u64 {
            chrono::Utc::now().timestamp_millis() as u64
        })?;

        // ── host import: veyn::http_get(url_ptr, url_len, tok_ptr, tok_len,
        //                               out_ptr, out_cap) -> i32
        linker.func_wrap(
            "veyn",
            "http_get",
            |mut caller: Caller<'_, HostState>,
             url_ptr: u32,
             url_len: u32,
             tok_ptr: u32,
             tok_len: u32,
             out_ptr: u32,
             out_cap: u32|
             -> i32 {
                let mem = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .expect("memory export");

                let (url, token) = {
                    let data = mem.data(&caller);
                    let url = match data
                        .get(url_ptr as usize..(url_ptr + url_len) as usize)
                        .and_then(|s| std::str::from_utf8(s).ok())
                    {
                        Some(s) => s.to_owned(),
                        None => return -1,
                    };
                    let token = if tok_len > 0 {
                        data.get(tok_ptr as usize..(tok_ptr + tok_len) as usize)
                            .and_then(|s| std::str::from_utf8(s).ok())
                            .map(|s| s.to_owned())
                    } else {
                        None
                    };
                    (url, token)
                };

                let client = Arc::clone(&caller.data().http_client);
                let mut req = client.get(&url);
                if let Some(ref t) = token {
                    req = req.bearer_auth(t);
                }

                let body = match req.send().and_then(|r| r.bytes()) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!("plugin http_get {}: {}", url, e);
                        return -1;
                    }
                };

                let write_len = body.len().min(out_cap as usize);
                let data = mem.data_mut(&mut caller);
                data[out_ptr as usize..out_ptr as usize + write_len]
                    .copy_from_slice(&body[..write_len]);
                write_len as i32
            },
        )?;

        // ── host import: veyn::read_device(id_ptr, id_len, out_ptr, out_cap) -> i32
        // Non-blocking: drains one pending chunk from the device queue into guest
        // memory. Returns bytes written, 0 if queue is empty, -1 on error.
        {
            let queues = Arc::clone(&device_queues);
            linker.func_wrap(
                "veyn",
                "read_device",
                move |mut caller: Caller<'_, HostState>,
                      id_ptr: u32,
                      id_len: u32,
                      out_ptr: u32,
                      out_cap: u32|
                      -> i32 {
                    let mem = caller
                        .get_export("memory")
                        .and_then(|e| e.into_memory())
                        .expect("memory export");

                    let device_id = {
                        let data = mem.data(&caller);
                        match data
                            .get(id_ptr as usize..(id_ptr + id_len) as usize)
                            .and_then(|s| std::str::from_utf8(s).ok())
                        {
                            Some(s) => s.to_owned(),
                            None => return -1,
                        }
                    };

                    let chunk = {
                        let map = match queues.lock() {
                            Ok(m) => m,
                            Err(_) => return -1, // mutex poisoned
                        };
                        if let Some(queue) = map.get(&device_id) {
                            match queue.lock() {
                                Ok(mut q) => q.pop_front(),
                                Err(_) => return -1, // mutex poisoned
                            }
                        } else {
                            return -1; // unknown handle
                        }
                    };

                    match chunk {
                        None => 0, // empty queue
                        Some(bytes) => {
                            let write_len = bytes.len().min(out_cap as usize);
                            let data = mem.data_mut(&mut caller);
                            data[out_ptr as usize..out_ptr as usize + write_len]
                                .copy_from_slice(&bytes[..write_len]);
                            write_len as i32
                        }
                    }
                },
            )?;
        }

        // ── load and instantiate the WASM module ──────────────────────────────
        let wasm_bytes = std::fs::read(&manifest.wasm_path)
            .with_context(|| format!("reading {:?}", manifest.wasm_path))?;

        let module = Module::new(&engine, &wasm_bytes)?;

        let host_state = HostState {
            http_client: Arc::new(reqwest::blocking::Client::new()),
            device_queues: Arc::clone(&device_queues),
        };
        let mut store = Store::new(&engine, host_state);
        let instance = linker.instantiate(&mut store, &module)?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow!("plugin '{}' has no 'memory' export", manifest.plugin.name))?;

        let alloc = instance.get_typed_func::<u32, u32>(&mut store, "veyn_alloc")?;
        let poll_buf_ptr = alloc.call(&mut store, POLL_BUF)?;

        let config_json =
            serde_json::to_string(&manifest.config).unwrap_or_else(|_| "{}".to_string());
        let config_bytes = config_json.as_bytes();
        let config_ptr = alloc.call(&mut store, config_bytes.len() as u32)?;
        memory.write(&mut store, config_ptr as usize, config_bytes)?;

        let init = instance.get_typed_func::<(u32, u32), i32>(&mut store, "veyn_init")?;
        let rc = init.call(&mut store, (config_ptr, config_bytes.len() as u32))?;

        let free = instance.get_typed_func::<(u32, u32), ()>(&mut store, "veyn_free")?;
        free.call(&mut store, (config_ptr, config_bytes.len() as u32))?;

        if rc != 0 {
            return Err(anyhow!(
                "plugin '{}' veyn_init returned {}",
                manifest.plugin.name,
                rc
            ));
        }

        let poll = instance.get_typed_func::<(u32, u32), u32>(&mut store, "veyn_poll")?;

        tracing::info!(
            plugin  = %manifest.plugin.name,
            version = %manifest.plugin.version,
            devices = manifest.devices.len(),
            "plugin loaded"
        );

        Ok(Self {
            name: manifest.plugin.name,
            store,
            poll,
            poll_buf_ptr,
            memory,
            device_queues,
        })
    }

    /// Drive the plugin's poll function and decode any emitted events.
    pub fn poll_events(&mut self) -> Result<Vec<VeynEvent>> {
        let bytes_written = self
            .poll
            .call(&mut self.store, (self.poll_buf_ptr, POLL_BUF))?;

        if bytes_written == 0 {
            return Ok(vec![]);
        }

        let data = self.memory.data(&self.store);
        let slice =
            &data[self.poll_buf_ptr as usize..self.poll_buf_ptr as usize + bytes_written as usize];

        let events = slice
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                serde_json::from_slice::<VeynEvent>(line)
                    .map_err(|e| tracing::warn!(plugin = %self.name, "event decode error: {}", e))
                    .ok()
            })
            .collect();

        Ok(events)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
