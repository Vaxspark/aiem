use axum::extract::{Form, Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::profiles::{Profile, ProfileStore};

use crate::events::ResourceKind;
use crate::layout::{
    btn_danger, btn_primary, btn_secondary, empty_state, page, page_header, settings_group, tag,
    TagKind,
};
use crate::state::AppState;
use crate::tasks::{invalidate, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/profiles", get(index))
        .route("/profiles/fragment", get(fragment))
        .route("/profiles/create", post(create))
        .route("/profiles/:name/activate", post(activate))
        .route("/profiles/deactivate", post(deactivate))
        .route("/profiles/:name/remove", post(remove))
}

async fn index(State(st): State<AppState>) -> Markup {
    page(
        "Profiles",
        "/profiles",
        html! {
            (page_header("Profiles", "", html!{}))
            div class="content-padding" {
                (settings_group("Create profile", html! {
                    div style="padding:12px 16px" {
                        form hx-post="/profiles/create" hx-swap="none" class="grid gap-3" style="grid-template-columns:1fr 1fr auto" {
                            div {
                                label class="label" { "Name *" }
                                input name="name" required class="field";
                            }
                            div {
                                label class="label" { "Description" }
                                input name="description" class="field";
                            }
                            div class="flex items-end" { (btn_primary("Create")) }
                        }
                    }
                }))
                div data-resource="profiles"
                    hx-get="/profiles/fragment"
                    hx-trigger="refresh from:body, load"
                    hx-swap="innerHTML" { (render(st.profiles().ok().as_ref())) }
            }
        },
    )
}

async fn fragment(State(st): State<AppState>) -> Markup {
    render(st.profiles().ok().as_ref())
}

fn render(store: Option<&ProfileStore>) -> Markup {
    let Some(s) = store else {
        return html! { div style="color:var(--danger);padding:16px" {"Load failed."} };
    };
    let items: Vec<&Profile> = s.list().collect();
    if items.is_empty() {
        return empty_state(
            "No profiles yet",
            "Create one above to bundle skills & MCP servers.",
        );
    }
    let active = s.active_name().map(|s| s.to_string());
    settings_group(
        "",
        html! {
            table class="aiem" {
                thead { tr { th{"Name"} th{"Description"} th{"Skills / MCP"} th{"Status"} th style="text-align:right"{"Actions"} } }
                tbody {
                    @for p in &items {
                        @let is_active = active.as_deref() == Some(p.name.as_str());
                        tr {
                            td style="font-weight:500" { (p.name) }
                            td class="meta" { (p.description.clone().unwrap_or_default()) }
                            td class="meta" { (p.skills.len()) " skills \u{b7} " (p.mcp_servers.len()) " MCP" }
                            td {
                                @if is_active { (tag("active", TagKind::Success)) }
                                @else        { span class="meta" { "\u{2014}" } }
                            }
                            td style="text-align:right;white-space:nowrap" {
                                div class="row-gap" style="justify-content:flex-end" {
                                    @if is_active {
                                        form hx-post="/profiles/deactivate" hx-swap="none" { (btn_secondary("Deactivate")) }
                                    } @else {
                                        form hx-post=(format!("/profiles/{}/activate", p.name)) hx-swap="none" { (btn_secondary("Activate")) }
                                    }
                                    form hx-post=(format!("/profiles/{}/remove", p.name)) hx-swap="none" hx-confirm="Delete profile?" { (btn_danger("Delete")) }
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
struct CreateForm {
    name: String,
    #[serde(default)]
    description: String,
}

async fn create(State(st): State<AppState>, Form(f): Form<CreateForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut store = match ProfileStore::load() {
        Ok(s) => s,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    store.upsert(Profile {
        name: f.name.clone(),
        description: if f.description.is_empty() {
            None
        } else {
            Some(f.description)
        },
        skills: vec![],
        mcp_servers: vec![],
    });
    if let Err(e) = store.save() {
        toast_error(&tx, format!("{e}"));
        return ok();
    }
    toast_info(&tx, format!("created {}", f.name));
    invalidate(&tx, ResourceKind::Profiles);
    ok()
}

async fn activate(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut store = match ProfileStore::load() {
        Ok(s) => s,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    if let Err(e) = store.set_active(Some(&name)) {
        toast_error(&tx, format!("{e}"));
        return ok();
    }
    let _ = store.save();
    toast_info(&tx, format!("activated {name}"));
    invalidate(&tx, ResourceKind::Profiles);
    ok()
}

async fn deactivate(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut store = match ProfileStore::load() {
        Ok(s) => s,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    let _ = store.set_active(None);
    let _ = store.save();
    toast_info(&tx, "deactivated");
    invalidate(&tx, ResourceKind::Profiles);
    ok()
}

async fn remove(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut store = match ProfileStore::load() {
        Ok(s) => s,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    if let Err(e) = store.remove(&name) {
        toast_error(&tx, format!("{e}"));
        return ok();
    }
    let _ = store.save();
    toast_info(&tx, format!("removed {name}"));
    invalidate(&tx, ResourceKind::Profiles);
    ok()
}

fn ok() -> Response {
    (axum::http::StatusCode::OK, "ok").into_response()
}
