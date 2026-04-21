use axum::extract::{Form, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::projects::{Project, ProjectStore};

use crate::events::ResourceKind;
use crate::layout::{btn_danger, btn_primary, card, empty_state, page, page_header};
use crate::state::AppState;
use crate::tasks::{invalidate, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(index))
        .route("/projects/fragment", get(fragment))
        .route("/projects/add", post(add))
        .route("/projects/remove", post(remove))
}

async fn index(State(st): State<AppState>) -> Markup {
    page("Projects", "/projects", html!{
        (page_header("Projects", "Register workspace roots so you can deploy skills / MCP per project.", html!{}))
        (card(html!{
            div class="text-sm font-semibold mb-3" { "Register project" }
            form hx-post="/projects/add" hx-swap="none" class="grid gap-3" style="grid-template-columns:1fr 2fr auto" {
                div { label class="label" { "Name *" } input name="name" required class="field"; }
                div { label class="label" { "Absolute path *" } input name="path" required placeholder="/home/user/my-project" class="field"; }
                div class="flex items-end" { (btn_primary("Register")) }
            }
        }))
        div data-resource="projects"
            hx-get="/projects/fragment"
            hx-trigger="refresh from:body, load"
            hx-swap="innerHTML" { (render(st.projects().ok().as_ref())) }
    })
}

async fn fragment(State(st): State<AppState>) -> Markup {
    render(st.projects().ok().as_ref())
}

fn render(store: Option<&ProjectStore>) -> Markup {
    let Some(s) = store else { return html!{ div class="aiem-card" style="color:var(--danger)" {"Load failed."} }; };
    let items: Vec<&Project> = s.list().collect();
    if items.is_empty() {
        return empty_state("No projects registered", "Register one above, then use it as a deploy scope.");
    }
    card(html!{
        table class="aiem" {
            thead { tr { th{"Name"} th{"Path"} th{"IDEs"} th{"Skills"} th{"MCP"} th style="text-align:right"{"Actions"} } }
            tbody {
                @for p in &items {
                    tr {
                        td style="font-weight:500" { (p.name) }
                        td class="mono meta" style="word-break:break-all" { (p.path) }
                        td class="meta" { (p.ides.join(", ")) }
                        td class="meta" { (p.skills.len()) }
                        td class="meta" { (p.mcp_servers.len()) }
                        td style="text-align:right" {
                            form hx-post="/projects/remove" hx-swap="none" hx-confirm="Remove project entry?" {
                                input type="hidden" name="path" value=(p.path);
                                (btn_danger("Remove"))
                            }
                        }
                    }
                }
            }
        }
    })
}

#[derive(Deserialize)]
struct AddForm { name: String, path: String }

async fn add(State(st): State<AppState>, Form(f): Form<AddForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut store = match ProjectStore::load() { Ok(s)=>s, Err(e)=>{toast_error(&tx,format!("{e}"));return ok();} };
    store.upsert(Project { name: f.name.clone(), path: f.path, ides: vec![], skills: vec![], mcp_servers: vec![] });
    if let Err(e) = store.save() { toast_error(&tx, format!("{e}")); return ok(); }
    toast_info(&tx, format!("registered {}", f.name));
    invalidate(&tx, ResourceKind::Projects);
    ok()
}

#[derive(Deserialize)]
struct RemoveForm { path: String }

async fn remove(State(st): State<AppState>, Form(f): Form<RemoveForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut store = match ProjectStore::load() { Ok(s)=>s, Err(e)=>{toast_error(&tx,format!("{e}"));return ok();} };
    if let Err(e) = store.remove(&f.path) { toast_error(&tx, format!("{e}")); return ok(); }
    let _ = store.save();
    toast_info(&tx, "removed");
    invalidate(&tx, ResourceKind::Projects);
    ok()
}

fn ok() -> Response { (axum::http::StatusCode::OK, "ok").into_response() }
