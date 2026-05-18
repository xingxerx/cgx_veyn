//! Filesystem watcher adapter — emits events when files under watched paths
//! are created, modified, or deleted.
//!
//! Emits: source="fs", metric="fs_event", value=1.0 (create/modify) or 0.0 (remove)
//! with `meta.path`, `meta.event_kind`.

use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;

use anyhow::Result;
use async_trait::async_trait;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{info, warn};
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

pub struct FsWatcherAdapter {
    paths: Vec<String>,
}

impl FsWatcherAdapter {
    pub fn new(paths: Vec<String>) -> Self {
        Self { paths }
    }
}

#[async_trait]
impl VeynAdapter for FsWatcherAdapter {
    fn name(&self) -> &str {
        "fs"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        if self.paths.is_empty() {
            warn!("fs-watcher: no paths configured; adapter idle");
            std::future::pending::<()>().await;
            return Ok(());
        }

        let paths = self.paths.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_watcher(paths, tx) {
                warn!("fs-watcher error: {}", e);
            }
        })
        .await?;
        Ok(())
    }
}

fn run_watcher(paths: Vec<String>, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
    let (fs_tx, fs_rx) = std_mpsc::channel();

    let mut watcher = RecommendedWatcher::new(fs_tx, Config::default())?;

    for path_str in &paths {
        let path = PathBuf::from(path_str);
        match watcher.watch(&path, RecursiveMode::Recursive) {
            Ok(()) => info!(path = %path_str, "fs-watcher: watching"),
            Err(e) => warn!(path = %path_str, "fs-watcher: cannot watch: {}", e),
        }
    }

    for result in fs_rx {
        if tx.is_closed() {
            break;
        }
        let fs_event = match result {
            Ok(e) => e,
            Err(e) => {
                warn!("fs-watcher notify error: {}", e);
                continue;
            }
        };

        let (metric_value, kind_str) = classify_event(&fs_event.kind);

        for path in &fs_event.paths {
            let path_str = path.to_string_lossy().to_string();
            let event = VeynEvent::new("fs:watcher", "fs", "fs_event", metric_value, "")
                .with_meta("path", serde_json::Value::String(path_str))
                .with_meta(
                    "event_kind",
                    serde_json::Value::String(kind_str.to_string()),
                );

            if tx.blocking_send(event).is_err() {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn classify_event(kind: &notify::EventKind) -> (f64, &'static str) {
    use notify::EventKind;
    match kind {
        EventKind::Create(_) => (1.0, "create"),
        EventKind::Modify(_) => (1.0, "modify"),
        EventKind::Remove(_) => (0.0, "remove"),
        EventKind::Access(_) => (1.0, "access"),
        _ => (1.0, "other"),
    }
}
