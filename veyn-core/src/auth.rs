use std::fs;
use std::path::{Path, PathBuf};

#[allow(unused_imports)]
use anyhow::{bail, Context, Result};
use axum::{
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
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

pub fn scoped_tokens_path() -> PathBuf {
    token_dir().join("tokens.json")
}

// ── Scope-limited tokens ──────────────────────────────────────────────────────

/// A token with an associated permission scope set.
///
/// Scopes:
///   - `"read"` — GET/HEAD only; POST/PUT/DELETE are rejected
///   - `"source:<class>"` — context snapshots are filtered to that source class
///     (e.g. `"source:ble"`, `"source:midi"`)
///   - Empty list / absent — equivalent to full access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedToken {
    pub token: String,
    pub label: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl ScopedToken {
    pub fn is_read_only(&self) -> bool {
        self.scopes.iter().any(|s| s == "read")
    }

    /// Returns `Some(Vec<source_class>)` if the token is limited to specific
    /// source classes, or `None` if all sources are allowed.
    pub fn allowed_sources(&self) -> Option<Vec<String>> {
        let sources: Vec<String> = self
            .scopes
            .iter()
            .filter_map(|s| s.strip_prefix("source:").map(str::to_string))
            .collect();
        if sources.is_empty() {
            None
        } else {
            Some(sources)
        }
    }
}

/// Token claim injected into request extensions after successful auth.
#[derive(Clone, Debug)]
pub struct TokenClaim {
    #[allow(dead_code)]
    pub read_only: bool,
    pub allowed_sources: Option<Vec<String>>,
    #[allow(dead_code)]
    pub label: String,
}

// ── Token load/create ─────────────────────────────────────────────────────────

/// Load the primary full-access token, creating it if it doesn't exist.
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

/// Load optional scope-limited tokens from `tokens.json`.
/// Returns an empty Vec if the file does not exist.
pub fn load_scoped_tokens() -> Vec<ScopedToken> {
    let path = scoped_tokens_path();
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| warn!("tokens.json parse error: {}", e))
            .unwrap_or_default(),
        Err(e) => {
            warn!(path = %path.display(), "cannot read tokens.json: {}", e);
            Vec::new()
        }
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

#[allow(unused_variables)]
fn verify_dir_ownership(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
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
    }
    Ok(())
}

#[allow(unused_variables)]
fn verify_file_ownership(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;
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
    }
    Ok(())
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Enforce `Authorization: Bearer <token>` on every request.
/// Bypassed when `require_auth = false` in config (dev/--no-auth mode).
/// `/health` and `/v1/health` are always public for operator liveness checks.
/// For WebSocket upgrades the token may be supplied as `?token=<value>`.
pub async fn require_bearer(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if !state.config.require_auth {
        req.extensions_mut().insert(TokenClaim {
            read_only: false,
            allowed_sources: None,
            label: "no-auth".to_string(),
        });
        return next.run(req).await;
    }

    let path = req.uri().path();
    if path == "/health" || path == "/v1/health" {
        return next.run(req).await;
    }

    let provided = extract_token(&req);

    // Check primary full-access token.
    if provided.as_deref() == Some(state.auth_token.as_str()) {
        req.extensions_mut().insert(TokenClaim {
            read_only: false,
            allowed_sources: None,
            label: "primary".to_string(),
        });
        return next.run(req).await;
    }

    // Check scope-limited tokens.
    if let Some(tok_str) = &provided {
        if let Some(entry) = state.scoped_tokens.iter().find(|e| e.token == *tok_str) {
            // Enforce read-only scope.
            if entry.is_read_only()
                && req.method() != Method::GET
                && req.method() != Method::HEAD
                && req.method() != Method::OPTIONS
            {
                warn!(label = %entry.label, path = %path, "read-only token rejected write request");
                append_audit_log(
                    state.config.audit_log_path.as_deref(),
                    &format!(
                        "auth_failure reason=read_only_write label={} path={}",
                        entry.label, path
                    ),
                );
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "token is read-only" })),
                )
                    .into_response();
            }
            req.extensions_mut().insert(TokenClaim {
                read_only: entry.is_read_only(),
                allowed_sources: entry.allowed_sources(),
                label: entry.label.clone(),
            });
            return next.run(req).await;
        }
    }

    let reason = if provided.is_none() {
        "missing"
    } else {
        "invalid"
    };
    let path_owned = path.to_owned();
    warn!(path = %path_owned, reason, "auth failure");
    append_audit_log(
        state.config.audit_log_path.as_deref(),
        &format!("auth_failure reason={reason} path={path_owned}"),
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_token_read_only_detection() {
        let tok = ScopedToken {
            token: "abc".to_string(),
            label: "test".to_string(),
            scopes: vec!["read".to_string()],
        };
        assert!(tok.is_read_only());
        assert!(tok.allowed_sources().is_none());
    }

    #[test]
    fn scoped_token_source_filter() {
        let tok = ScopedToken {
            token: "abc".to_string(),
            label: "test".to_string(),
            scopes: vec!["source:ble".to_string(), "source:midi".to_string()],
        };
        assert!(!tok.is_read_only());
        let sources = tok.allowed_sources().unwrap();
        assert_eq!(sources, vec!["ble", "midi"]);
    }

    #[test]
    fn scoped_token_full_access_when_no_scopes() {
        let tok = ScopedToken {
            token: "abc".to_string(),
            label: "test".to_string(),
            scopes: vec![],
        };
        assert!(!tok.is_read_only());
        assert!(tok.allowed_sources().is_none());
    }

    #[test]
    fn extract_token_from_bearer_header() {
        use axum::http::{header, Request};
        let req = Request::builder()
            .header(header::AUTHORIZATION, "Bearer mytoken123")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_token(&req), Some("mytoken123".to_string()));
    }

    #[test]
    fn extract_token_from_query_string() {
        use axum::http::Request;
        let req = Request::builder()
            .uri("http://localhost/stream?token=abc123&other=val")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_token(&req), Some("abc123".to_string()));
    }

    #[test]
    fn extract_token_missing_returns_none() {
        use axum::http::Request;
        let req = Request::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_token(&req), None);
    }
}
