use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginMeta,
    #[serde(default)]
    pub config: serde_json::Value,
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

fn load_manifest(path: &Path) -> Result<PluginManifest> {
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
