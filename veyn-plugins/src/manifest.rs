use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

// ── Device descriptor ─────────────────────────────────────────────────────────

/// A hardware device that the plugin wants to read from.
/// The daemon opens the device on the plugin's behalf and calls
/// `veyn_on_device_data` when bytes arrive.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceDescriptor {
    /// "hidraw" | "serial" | "ble"
    #[serde(rename = "type")]
    pub kind: String,
    /// For hidraw: "/dev/hidrawN" or a glob; for serial: "/dev/ttyUSB*";
    /// for BLE: the service UUID string.
    #[serde(default)]
    pub path: Option<String>,
    /// Friendly identifier passed back to the plugin as the handle string.
    pub id: String,
}

// ── Signature ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginSignature {
    /// SHA-256 hex digest of the compiled `.wasm` binary.
    pub sha256: Option<String>,
}

// ── Manifest ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginMeta,
    #[serde(default)]
    pub config: serde_json::Value,
    /// Devices this plugin needs hardware access to.
    #[serde(default)]
    pub devices: Vec<DeviceDescriptor>,
    /// Optional signature block for integrity verification.
    #[serde(default)]
    pub signature: PluginSignature,
    /// Resolved at load time — not from TOML.
    #[serde(skip)]
    pub wasm_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Path to the .wasm file relative to the manifest (default: `<name>.wasm`).
    pub wasm: Option<String>,
}

// ── Discovery ─────────────────────────────────────────────────────────────────

/// Scan `plugins_dir` for `plugin.toml` files and parse each one.
pub fn discover_plugins(plugins_dir: &str) -> Vec<PluginManifest> {
    let dir = Path::new(plugins_dir);
    if !dir.exists() {
        return vec![];
    }

    let mut manifests = vec![];

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("cannot read plugins dir {:?}: {}", dir, e);
            return vec![];
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let manifest_path = if path.is_dir() {
            path.join("plugin.toml")
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            path.clone()
        } else {
            continue;
        };

        if !manifest_path.exists() {
            continue;
        }

        match load_manifest(&manifest_path) {
            Ok(m) => {
                tracing::info!(plugin = %m.plugin.name, path = ?manifest_path, "found plugin manifest");
                manifests.push(m);
            }
            Err(e) => tracing::warn!("bad plugin manifest {:?}: {}", manifest_path, e),
        }
    }

    manifests
}

pub fn load_manifest(path: &Path) -> Result<PluginManifest> {
    let content = std::fs::read_to_string(path).with_context(|| format!("reading {:?}", path))?;
    let mut manifest: PluginManifest =
        toml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;

    let dir = path.parent().unwrap_or(Path::new("."));
    let wasm_name = manifest
        .plugin
        .wasm
        .as_deref()
        .unwrap_or(&format!("{}.wasm", manifest.plugin.name))
        .to_owned();
    manifest.wasm_path = dir.join(wasm_name);

    Ok(manifest)
}

// ── Signature verification ────────────────────────────────────────────────────

/// Verify the SHA-256 of the wasm binary matches the manifest's declared hash.
/// Returns Ok(()) if no signature is declared (unsigned).
pub fn verify_signature(manifest: &PluginManifest, allow_unsigned: bool) -> Result<()> {
    use sha2::{Digest, Sha256};

    let expected = match &manifest.signature.sha256 {
        Some(h) => h.to_lowercase(),
        None => {
            if allow_unsigned {
                tracing::warn!(
                    plugin = %manifest.plugin.name,
                    "loading unsigned plugin (allow_unsigned=true)"
                );
                return Ok(());
            } else {
                anyhow::bail!(
                    "plugin '{}' has no SHA-256 signature and allow_unsigned=false",
                    manifest.plugin.name
                );
            }
        }
    };

    let bytes = std::fs::read(&manifest.wasm_path)
        .with_context(|| format!("reading {:?} for signature check", manifest.wasm_path))?;

    let digest = Sha256::digest(&bytes);
    let actual = hex::encode(digest);

    if actual != expected {
        anyhow::bail!(
            "plugin '{}' signature mismatch: expected {} got {}",
            manifest.plugin.name,
            expected,
            actual
        );
    }

    tracing::info!(plugin = %manifest.plugin.name, "plugin signature verified");
    Ok(())
}

/// Compute the SHA-256 hex digest of a file — used by `plugin install`.
pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).with_context(|| format!("reading {:?}", path))?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}
