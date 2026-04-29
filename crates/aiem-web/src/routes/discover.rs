use axum::extract::{Form, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::discover;

use crate::events::ResourceKind;
use crate::layout::{btn_primary, btn_secondary, empty_state, page, page_header, settings_group};
use crate::state::AppState;
use crate::tasks::{invalidate, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/discover", get(index))
        .route("/discover/import-skill", post(import_skill))
        .route("/discover/import-all-skills", post(import_all_skills))
        .route("/discover/import-mcp", post(import_mcp))
        .route("/discover/import-all-mcp", post(import_all_mcp))
}

async fn index(State(_st): State<AppState>) -> Markup {
    let skills = discover::discover_skills().unwrap_or_default();
    let mcps = discover::discover_mcp().unwrap_or_default();
    page(
        "Discover",
        "/discover",
        html! {
            (page_header("Discover", "Scan your IDEs for unmanaged skills & MCP servers.", html!{}))
            div class="content-padding wide-content" {
            (settings_group("", html!{
                div style="padding:12px 16px;display:flex;align-items:center;justify-content:space-between" {
                    div style="font-size:14px;font-weight:600" { "Unmanaged skills (" (skills.len()) ")" }
                    @if !skills.is_empty() {
                        form hx-post="/discover/import-all-skills" hx-swap="none" class="flex gap-2 items-center" {
                            label class="meta" {
                                input type="checkbox" name="copy" value="true" checked;
                                " Copy to ~/.aiem"
                            }
                            (btn_primary("Import all"))
                        }
                    }
                }
                @if skills.is_empty() {
                    (empty_state("Nothing to import", "All skills on disk are already managed."))
                } @else {
                    table class="aiem" {
                        thead { tr { th{"Directory"} th{"IDE"} th{"Path"} th style="text-align:right"{"Actions"} } }
                        tbody {
                            @for s in &skills {
                                tr {
                                    td style="font-weight:500" { (s.dir_name) }
                                    td class="meta" { (s.ide_id) }
                                    td class="mono meta" style="word-break:break-all" { (s.path.display().to_string()) }
                                    td style="text-align:right" {
                                        form hx-post="/discover/import-skill" hx-swap="none" class="flex gap-2 items-center justify-end" {
                                            input type="hidden" name="path" value=(s.path.display().to_string());
                                            input type="hidden" name="ide" value=(s.ide_id);
                                            input type="hidden" name="name" value=(s.dir_name);
                                            label class="meta" {
                                                input type="checkbox" name="copy" value="true" checked;
                                                " Copy"
                                            }
                                            (btn_secondary("Import"))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }))
            (settings_group("", html!{
                div style="padding:12px 16px;display:flex;align-items:center;justify-content:space-between" {
                    div style="font-size:14px;font-weight:600" { "Unmanaged MCP servers (" (mcps.len()) ")" }
                    @if !mcps.is_empty() {
                        form hx-post="/discover/import-all-mcp" hx-swap="none" { (btn_primary("Import all")) }
                    }
                }
                @if mcps.is_empty() {
                    (empty_state("Nothing to import", "No unmanaged MCP servers detected."))
                } @else {
                    table class="aiem" {
                        thead { tr { th{"Name"} th{"IDE"} th{"Transport"} th style="text-align:right"{"Actions"} } }
                        tbody {
                            @for m in &mcps {
                                tr {
                                    td style="font-weight:500" { (m.server.name) }
                                    td class="meta" { (m.source_ide) }
                                    td class="mono meta" { (format!("{:?}", m.server.transport)) }
                                    td style="text-align:right" {
                                        form hx-post="/discover/import-mcp" hx-swap="none" {
                                            input type="hidden" name="name" value=(m.server.name);
                                            (btn_secondary("Import"))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }))
            } // content-padding
        },
    )
}

#[derive(Deserialize)]
struct ImportSkillForm {
    path: String,
    ide: String,
    name: String,
    #[serde(default)]
    copy: bool,
}

async fn import_skill(State(st): State<AppState>, Form(f): Form<ImportSkillForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let found = discover::FoundSkill {
        path: std::path::PathBuf::from(&f.path),
        ide_id: f.ide,
        dir_name: f.name.clone(),
        is_link: false,
    };
    match discover::import_skill(&found, f.copy) {
        Ok(skill) => {
            if let Ok(mut reg) = aiem_core::skills::SkillRegistry::load() {
                reg.upsert(skill);
                let _ = reg.save();
                toast_info(&tx, format!("imported {}", f.name));
                invalidate(&tx, ResourceKind::Skills);
            }
        }
        Err(e) => toast_error(&tx, format!("import: {e}")),
    }
    ok()
}

#[derive(Deserialize)]
struct ImportAllSkillsForm {
    #[serde(default)]
    copy: bool,
}

async fn import_all_skills(
    State(st): State<AppState>,
    Form(f): Form<ImportAllSkillsForm>,
) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    match discover::discover_skills() {
        Ok(found) => match discover::import_all_skills(&found, f.copy) {
            Ok(n) => {
                toast_info(&tx, format!("imported {n} skill(s)"));
                invalidate(&tx, ResourceKind::Skills);
            }
            Err(e) => toast_error(&tx, format!("{e}")),
        },
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    ok()
}

#[derive(Deserialize)]
struct ImportMcpForm {
    name: String,
}

async fn import_mcp(State(st): State<AppState>, Form(f): Form<ImportMcpForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    match discover::discover_mcp() {
        Ok(found) => {
            if let Some(m) = found.iter().find(|m| m.server.name == f.name) {
                match discover::import_mcp(m) {
                    Ok(()) => {
                        toast_info(&tx, format!("imported {}", f.name));
                        invalidate(&tx, ResourceKind::Mcp);
                    }
                    Err(e) => toast_error(&tx, format!("{e}")),
                }
            } else {
                toast_error(&tx, "not found");
            }
        }
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    ok()
}

async fn import_all_mcp(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    match discover::discover_mcp() {
        Ok(found) => match discover::import_all_mcp(&found) {
            Ok(n) => {
                toast_info(&tx, format!("imported {n} MCP server(s)"));
                invalidate(&tx, ResourceKind::Mcp);
            }
            Err(e) => toast_error(&tx, format!("{e}")),
        },
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    ok()
}

fn ok() -> Response {
    (axum::http::StatusCode::OK, "ok").into_response()
}
