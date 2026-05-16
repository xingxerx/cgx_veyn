pub mod routes;
pub mod state;

use anyhow::Result;
use axum::http::{header, HeaderValue, Method};
use axum::middleware::{from_fn, from_fn_with_state};
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::auth;
use self::state::AppState;

pub async fn serve(state: AppState, port: u16) -> Result<()> {
    let cors = CorsLayer::new()
        .allow_origin([
            format!("http://localhost:{port}")
                .parse::<HeaderValue>()
                .expect("valid origin"),
            format!("http://127.0.0.1:{port}")
                .parse::<HeaderValue>()
                .expect("valid origin"),
        ])
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    // Layer application order: last .layer() is outermost (first to process requests).
    // Request flow: cors → host_guard → require_bearer → router
    let app = routes::router(state.clone())
        .layer(from_fn_with_state(state.clone(), auth::require_bearer))
        .layer(from_fn(auth::host_guard))
        .layer(cors);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("API listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
