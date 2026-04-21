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
///
/// If the configured port is already in use, automatically falls back to
/// the next available port (tries up to 10 ports before giving up).
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

    // Try the requested port first; if it is in use, probe the next few ports.
    let listener = bind_with_fallback(cfg.addr, 10).await?;
    let actual_addr = listener.local_addr()?;

    tracing::info!("aiem-web listening on http://{}", actual_addr);
    eprintln!("\n  aiem-web is running:  http://{}\n", actual_addr);
    eprintln!("  (on a remote box, use:  ssh -L {port}:localhost:{port} user@host)\n",
        port = actual_addr.port());
    if actual_addr.port() != cfg.addr.port() {
        eprintln!(
            "  Note: port {} was in use — using {} instead.\n",
            cfg.addr.port(), actual_addr.port()
        );
    }

    if cfg.open_browser {
        let url = format!("http://{}", actual_addr);
        let _ = webbrowser_open(&url);
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Try to bind `addr`; if the port is taken, increment port and retry up to
/// `max_tries` times.  Returns the first successful listener.
async fn bind_with_fallback(
    addr: std::net::SocketAddr,
    max_tries: u16,
) -> anyhow::Result<tokio::net::TcpListener> {
    let mut last_err = None;
    for offset in 0..max_tries {
        let port = addr.port().saturating_add(offset);
        let candidate = std::net::SocketAddr::new(addr.ip(), port);
        match tokio::net::TcpListener::bind(candidate).await {
            Ok(l) => return Ok(l),
            Err(e) => {
                tracing::debug!("port {} in use: {}", port, e);
                last_err = Some(e);
            }
        }
    }
    Err(anyhow::anyhow!(
        "Could not bind to any port in range {}–{}: {}",
        addr.port(),
        addr.port().saturating_add(max_tries - 1),
        last_err.unwrap()
    ))
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
