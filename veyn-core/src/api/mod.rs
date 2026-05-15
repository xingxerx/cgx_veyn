pub mod routes;
pub mod state;

use anyhow::Result;
use tower_http::cors::CorsLayer;
use tracing::info;

use self::state::AppState;

pub async fn serve(state: AppState, port: u16) -> Result<()> {
    let app = routes::router(state).layer(CorsLayer::permissive());

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("API listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
