//! mp-daemon — HTTP admission server for manifold-plane.

use mp_daemon::{app, AppState, DaemonInner};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let addr_str = std::env::var("MP_ENGINE_ADDR").unwrap_or_else(|_| "0.0.0.0:8787".into());
    let addr: SocketAddr = addr_str.parse().expect("MP_ENGINE_ADDR must be host:port");

    let state = AppState {
        inner: Arc::new(RwLock::new(DaemonInner::new_default())),
    };
    let router = app(state);

    tracing::info!("mp-daemon listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, router).await.expect("serve");
}
