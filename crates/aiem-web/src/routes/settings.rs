use axum::extract::{Form, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::secrets::Vault;

use crate::layout::{btn_danger, btn_primary, card, page, page_header, tag, TagKind};
use crate::state::AppState;
use crate::tasks::{toast_error, toast_info};

const GH_TOKEN_NAME: &str = "github_token";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(index))
        .route("/settings/github-token", post(save_token))
        .route("/settings/github-token/clear", post(clear_token))
}

async fn index(State(_st): State<AppState>) -> Markup {
    let has_token = std::env::var("GITHUB_TOKEN").map(|s| !s.is_empty()).unwrap_or(false);
    let aiem_home = aiem_core::paths::home().map(|p| p.display().to_string()).unwrap_or_else(|_| "?".into());
    page("Settings", "/settings", html! {
        (page_header("Settings", "Credentials, paths, and runtime info.", html!{}))
        (card(html!{
            div class="text-sm font-semibold mb-2" { "GitHub Token" }
            p class="meta mb-3" {
                "Stored in the OS keyring. Loaded automatically at startup. "
                "Only needed to avoid the 60-request/hour anonymous rate limit."
            }
            div class="mb-3" {
                @if has_token { (tag("● configured", TagKind::Success)) }
                @else         { span class="meta" { "○ no token set" } }
            }
            form hx-post="/settings/github-token" hx-swap="none" class="flex gap-2 items-end" {
                input name="token" type="password" placeholder="ghp_..." required class="field" style="flex:1";
                (btn_primary("Save"))
            }
            form hx-post="/settings/github-token/clear" hx-swap="none" hx-confirm="Clear stored token?" style="margin-top:8px" {
                (btn_danger("Clear"))
            }
        }))
        (card(html!{
            div class="text-sm font-semibold mb-2" { "Paths" }
            dl class="grid gap-2" style="grid-template-columns:160px 1fr;font-size:13px" {
                dt class="meta" { "AIEM_HOME" } dd class="mono" { (aiem_home) }
                dt class="meta" { "Hostname" }  dd class="mono" { (hostname()) }
                dt class="meta" { "User" }      dd class="mono" { (whoami()) }
                dt class="meta" { "OS" }        dd class="mono" { (std::env::consts::OS) " / " (std::env::consts::ARCH) }
            }
        }))
        (card(html!{
            div class="text-sm font-semibold mb-2" { "About" }
            p class="meta" { "aiem-web " (env!("CARGO_PKG_VERSION")) " — headless management for skills & MCP across IDEs." }
        }))
    })
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

#[derive(Deserialize)]
struct TokenForm { token: String }

async fn save_token(State(st): State<AppState>, Form(f): Form<TokenForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut vault = match Vault::load() { Ok(v) => v, Err(e) => { toast_error(&tx, format!("{e}")); return ok(); } };
    match vault.set(GH_TOKEN_NAME, &f.token, Some("GitHub Personal Access Token".into())) {
        Ok(()) => {
            std::env::set_var("GITHUB_TOKEN", &f.token);
            toast_info(&tx, "GITHUB_TOKEN saved to keyring");
        }
        Err(e) => toast_error(&tx, format!("keyring: {e}")),
    }
    ok()
}

async fn clear_token(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut vault = match Vault::load() { Ok(v) => v, Err(e) => { toast_error(&tx, format!("{e}")); return ok(); } };
    match vault.delete(GH_TOKEN_NAME) {
        Ok(()) => { std::env::remove_var("GITHUB_TOKEN"); toast_info(&tx, "cleared"); }
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    ok()
}

fn ok() -> Response { (axum::http::StatusCode::OK, "ok").into_response() }
