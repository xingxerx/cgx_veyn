pub mod routes;
pub mod state;

use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
};
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

    // Layer order: last added is outermost (first to process requests).
    // Request flow: cors → host_guard → require_bearer → router
    let app = routes::router(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ))
        .layer(middleware::from_fn_with_state(state.clone(), host_guard))
        .layer(cors);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(addr = %addr, "API listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    info!("API server shut down cleanly");
    Ok(())
}

/// Reject requests whose `Host` header is not localhost or 127.0.0.1,
/// blocking DNS-rebinding attacks. Configured CORS origins are also allowed.
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
