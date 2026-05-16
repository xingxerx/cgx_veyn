use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

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

pub fn token_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/tmp"))
                .join(".local/share")
        })
        .join("veyn")
}

pub fn token_path() -> PathBuf {
    token_dir().join("token")
}

/// Load the token from disk, creating it (and the storage directory) if absent.
/// Verifies that the directory and file are owned by the current process uid
/// and that the file has mode 0600 before trusting its contents.
pub fn load_or_create_token() -> Result<String> {
    let dir = token_dir();
    let path = token_path();

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
        Ok(token)
    }
}

fn generate_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    let mut f = fs::File::open("/dev/urandom").context("open /dev/urandom")?;
    f.read_exact(&mut bytes).context("read /dev/urandom")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn write_token(path: &Path, token: &str) -> Result<()> {
    fs::write(path, token).with_context(|| format!("write token to {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 0600 {}", path.display()))?;
    Ok(())
}

fn current_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:\t") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

fn verify_dir_ownership(dir: &Path) -> Result<()> {
    let meta = fs::metadata(dir)
        .with_context(|| format!("stat {}", dir.display()))?;
    let Some(expected) = current_uid() else {
        warn!("cannot determine process uid; skipping directory ownership check");
        return Ok(());
    };
    if meta.uid() != expected {
        bail!(
            "token directory {} is owned by uid {} but process is uid {}",
            dir.display(), meta.uid(), expected
        );
    }
    Ok(())
}

fn verify_file_ownership(path: &Path) -> Result<()> {
    let meta = fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?;
    let Some(expected) = current_uid() else {
        warn!("cannot determine process uid; skipping file ownership check");
        return Ok(());
    };
    if meta.uid() != expected {
        bail!(
            "token file {} is owned by uid {} but process is uid {}",
            path.display(), meta.uid(), expected
        );
    }
    // Reject any bits beyond owner read/write (i.e. reject group/other access)
    if meta.permissions().mode() & 0o177 != 0 {
        bail!(
            "token file {} has unsafe permissions {:o}; expected 0600",
            path.display(), meta.permissions().mode() & 0o777
        );
    }
    Ok(())
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Tower middleware that enforces `Authorization: Bearer <token>` on every
/// request except GET /health.  For WebSocket upgrades (where browsers cannot
/// set custom headers) the token may also be supplied as `?token=<value>` in
/// the query string.  All failures are logged for audit purposes.
pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    if req.uri().path() == "/health" {
        return next.run(req).await;
    }

    let expected = state.token.as_ref();

    let from_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned);

    let from_query = req.uri().query().and_then(|q| {
        q.split('&')
            .find(|p| p.starts_with("token="))
            .and_then(|p| p.strip_prefix("token="))
            .map(str::to_owned)
    });

    let provided = from_header.or(from_query);

    if provided.as_deref() == Some(expected) {
        return next.run(req).await;
    }

    let path = req.uri().path().to_owned();
    let reason = if provided.is_none() { "missing" } else { "invalid" };
    warn!(path = %path, reason = reason, "auth failure");

    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized" })),
    )
        .into_response()
}

/// Tower middleware that rejects requests whose `Host` header is not
/// `localhost` or `127.0.0.1`, preventing DNS-rebinding attacks.
pub async fn host_guard(req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let host_part = host.split(':').next().unwrap_or(host);

    if host_part.is_empty() || host_part == "localhost" || host_part == "127.0.0.1" {
        return next.run(req).await;
    }

    warn!(host = %host, "rejected: unexpected Host header");
    (StatusCode::BAD_REQUEST, "invalid host").into_response()
}
