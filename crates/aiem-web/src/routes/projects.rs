use std::collections::BTreeSet;
use std::path::Path;

use axum::extract::{Form, Query, RawForm, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::ide;
use aiem_core::mcp::{self, McpRegistry};
use aiem_core::projects::{self, Project, ProjectStore};
use aiem_core::skills::{install, SkillRegistry};

use crate::events::ResourceKind;
use crate::layout::{
    btn_danger, btn_primary, btn_secondary, card, empty_state, page, page_header, settings_group,
    tag, TagKind,
};
use crate::state::AppState;
use crate::tasks::{invalidate, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(index))
        .route("/projects/fragment", get(fragment))
        .route("/projects/add", post(add))
        .route("/projects/save", post(save))
        .route("/projects/sync", post(sync_project))
        .route("/projects/remove", post(remove))
}

#[derive(Deserialize, Default)]
struct IndexQuery {
    #[serde(default)]
    edit: Option<String>,
}

async fn index(State(st): State<AppState>, Query(q): Query<IndexQuery>) -> Markup {
    let store = st.projects().ok();
    let skills = SkillRegistry::load().ok();
    let mcp = McpRegistry::load().ok();
    page(
        "Projects",
        "/projects",
        html! {
            (page_header("Projects", "", html!{}))
            div class="content-padding wide-content" {
                (settings_group("Register project", html! {
                    div style="padding:12px 16px" {
                        form hx-post="/projects/add" hx-swap="none" class="grid gap-3" style="grid-template-columns:2fr 1fr auto" {
                            div { label class="label" { "Absolute path *" } input name="path" required placeholder="/home/user/my-project" class="field"; }
                            div { label class="label" { "Name" } input name="name" placeholder="auto from folder" class="field"; }
                            div class="flex items-end" { (btn_primary("Register")) }
                        }
                    }
                }))
                @if let (Some(store), Some(edit)) = (store.as_ref(), q.edit.as_deref()) {
                    @if let Some(project) = store.get(edit) {
                        (render_editor(project, skills.as_ref(), mcp.as_ref()))
                    }
                }
                div data-resource="projects"
                    hx-get="/projects/fragment"
                    hx-trigger="refresh from:body, load"
                    hx-swap="innerHTML" { (render(store.as_ref())) }
            }
        },
    )
}

async fn fragment(State(st): State<AppState>) -> Markup {
    render(st.projects().ok().as_ref())
}

fn render(store: Option<&ProjectStore>) -> Markup {
    let Some(s) = store else {
        return html! { div style="color:var(--danger);padding:16px" {"Load failed."} };
    };
    let items: Vec<&Project> = s.list().collect();
    if items.is_empty() {
        return empty_state(
            "No projects registered",
            "Register one above, then configure skills and MCP for that project.",
        );
    }
    settings_group(
        "",
        html! {
            table class="aiem" {
                thead { tr { th{"Name"} th{"Path"} th{"IDEs"} th{"Skills"} th{"MCP"} th style="text-align:right"{"Actions"} } }
                tbody {
                    @for p in &items {
                        tr {
                            td style="font-weight:500" { (p.name) }
                            td class="mono meta" style="word-break:break-all" { (p.path) }
                            td class="meta" {
                                @if p.ides.is_empty() { "—" }
                                @else {
                                    @for ide_id in &p.ides { (tag(ide_id, TagKind::Neutral)) " " }
                                }
                            }
                            td class="meta" { (p.skills.len()) }
                            td class="meta" { (p.mcp_servers.len()) }
                            td style="text-align:right;white-space:nowrap" {
                                a class="btn" href=(format!("/projects?edit={}", urlencoding::encode(&p.path))) { "Configure" }
                                form hx-post="/projects/sync" hx-swap="none" style="display:inline" {
                                    input type="hidden" name="path" value=(p.path);
                                    (btn_secondary("Sync"))
                                }
                                form hx-post="/projects/remove" hx-swap="none" hx-confirm="Remove project entry and undeploy its managed skills/MCP?" style="display:inline" {
                                    input type="hidden" name="path" value=(p.path);
                                    (btn_danger("Remove"))
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

fn render_editor(
    project: &Project,
    skills: Option<&SkillRegistry>,
    mcp: Option<&McpRegistry>,
) -> Markup {
    let selected_ides: BTreeSet<&str> = project.ides.iter().map(String::as_str).collect();
    let selected_skills: BTreeSet<&str> = project.skills.iter().map(String::as_str).collect();
    let selected_servers: BTreeSet<&str> = project.mcp_servers.iter().map(String::as_str).collect();

    card(html! {
        div class="flex items-center justify-between mb-2" {
            div {
                div class="text-sm font-semibold" { "Configure: " (project.name) }
                div class="meta mono" style="word-break:break-all" { (project.path) }
            }
            a class="btn-ghost" href="/projects" { "Close" }
        }
        form hx-post="/projects/save" hx-swap="none" class="grid gap-4" {
            input type="hidden" name="path" value=(project.path);

            div {
                div class="text-xs font-semibold mb-1" { "Target IDEs" }
                div class="row-gap" {
                    @for target in ide::IDES {
                        @let checked = selected_ides.contains(target.id);
                        label class="tag" {
                            input type="checkbox" name="ides" value=(target.id) checked[checked];
                            " " (target.display_name)
                        }
                    }
                }
            }

            div class="project-picker-grid" {
                div class="project-picker" {
                    div class="text-xs font-semibold mb-1" { "Skills" }
                    div class="meta mb-2" { "Choose skills to link into this project." }
                    div class="check-list project-check-list" {
                        @if let Some(reg) = skills {
                            @for skill in reg.list() {
                                @let checked = selected_skills.contains(skill.id.as_str());
                                label {
                                    input type="checkbox" name="skills" value=(skill.id) checked[checked];
                                    " " (short_id(&skill.id))
                                }
                            }
                        } @else {
                            span class="meta" { "Failed to load skills." }
                        }
                    }
                }
                div class="project-picker" {
                    div class="text-xs font-semibold mb-1" { "MCP Servers" }
                    div class="meta mb-2" { "Choose servers to write into project MCP configs." }
                    div class="check-list project-check-list" {
                        @if let Some(reg) = mcp {
                            @for server in reg.list() {
                                @let checked = selected_servers.contains(server.name.as_str());
                                label {
                                    input type="checkbox" name="mcp_servers" value=(server.name) checked[checked];
                                    " " (server.name)
                                }
                            }
                        } @else {
                            span class="meta" { "Failed to load MCP servers." }
                        }
                    }
                }
            }

            div class="flex gap-2" {
                button type="submit" name="deploy" value="true" class="btn-primary" { "Save & Deploy" }
                (btn_secondary("Save only"))
            }
        }
    })
}

#[derive(Deserialize)]
struct AddForm {
    #[serde(default)]
    name: String,
    path: String,
}

async fn add(State(st): State<AppState>, Form(f): Form<AddForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let path = f.path.trim();
    if path.is_empty() {
        toast_error(&tx, "path is required");
        return ok();
    }
    if !Path::new(path).is_dir() {
        toast_error(&tx, "directory does not exist on this server");
        return ok();
    }
    let name = if f.name.trim().is_empty() {
        Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string())
    } else {
        f.name.trim().to_string()
    };
    let mut store = match ProjectStore::load() {
        Ok(s) => s,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    let detected = projects::detect_project_ides(Path::new(path));
    store.upsert(Project {
        name: name.clone(),
        path: path.to_string(),
        ides: detected,
        skills: vec![],
        mcp_servers: vec![],
    });
    if let Err(e) = store.save() {
        toast_error(&tx, format!("{e}"));
        return ok();
    }
    toast_info(&tx, format!("registered {name}"));
    invalidate(&tx, ResourceKind::Projects);
    ok()
}

#[derive(Default)]
struct SaveForm {
    path: String,
    ides: Vec<String>,
    skills: Vec<String>,
    mcp_servers: Vec<String>,
    deploy: Option<String>,
}

impl SaveForm {
    fn from_urlencoded(body: &[u8]) -> Self {
        let mut form = SaveForm::default();
        let raw = String::from_utf8_lossy(body);
        for pair in raw.split('&').filter(|part| !part.is_empty()) {
            let (key_raw, value_raw) = pair.split_once('=').unwrap_or((pair, ""));
            let key = decode_form_component(key_raw);
            let value = decode_form_component(value_raw);
            match key.trim_end_matches("[]") {
                "path" => form.path = value,
                "ides" => push_form_value(&mut form.ides, value),
                "skills" => push_form_value(&mut form.skills, value),
                "mcp_servers" => push_form_value(&mut form.mcp_servers, value),
                "deploy" => form.deploy = Some(value),
                _ => {}
            }
        }
        form
    }
}

fn push_form_value(values: &mut Vec<String>, value: String) {
    let trimmed = value.trim();
    if !trimmed.is_empty() && !values.iter().any(|existing| existing == trimmed) {
        values.push(trimmed.to_string());
    }
}

fn decode_form_component(raw: &str) -> String {
    let plus_decoded = raw.replace('+', " ");
    urlencoding::decode(&plus_decoded)
        .map(|decoded| decoded.into_owned())
        .unwrap_or(plus_decoded)
}

async fn save(State(st): State<AppState>, RawForm(body): RawForm) -> Response {
    let f = SaveForm::from_urlencoded(&body);
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut store = match ProjectStore::load() {
        Ok(s) => s,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    let Some(previous) = store.get(&f.path).cloned() else {
        toast_error(&tx, "project not found");
        return ok();
    };
    if let Some(project) = store.get_mut(&f.path) {
        project.ides = f.ides.clone();
        project.skills = f.skills.clone();
        project.mcp_servers = f.mcp_servers.clone();
    }
    if let Err(e) = store.save() {
        toast_error(&tx, format!("save: {e}"));
        return ok();
    }

    if f.deploy.is_some() {
        match deploy_project_selection(&previous, &f.ides, &f.skills, &f.mcp_servers) {
            Ok((skills_count, mcp_count)) => {
                toast_info(&tx, format!("deployed {skills_count} skill×IDE link(s), touched {mcp_count} MCP file(s)"));
            }
            Err(e) => toast_error(&tx, format!("deploy: {e}")),
        }
    } else {
        toast_info(&tx, "project config saved");
    }
    invalidate(&tx, ResourceKind::Projects);
    invalidate(&tx, ResourceKind::Skills);
    invalidate(&tx, ResourceKind::Mcp);
    ok()
}

#[derive(Deserialize)]
struct PathForm {
    path: String,
}

async fn sync_project(State(st): State<AppState>, Form(f): Form<PathForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let store = match ProjectStore::load() {
        Ok(s) => s,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    let Some(project) = store.get(&f.path).cloned() else {
        toast_error(&tx, "project not found");
        return ok();
    };
    match deploy_project_selection(
        &project,
        &project.ides,
        &project.skills,
        &project.mcp_servers,
    ) {
        Ok((skills_count, mcp_count)) => {
            toast_info(
                &tx,
                format!("synced {skills_count} skill×IDE link(s), touched {mcp_count} MCP file(s)"),
            );
            invalidate(&tx, ResourceKind::Skills);
            invalidate(&tx, ResourceKind::Mcp);
        }
        Err(e) => toast_error(&tx, format!("sync: {e}")),
    }
    ok()
}

async fn remove(State(st): State<AppState>, Form(f): Form<PathForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut store = match ProjectStore::load() {
        Ok(s) => s,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    let project = store.get(&f.path).cloned();
    if let Some(project) = &project {
        let _ = undeploy_project(project);
    }
    if let Err(e) = store.remove(&f.path) {
        toast_error(&tx, format!("{e}"));
        return ok();
    }
    let _ = store.save();
    toast_info(&tx, "removed");
    invalidate(&tx, ResourceKind::Projects);
    invalidate(&tx, ResourceKind::Skills);
    invalidate(&tx, ResourceKind::Mcp);
    ok()
}

fn deploy_project_selection(
    previous: &Project,
    ides: &[String],
    skills: &[String],
    mcp_servers: &[String],
) -> anyhow::Result<(usize, usize)> {
    let project_path = Path::new(&previous.path);
    let mut reg = SkillRegistry::load()?;
    for old_skill in &previous.skills {
        if skills.iter().any(|s| s == old_skill) {
            continue;
        }
        for ide_id in &previous.ides {
            if let Some(mut skill) = reg.get(old_skill).cloned() {
                let _ = install::undeploy(&mut skill, ide_id, Some(project_path));
                reg.upsert(skill);
            }
        }
    }

    let mut skill_links = 0usize;
    for skill_id in skills {
        for ide_id in ides {
            let Some(mut skill) = reg.get(skill_id).cloned() else {
                continue;
            };
            install::deploy(&mut skill, ide_id, Some(project_path))?;
            reg.upsert(skill);
            skill_links += 1;
        }
    }
    reg.save()?;

    let mcp_reg = McpRegistry::load()?;
    let plan = mcp::sync::plan(&mcp_reg, &[], Some(mcp_servers));
    let touched = mcp::sync::execute(&mcp_reg, &plan, Some(project_path), Some(mcp_servers))?;
    Ok((skill_links, touched.len()))
}

fn undeploy_project(project: &Project) -> anyhow::Result<()> {
    let project_path = Path::new(&project.path);
    let mut reg = SkillRegistry::load()?;
    for skill_id in &project.skills {
        for ide_id in &project.ides {
            if let Some(mut skill) = reg.get(skill_id).cloned() {
                let _ = install::undeploy(&mut skill, ide_id, Some(project_path));
                reg.upsert(skill);
            }
        }
    }
    reg.save()?;

    let mcp_reg = McpRegistry::load()?;
    let empty: Vec<String> = Vec::new();
    let plan = mcp::sync::plan(&mcp_reg, &[], Some(empty.as_slice()));
    let _ = mcp::sync::execute(&mcp_reg, &plan, Some(project_path), Some(empty.as_slice()))?;
    Ok(())
}

fn short_id(id: &str) -> &str {
    let tail = if let Some(pos) = id.rfind("__") {
        &id[pos + 2..]
    } else {
        id
    };
    tail.rsplit(|c: char| c == '/' || c == '\\' || c == '_')
        .find(|s| !s.is_empty())
        .unwrap_or(tail)
}

fn ok() -> Response {
    (axum::http::StatusCode::OK, "ok").into_response()
}

#[cfg(test)]
mod tests {
    use super::SaveForm;

    #[test]
    fn save_form_accepts_single_checkbox_values() {
        let form = SaveForm::from_urlencoded(
            b"path=%2Ftmp%2Fdemo&ides=cursor&skills=local__demo&mcp_servers=webui-test&deploy=true",
        );

        assert_eq!(form.path, "/tmp/demo");
        assert_eq!(form.ides, vec!["cursor".to_string()]);
        assert_eq!(form.skills, vec!["local__demo".to_string()]);
        assert_eq!(form.mcp_servers, vec!["webui-test".to_string()]);
        assert_eq!(form.deploy, Some("true".to_string()));
    }

    #[test]
    fn save_form_accepts_repeated_checkbox_values() {
        let form = SaveForm::from_urlencoded(
            b"path=%2Ftmp%2Fdemo&ides=cursor&ides=vscode&skills=a&skills=b&mcp_servers%5B%5D=one&mcp_servers%5B%5D=two",
        );

        assert_eq!(form.ides, vec!["cursor".to_string(), "vscode".to_string()]);
        assert_eq!(form.skills, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(form.mcp_servers, vec!["one".to_string(), "two".to_string()]);
    }
}
