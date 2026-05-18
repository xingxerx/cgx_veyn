use std::fs;
use std::path::{Path, PathBuf};

#[allow(unused_imports)]
use anyhow::{bail, Context, Result};
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use tracing::{info, warn};

use crate::api::state::AppState;

// ── Token path ────────────────────────────────────────────────────────────────

pub fn token_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir())
                .join(".local/share")
        })
        .join("veyn")
}

pub fn token_path() -> PathBuf {
    token_dir().join("token")
}

// ── Token load/create ─────────────────────────────────────────────────────────

/// Load an existing token or generate a new one.
/// `custom_path` overrides the default XDG location (set via `veyn.toml`).
pub fn load_or_create_token(custom_path: Option<&str>) -> Result<String> {
    let path = custom_path.map(PathBuf::from).unwrap_or_else(token_path);
    let dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(token_dir);

    if dir.exists() {
        verify_dir_ownership(&dir)?;
    } else {
        fs::create_dir_all(&dir)
            .with_context(|| format!("create token directory {}", dir.display()))?;
    }

    if path.exists() {
        verify_file_ownership(&path)?;
        let token = fs::read_to_string(&path)
            .with_context(|| format!("read token from {}", path.display()))?
            .trim()
            .to_owned();
        info!(path = %path.display(), "loaded existing API token");
        Ok(token)
    } else {
        let token = generate_token()?;
        write_token(&path, &token)?;
        info!(path = %path.display(), "generated new API token");
        append_audit_log(None, &format!("token generated at {}", path.display()));
        Ok(token)
    }
}

fn generate_token() -> Result<String> {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn write_token(path: &Path, token: &str) -> Result<()> {
    fs::write(path, token).with_context(|| format!("write token to {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod 0600 {}", path.display()))?;
    }
    Ok(())
}

// ── Ownership / permission verification ───────────────────────────────────────

#[cfg(unix)]
fn current_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:\t") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

#[cfg(unix)]
fn verify_dir_ownership(dir: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::metadata(dir).with_context(|| format!("stat {}", dir.display()))?;
    let Some(expected) = current_uid() else {
        warn!("cannot determine process uid; skipping directory ownership check");
        return Ok(());
    };
    if meta.uid() != expected {
        bail!(
            "token directory {} is owned by uid {} but process is uid {}",
            dir.display(),
            meta.uid(),
            expected
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_dir_ownership(_dir: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn verify_file_ownership(path: &Path) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let meta = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let Some(expected) = current_uid() else {
        warn!("cannot determine process uid; skipping file ownership check");
        return Ok(());
    };
    if meta.uid() != expected {
        bail!(
            "token file {} is owned by uid {} but process is uid {}",
            path.display(),
            meta.uid(),
            expected
        );
    }
    if meta.permissions().mode() & 0o177 != 0 {
        bail!(
            "token file {} has unsafe permissions {:o}; expected 0600",
            path.display(),
            meta.permissions().mode() & 0o777
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_file_ownership(_path: &Path) -> Result<()> {
    Ok(())
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Enforce `Authorization: Bearer <token>` on every request.
/// Bypassed when `require_auth = false` in config (dev/--no-auth mode).
/// `/health` and `/v1/health` are always public for operator liveness checks.
/// For WebSocket upgrades the token may be supplied as `?token=<value>`.
pub async fn require_bearer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if !state.config.require_auth {
        return next.run(req).await;
    }

    let path = req.uri().path();
    if path == "/health" || path == "/v1/health" {
        return next.run(req).await;
    }

    let expected = state.auth_token.as_str();
    let provided = extract_token(&req);

    if provided.as_deref() == Some(expected) {
        return next.run(req).await;
    }

    let path = path.to_owned();
    let reason = if provided.is_none() {
        "missing"
    } else {
        "invalid"
    };
    warn!(path = %path, reason, "auth failure");
    append_audit_log(
        state.config.audit_log_path.as_deref(),
        &format!("auth_failure reason={reason} path={path}"),
    );

    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized" })),
    )
        .into_response()
}

fn extract_token(req: &Request) -> Option<String> {
    if let Some(auth) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return Some(t.to_string());
            }
        }
    }
    req.uri()
        .query()
        .unwrap_or("")
        .split('&')
        .find(|p| p.starts_with("token="))
        .and_then(|p| p.strip_prefix("token="))
        .map(str::to_string)
}

// ── Audit log ─────────────────────────────────────────────────────────────────

pub fn append_audit_log(path: Option<&str>, entry: &str) {
    let path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| token_dir().join("audit.log"));
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let ts = chrono::Utc::now().to_rfc3339();
    let line = format!("{ts} {entry}\n");
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}
