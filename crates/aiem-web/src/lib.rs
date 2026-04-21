//! aiem-web: headless browser-based management UI.
//!
//! Exposes a single entry point [`serve`] that starts an axum HTTP server.
//! Default bind is `127.0.0.1:8787`; intended to be exposed via SSH port
//! forwarding from the developer's laptop to the remote Linux box.

mod state;
mod events;
mod tasks;
mod layout;
mod fs_merge;
mod routes;

use std::net::SocketAddr;

pub use state::AppState;
pub use events::{UiEvent, ToastLevel, ResourceKind};

use axum::Router;
use tower_http::trace::TraceLayer;

pub struct ServeConfig {
    pub addr: SocketAddr,
    pub open_browser: bool,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 8787)),
            open_browser: false,
        }
    }
}

/// Start the Web UI server and block until Ctrl-C.
pub async fn serve(cfg: ServeConfig) -> anyhow::Result<()> {
    aiem_core::paths::ensure_layout()?;

    let state = AppState::new()?;

    let app = Router::new()
        .merge(routes::layout::router())
        .merge(routes::skills::router())
        .merge(routes::mcp::router())
        .merge(routes::secrets::router())
        .merge(routes::settings::router())
        .merge(routes::ides::router())
        .merge(routes::profiles::router())
        .merge(routes::projects::router())
        .merge(routes::discover::router())
        .merge(routes::store::router())
        .merge(routes::events::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!("aiem-web listening on http://{}", cfg.addr);
    eprintln!("\n  aiem-web is running:  http://{}\n", cfg.addr);
    eprintln!("  (on a remote box, use:  ssh -L {}:localhost:{} user@host)\n",
        cfg.addr.port(), cfg.addr.port());

    if cfg.open_browser {
        let url = format!("http://{}", cfg.addr);
        let _ = webbrowser_open(&url);
    }

    let listener = tokio::net::TcpListener::bind(cfg.addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}

#[cfg(target_os = "windows")]
fn webbrowser_open(url: &str) -> std::io::Result<()> {
    std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn()?;
    Ok(())
}
#[cfg(target_os = "macos")]
fn webbrowser_open(url: &str) -> std::io::Result<()> {
    std::process::Command::new("open").arg(url).spawn()?;
    Ok(())
}
#[cfg(all(unix, not(target_os = "macos")))]
fn webbrowser_open(url: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open").arg(url).spawn()?;
    Ok(())
}
