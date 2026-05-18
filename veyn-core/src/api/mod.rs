pub mod routes;
pub mod state;

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use serde_json::json;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tracing::info;

use self::state::AppState;
use crate::auth;

pub async fn serve(
    state: AppState,
    port: u16,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let cors = build_cors(&state.config.cors_origins, port);

    // Build optional per-IP rate limiter.
    let rate_limiter: Option<Arc<DefaultKeyedRateLimiter<IpAddr>>> =
        state.config.rate_limit_rps.and_then(|rps| {
            NonZeroU32::new(rps).map(|n| {
                info!(rps, "rate limiting enabled");
                Arc::new(RateLimiter::keyed(Quota::per_second(n)))
            })
        });

    // Layer order: last added = outermost (first to process requests).
    // Request flow: cors → host_guard → rate_limit → require_bearer → router
    let app = routes::router(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ))
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(state.clone(), host_guard))
        .layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(addr = %addr, "API listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await?;

    info!("API server shut down cleanly");
    Ok(())
}

// ── Rate limiting ─────────────────────────────────────────────────────────────

async fn rate_limit_middleware(
    State(limiter): State<Option<Arc<DefaultKeyedRateLimiter<IpAddr>>>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    if let Some(lim) = limiter {
        let ip = addr.ip();
        if lim.check_key(&ip).is_err() {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limit exceeded" })),
            )
                .into_response();
        }
    }
    next.run(req).await
}

// ── Host guard ────────────────────────────────────────────────────────────────

/// Reject requests whose `Host` header is not localhost or 127.0.0.1,
/// blocking DNS-rebinding attacks.
async fn host_guard(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(host_hdr) = req.headers().get(axum::http::header::HOST) {
        if let Ok(host) = host_hdr.to_str() {
            let port = state.config.api_port;
            let local = [
                format!("localhost:{port}"),
                format!("127.0.0.1:{port}"),
                "localhost".to_string(),
                "127.0.0.1".to_string(),
            ];
            let in_allowlist = state.config.cors_origins.iter().any(|o| o.contains(host));
            if !local.iter().any(|h| h == host) && !in_allowlist {
                tracing::warn!(host = %host, "rejected unexpected Host header (DNS-rebinding guard)");
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }
    Ok(next.run(req).await)
}

fn build_cors(origins: &[String], port: u16) -> CorsLayer {
    let allow_headers = AllowHeaders::list([
        axum::http::header::AUTHORIZATION,
        axum::http::header::CONTENT_TYPE,
    ]);
    let allow_methods = AllowMethods::list([Method::GET, Method::POST, Method::OPTIONS]);

    if origins.is_empty() {
        let localhost: HeaderValue = format!("http://localhost:{port}")
            .parse()
            .expect("valid header value");
        CorsLayer::new()
            .allow_origin(AllowOrigin::exact(localhost))
            .allow_methods(allow_methods)
            .allow_headers(allow_headers)
    } else {
        let parsed: Vec<HeaderValue> = origins.iter().filter_map(|o| o.parse().ok()).collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(parsed))
            .allow_methods(allow_methods)
            .allow_headers(allow_headers)
    }
}
