//! Guest-side SDK for VEYN WASM plugins.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use veyn_plugin_sdk::{VeynPlugin, VeynEvent, veyn_register_plugin};
//! use serde_json::Value;
//!
//! struct MyPlugin { /* ... */ }
//!
//! impl VeynPlugin for MyPlugin {
//!     fn init(config: Value) -> Result<Self, String> { Ok(MyPlugin {}) }
//!     fn poll(&mut self) -> Vec<VeynEvent> { vec![] }
//! }
//!
//! veyn_register_plugin!(MyPlugin);
//! ```
//!
//! Compile with:
//! ```sh
//! cargo build --target wasm32-unknown-unknown --release
//! ```

pub use serde_json;

// ── Re-exported event schema ──────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Mirrors `veyn_schemas::VeynEvent` for use inside the WASM guest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeynEvent {
    pub id: String,
    pub ts: i64,
    pub device_id: String,
    pub source: String,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub meta: HashMap<String, serde_json::Value>,
}

impl VeynEvent {
    pub fn new(
        device_id: impl Into<String>,
        source: impl Into<String>,
        metric: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            id: new_uuid(),
            ts: time_ms() as i64,
            device_id: device_id.into(),
            source: source.into(),
            metric: metric.into(),
            value,
            unit: unit.into(),
            meta: HashMap::new(),
        }
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.meta.insert(key.into(), value);
        self
    }
}

// ── Plugin trait ──────────────────────────────────────────────────────────────

/// Implement this trait for your plugin type.
pub trait VeynPlugin: Sized {
    /// Called once at startup with the plugin's `[config]` table as JSON.
    fn init(config: serde_json::Value) -> Result<Self, String>;
    /// Called by the runtime at the configured `poll_interval_secs`.
    fn poll(&mut self) -> Vec<VeynEvent>;
}

// ── Host-provided utilities ───────────────────────────────────────────────────

/// Current Unix time in milliseconds (provided by the host).
pub fn time_ms() -> u64 {
    unsafe { __veyn_time_ms() }
}

/// Today's date as `"YYYY-MM-DD"` (computed from host time).
pub fn today_date() -> String {
    epoch_ms_to_date(time_ms())
}

/// Log at info level.
pub fn log_info(msg: &str) {
    unsafe { __veyn_log(1, msg.as_ptr() as u32, msg.len() as u32) }
}

/// Log at warning level.
pub fn log_warn(msg: &str) {
    unsafe { __veyn_log(2, msg.as_ptr() as u32, msg.len() as u32) }
}

/// Log at error level.
pub fn log_error(msg: &str) {
    unsafe { __veyn_log(3, msg.as_ptr() as u32, msg.len() as u32) }
}

/// Make an HTTP GET request.  Pass `None` for `bearer_token` if the endpoint
/// is unauthenticated.  Returns the response body or an error string.
pub fn http_get(url: &str, bearer_token: Option<&str>) -> Result<Vec<u8>, String> {
    // Pre-allocate 256 KiB for the response.
    let mut out = vec![0u8; 256 * 1024];
    let (tok_ptr, tok_len) = bearer_token
        .map(|t| (t.as_ptr() as u32, t.len() as u32))
        .unwrap_or((0, 0));

    let n = unsafe {
        __veyn_http_get(
            url.as_ptr() as u32,
            url.len() as u32,
            tok_ptr,
            tok_len,
            out.as_mut_ptr() as u32,
            out.len() as u32,
        )
    };

    if n < 0 {
        Err(format!("http_get failed: {}", url))
    } else {
        out.truncate(n as usize);
        Ok(out)
    }
}

// ── Host imports ──────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "veyn")]
extern "C" {
    #[link_name = "log"]
    fn __veyn_log(level: u32, ptr: u32, len: u32);

    #[link_name = "time_ms"]
    fn __veyn_time_ms() -> u64;

    #[link_name = "http_get"]
    fn __veyn_http_get(
        url_ptr: u32,
        url_len: u32,
        tok_ptr: u32,
        tok_len: u32,
        out_ptr: u32,
        out_cap: u32,
    ) -> i32;
}

// Stubs for non-wasm builds (host tests / IDE checks).
#[cfg(not(target_arch = "wasm32"))]
unsafe fn __veyn_log(_level: u32, _ptr: u32, _len: u32) {}
#[cfg(not(target_arch = "wasm32"))]
unsafe fn __veyn_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[cfg(not(target_arch = "wasm32"))]
unsafe fn __veyn_http_get(_: u32, _: u32, _: u32, _: u32, _: u32, _: u32) -> i32 {
    -1
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn new_uuid() -> String {
    // Simple UUID v4 using random bytes sourced from time (good enough for WASM).
    let t = time_ms();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (t >> 16) & 0xffff_ffff,
        (t >> 8) & 0xffff,
        t & 0x0fff,
        0x8000 | (t & 0x3fff),
        t.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
            & 0xffff_ffff_ffff,
    )
}

/// Pure arithmetic conversion from Unix epoch (ms) to `"YYYY-MM-DD"`.
/// Uses the algorithm from http://howardhinnant.github.io/date_algorithms.html
fn epoch_ms_to_date(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

// ── Plugin registration macro ─────────────────────────────────────────────────

/// Register your plugin type as the WASM module's entry point.
///
/// ```rust,ignore
/// veyn_register_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! veyn_register_plugin {
    ($ty:ty) => {
        mod __veyn_entry {
            use super::*;
            use std::alloc::{alloc, dealloc, Layout};

            static mut PLUGIN: Option<$ty> = None;

            #[no_mangle]
            pub unsafe extern "C" fn veyn_alloc(size: u32) -> u32 {
                let layout = Layout::from_size_align_unchecked(size as usize, 8);
                alloc(layout) as u32
            }

            #[no_mangle]
            pub unsafe extern "C" fn veyn_free(ptr: u32, size: u32) {
                let layout = Layout::from_size_align_unchecked(size as usize, 8);
                dealloc(ptr as *mut u8, layout);
            }

            #[no_mangle]
            pub unsafe extern "C" fn veyn_init(config_ptr: u32, config_len: u32) -> i32 {
                let bytes =
                    std::slice::from_raw_parts(config_ptr as *const u8, config_len as usize);
                let config = $crate::serde_json::from_slice(bytes)
                    .unwrap_or($crate::serde_json::Value::Object(Default::default()));
                match <$ty as $crate::VeynPlugin>::init(config) {
                    Ok(p) => {
                        PLUGIN = Some(p);
                        0
                    }
                    Err(e) => {
                        $crate::log_error(&e);
                        -1
                    }
                }
            }

            #[no_mangle]
            pub unsafe extern "C" fn veyn_poll(buf_ptr: u32, buf_cap: u32) -> u32 {
                let plugin = match PLUGIN.as_mut() {
                    Some(p) => p,
                    None => return 0,
                };
                let events = <$ty as $crate::VeynPlugin>::poll(plugin);
                let buf = std::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_cap as usize);
                let mut written = 0usize;
                for ev in &events {
                    if let Ok(json) = $crate::serde_json::to_string(ev) {
                        let bytes = json.as_bytes();
                        if written + bytes.len() + 1 > buf_cap as usize {
                            break;
                        }
                        buf[written..written + bytes.len()].copy_from_slice(bytes);
                        written += bytes.len();
                        buf[written] = b'\n';
                        written += 1;
                    }
                }
                written as u32
            }
        }
    };
}
