use std::path::PathBuf;

use anyhow::Result;
use axum::{
    extract::Request,
    extract::State,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use rand::Rng;
use tracing::{info, warn};

use crate::api::state::AppState;

pub fn token_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("veyn")
        .join("token")
}

pub fn load_or_create_token(custom_path: Option<&str>) -> Result<String> {
    let path = custom_path
        .map(PathBuf::from)
        .unwrap_or_else(token_path);

    if path.exists() {
        let token = std::fs::read_to_string(&path)?.trim().to_string();
        info!(path = ?path, "loaded auth token");
        return Ok(token);
    }

    // Generate a cryptographically random 256-bit token.
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    let token: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &token)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    info!(path = ?path, "generated new auth token (chmod 600)");
    append_audit_log(None, &format!("token generated at {:?}", path));
    Ok(token)
}

// ── Middleware ────────────────────────────────────────────────────────────────

pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Auth disabled in config → pass through.
    if !state.config.require_auth {
        return Ok(next.run(req).await);
    }

    let token = extract_token(&req);
    let path = req.uri().path().to_owned();

    match token {
        Some(ref t) if t == state.auth_token.as_str() => Ok(next.run(req).await),
        _ => {
            warn!(path = %path, "rejected unauthenticated request");
            append_audit_log(
                state.config.audit_log_path.as_deref(),
                &format!("auth_failure path={}", path),
            );
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Extract the bearer token from the Authorization header or `?token=` query param.
fn extract_token(req: &Request) -> Option<String> {
    if let Some(auth) = req.headers().get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return Some(t.to_string());
            }
        }
    }
    // Fallback: query param (useful for WebSocket connections from browser clients
    // that can't set custom headers).
    let query = req.uri().query().unwrap_or("");
    for kv in query.split('&') {
        let mut parts = kv.splitn(2, '=');
        if parts.next() == Some("token") {
            return parts.next().map(str::to_string);
        }
    }
    None
}

// ── Audit log ─────────────────────────────────────────────────────────────────

pub fn append_audit_log(path: Option<&str>, entry: &str) {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("veyn")
                .join("audit.log")
        }
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let ts = chrono::Utc::now().to_rfc3339();
    let line = format!("{} {}\n", ts, entry);
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}
