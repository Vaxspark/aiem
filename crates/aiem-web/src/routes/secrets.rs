use axum::extract::{Form, Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::secrets::Vault;

use crate::events::ResourceKind;
use crate::layout::{btn_danger, btn_primary, empty_state, page, page_header, settings_group};
use crate::state::AppState;
use crate::tasks::{invalidate, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/secrets", get(index))
        .route("/secrets/fragment", get(fragment))
        .route("/secrets/set", post(set))
        .route("/secrets/:name/remove", post(remove))
}

async fn index(State(st): State<AppState>) -> Markup {
    page(
        "Secrets",
        "/secrets",
        html! {
            (page_header("Secrets", "OS-keyring backed vault. Reference via ${secret:NAME} in MCP env/headers.", html!{}))
            div class="content-padding wide-content" {
                (form_view())
                div data-resource="secrets"
                    hx-get="/secrets/fragment"
                    hx-trigger="refresh from:body, load"
                    hx-swap="innerHTML" { (render(st.vault().ok().as_ref())) }
            }
        },
    )
}

async fn fragment(State(st): State<AppState>) -> Markup {
    render(st.vault().ok().as_ref())
}

fn form_view() -> Markup {
    settings_group(
        "Set secret",
        html! {
            div style="padding:12px 16px" {
                form hx-post="/secrets/set" hx-swap="none" class="grid gap-3" style="grid-template-columns:1fr 2fr auto" {
                    div { label class="label" { "Name *" } input name="name" required class="field"; }
                    div { label class="label" { "Value *" } input name="value" type="password" required class="field"; }
                    div class="flex items-end" { (btn_primary("Save")) }
                    div style="grid-column:1/-1" { label class="label" { "Description" } input name="description" class="field"; }
                }
            }
        },
    )
}

fn render(vault: Option<&Vault>) -> Markup {
    let Some(v) = vault else {
        return html! { div style="color:var(--danger);padding:16px" { "Failed to load vault." } };
    };
    let names: Vec<&String> = v.names().collect();
    if names.is_empty() {
        return empty_state(
            "No secrets stored",
            "Add one above — values live in the OS keyring, not on disk.",
        );
    }
    settings_group(
        "",
        html! {
            table class="aiem" {
                thead { tr { th{"Name"} th{"Description"} th{"Updated"} th style="text-align:right"{"Actions"} } }
                tbody {
                    @for n in &names {
                        @let meta = v.meta(n);
                        tr {
                            td class="mono" { (n) }
                            td class="meta" { (meta.and_then(|m| m.description.clone()).unwrap_or_default()) }
                            td class="meta" { (meta.map(|m| m.updated_at.to_rfc3339()).unwrap_or_default()) }
                            td style="text-align:right" {
                                form hx-post=(format!("/secrets/{n}/remove")) hx-swap="none" hx-confirm="Delete secret?" {
                                    (btn_danger("Delete"))
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

#[derive(Deserialize)]
struct SetForm {
    name: String,
    value: String,
    #[serde(default)]
    description: String,
}

async fn set(State(st): State<AppState>, Form(f): Form<SetForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut v = match Vault::load() {
        Ok(v) => v,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    let desc = if f.description.is_empty() {
        None
    } else {
        Some(f.description)
    };
    match v.set(&f.name, &f.value, desc) {
        Ok(()) => {
            toast_info(&tx, format!("saved {}", f.name));
            invalidate(&tx, ResourceKind::Secrets);
        }
        Err(e) => toast_error(&tx, format!("set: {e}")),
    }
    ok()
}

async fn remove(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut v = match Vault::load() {
        Ok(v) => v,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    match v.delete(&name) {
        Ok(()) => {
            toast_info(&tx, format!("deleted {name}"));
            invalidate(&tx, ResourceKind::Secrets);
        }
        Err(e) => toast_error(&tx, format!("delete: {e}")),
    }
    ok()
}

fn ok() -> Response {
    (axum::http::StatusCode::OK, "ok").into_response()
}
