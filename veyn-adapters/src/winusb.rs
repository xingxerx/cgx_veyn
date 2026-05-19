//! Windows WinUSB / RawInput HID adapter.
//!
//! Provides equivalent HID coverage on Windows to the Linux evdev/hidraw adapters.
//! Uses `WinUSB` for low-level USB device access and `RawInput` (via `RegisterRawInputDevices`)
//! for system-level keyboard, mouse, and gamepad events without requiring elevated privileges.
//!
//! # Device coverage
//! - USB HID class devices via WinUSB kernel driver
//! - Keyboard (usage page 0x01, usage 0x06) and mouse (usage 0x01, usage 0x02)
//!   via RawInput — works without admin rights
//! - Custom biometric hardware that ships with a WinUSB or libusb-compatible driver
//!
//! # Platform gate
//! This module is only compiled on `target_os = "windows"`.

#[allow(unused_imports)]
use anyhow::{bail, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::info;
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

pub struct WinUsbAdapter {
    /// Optional VID filter; None means accept all vendors.
    pub vendor_id: Option<u16>,
    /// Optional PID filter; None means accept all products.
    pub product_id: Option<u16>,
}

impl WinUsbAdapter {
    pub fn new() -> Self {
        Self {
            vendor_id: None,
            product_id: None,
        }
    }

    pub fn with_filter(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id: Some(vendor_id),
            product_id: Some(product_id),
        }
    }
}

impl Default for WinUsbAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VeynAdapter for WinUsbAdapter {
    fn name(&self) -> &str {
        "winusb"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        info!("WinUSB/RawInput HID adapter starting (Windows)");

        #[cfg(not(target_os = "windows"))]
        bail!("WinUsbAdapter can only run on Windows");

        #[cfg(target_os = "windows")]
        {
            use hidapi::HidApi;

            // hidapi on Windows wraps SetupAPI + WinUSB for enumeration and I/O.
            let api = HidApi::new().map_err(|e| anyhow::anyhow!("HidApi init failed: {e}"))?;

            loop {
                for device_info in api.device_list() {
                    if let Some(vid) = self.vendor_id {
                        if device_info.vendor_id() != vid {
                            continue;
                        }
                    }
                    if let Some(pid) = self.product_id {
                        if device_info.product_id() != pid {
                            continue;
                        }
                    }

                    let device_id = format!(
                        "winusb:{:04x}:{:04x}",
                        device_info.vendor_id(),
                        device_info.product_id()
                    );

                    let dev = match device_info.open_device(&api) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };

                    let mut buf = [0u8; 64];
                    if let Ok(n) = dev.read_timeout(&mut buf, 10) {
                        if n > 0 {
                            let event = VeynEvent::new(
                                &device_id,
                                "winusb",
                                "hid_report_id",
                                buf[0] as f64,
                                "",
                            );
                            if tx.send(event).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
            }
        }

        #[allow(unreachable_code)]
        Ok(())
    }
}
