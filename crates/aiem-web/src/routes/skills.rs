//! Skills view — feature parity with the desktop GUI.
//!
//! Features:
//! * Group skills by owner/repo (collapsible)
//! * Filter box
//! * Add form (source · subdir · ref · name)
//! * Clear all global deployments
//! * Per-group batch actions: Deploy All / Undeploy All / Update All / Remove All
//! * Per-skill actions: Deploy / Undeploy, Update, Remove, Link GitHub (local)
//! * IDE + project scope selector (global · registered project paths)

use axum::extract::{Form, Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::projects::ProjectStore;
use aiem_core::skills::model::{Skill, SkillSource};
use aiem_core::skills::{github, install, SkillRegistry};

use crate::events::ResourceKind;
use crate::layout::{btn_danger, btn_primary, btn_secondary, card, empty_state, page, page_header, tag, TagKind};
use crate::state::AppState;
use crate::tasks::{invalidate, task_finished, task_started, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/skills", get(index_page))
        .route("/skills/fragment", get(list_fragment))
        .route("/skills/add", post(add))
        .route("/skills/clear-global", post(clear_global))
        .route("/skills/:id/update", post(update_one))
        .route("/skills/:id/remove", post(remove_one))
        .route("/skills/:id/deploy", post(deploy))
        .route("/skills/:id/undeploy", post(undeploy))
        .route("/skills/:id/link-github", post(link_github))
        .route("/skills/group/:owner/:repo/sync", post(sync_group))
        .route("/skills/group/:owner/:repo/deploy-all", post(group_deploy_all))
        .route("/skills/group/:owner/:repo/undeploy-all", post(group_undeploy_all))
        .route("/skills/group/:owner/:repo/remove-all", post(group_remove_all))
}

#[derive(Deserialize, Default)]
struct ListQuery {
    #[serde(default)]
    q: String,
}

async fn index_page(State(st): State<AppState>, Query(q): Query<ListQuery>) -> Markup {
    let skills = st.skills().ok();
    let projects = ProjectStore::load().ok();
    let body = html! {
        (page_header("Skills", "Install Claude Code / Codex / Copilot skills and deploy them to your IDEs.", html! {
            form hx-post="/skills/clear-global" hx-swap="none"
                 hx-confirm="Remove every symlinked skill from every IDE's global config?" {
                (btn_danger("Clear global"))
            }
            button type="button" class="btn-primary" onclick="document.getElementById('add-skill').toggleAttribute('hidden')" {
                "+ Add skill"
            }
        }))

        div id="add-skill" hidden { (add_form()) }

        form class="aiem-card" style="display:flex;gap:8px;align-items:center;margin-bottom:12px"
             hx-get="/skills/fragment" hx-target="#skills-list" hx-trigger="input changed delay:200ms from:input[name=q], refresh, submit"
             hx-swap="innerHTML" {
            label class="label" style="margin:0;min-width:40px" { "Filter" }
            input name="q" class="field" placeholder="name / id" value=(q.q);
        }

        div id="skills-list" data-resource="skills"
            hx-get="/skills/fragment"
            hx-trigger="refresh from:body"
            hx-swap="innerHTML" {
            (render_list(skills.as_ref(), projects.as_ref(), &q.q))
        }
    };
    page("Skills", "/skills", body)
}

async fn list_fragment(
    State(st): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Markup {
    let skills = st.skills().ok();
    let projects = ProjectStore::load().ok();
    render_list(skills.as_ref(), projects.as_ref(), &q.q)
}

// ─── Rendering ────────────────────────────────────────────────────────────

fn add_form() -> Markup {
    card(html! {
        div class="text-sm font-semibold mb-1" { "Add skill from GitHub" }
        div class="meta mb-3" { "owner/repo · owner/repo//subdir · owner/repo@v1.2 · or a full GitHub URL" }
        form hx-post="/skills/add" hx-swap="none"
             hx-on--after-request="this.reset(); document.getElementById('add-skill').setAttribute('hidden','')"
             class="grid gap-3" style="grid-template-columns:1fr 1fr;" {
            div style="grid-column:1/-1" {
                label class="label" { "Source *" }
                input name="source" required class="field"
                      placeholder="owner/repo or https://github.com/owner/repo";
            }
            div {
                label class="label" { "Subdir (optional)" }
                input name="subdir" class="field" placeholder="path/inside/repo";
            }
            div {
                label class="label" { "Ref (optional)" }
                input name="reference" class="field" placeholder="branch / tag / commit";
            }
            div {
                label class="label" { "Display name (optional)" }
                input name="name" class="field" placeholder="auto";
            }
            div class="flex items-end gap-2" {
                (btn_primary("Download & install"))
                button type="button" class="btn-ghost"
                       onclick="document.getElementById('add-skill').setAttribute('hidden','')" { "Cancel" }
            }
        }
    })
}

fn render_list(reg: Option<&SkillRegistry>, projects: Option<&ProjectStore>, filter: &str) -> Markup {
    let Some(reg) = reg else {
        return html! { div class="aiem-card" style="color:var(--danger)" { "Failed to load skill registry." } };
    };
    let filter = filter.trim().to_ascii_lowercase();
    let mut groups: std::collections::BTreeMap<String, Vec<&Skill>> = Default::default();
    let total = reg.list().count();
    for s in reg.list() {
        let id_lc = s.id.to_ascii_lowercase();
        let name_lc = s.name.to_ascii_lowercase();
        if !filter.is_empty() && !id_lc.contains(&filter) && !name_lc.contains(&filter) {
            continue;
        }
        let key = match &s.source {
            SkillSource::GitHub { owner, repo, .. } => format!("{owner}/{repo}"),
            SkillSource::Local { .. } => "(local)".to_string(),
        };
        groups.entry(key).or_default().push(s);
    }
    if total == 0 {
        return empty_state("No skills installed", "Click \"+ Add skill\" to install one from GitHub.");
    }
    if groups.is_empty() {
        return empty_state("No matches", "Try a different filter.");
    }

    let project_list: Vec<(String, String)> = projects.map(|p| {
        p.list().map(|proj| {
            let label = std::path::Path::new(&proj.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| proj.path.clone());
            (proj.path.clone(), label)
        }).collect()
    }).unwrap_or_default();

    html! {
        @for (group_key, skills) in &groups {
            (render_group(group_key, skills, &project_list))
        }
    }
}

fn render_group(group_key: &str, skills: &[&Skill], projects: &[(String, String)]) -> Markup {
    let display_name = group_key.rsplit('/').next().unwrap_or(group_key);
    let is_github = group_key != "(local)";
    let owner_repo = if is_github { group_key.split_once('/') } else { None };

    html! {
        details class="group-box" open {
            summary {
                span class="chev" { "▸" }
                span { (display_name) }
                @if skills.len() > 1 { span style="color:var(--muted);font-weight:400;margin-left:6px;" { "(" (skills.len()) ")" } }
                span style="flex:1" {}
                @if is_github {
                    span class="meta mono" { (group_key) }
                }
            }

            // Group-level batch actions (only for github groups with >=1 items)
            @if let Some((owner, repo)) = owner_repo {
                div class="group-actions" {
                    (scope_selectors(&format!("grp-{group_key}"), projects))
                    @if skills.len() > 1 {
                        form class="inline" hx-post=(format!("/skills/group/{owner}/{repo}/deploy-all"))
                             hx-swap="none" hx-include=(format!("closest .group-actions")) {
                            (btn_primary("Deploy all"))
                        }
                        form class="inline" hx-post=(format!("/skills/group/{owner}/{repo}/undeploy-all"))
                             hx-swap="none" hx-include=(format!("closest .group-actions")) {
                            (btn_secondary("Undeploy all"))
                        }
                    }
                    form class="inline" hx-post=(format!("/skills/group/{owner}/{repo}/sync")) hx-swap="none" {
                        (btn_secondary("Update all"))
                    }
                    @if skills.len() >= 1 {
                        form class="inline" hx-post=(format!("/skills/group/{owner}/{repo}/remove-all"))
                             hx-swap="none"
                             hx-confirm=(format!("Remove all {} skills from this group? Local files will be deleted.", skills.len())) {
                            (btn_danger("Remove all"))
                        }
                    }
                }
            }

            div class="group-body" {
                @for s in skills { (render_skill_row(s, projects)) }
            }
        }
    }
}

fn scope_selectors(id_prefix: &str, projects: &[(String, String)]) -> Markup {
    let ide_id = format!("{id_prefix}-ide");
    let proj_id = format!("{id_prefix}-project");
    html! {
        label class="label" style="margin:0" for=(ide_id) { "IDE" }
        select id=(ide_id) name="ide" class="field" style="width:auto;min-width:140px" {
            @for i in aiem_core::ide::IDES {
                option value=(i.id) { (i.display_name) }
            }
        }
        label class="label" style="margin:0 0 0 8px" for=(proj_id) { "Scope" }
        select id=(proj_id) name="project" class="field" style="width:auto;min-width:160px" {
            option value="" { "Global" }
            @for (path, label) in projects {
                option value=(path) { (label) }
            }
        }
    }
}

fn render_skill_row(s: &Skill, projects: &[(String, String)]) -> Markup {
    let is_local = matches!(&s.source, SkillSource::Local { .. });
    let short = short_id(&s.id);
    let in_projects: Vec<String> =
        aiem_core::skills::registry::projects_with(&s.id).unwrap_or_default();

    html! {
        div class="skill-card" style="border:1px solid var(--stroke);border-radius:8px;padding:12px;margin-bottom:8px;background:var(--surface);" {
            div style="display:flex;gap:12px;align-items:flex-start;flex-wrap:wrap" {
                div style="flex:1;min-width:240px" {
                    div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap" {
                        span style="font-weight:600;font-size:14px" { (short) }
                        span class="tag" { (short_ver(&s.version)) }
                        @if is_local { (tag("local", TagKind::Neutral)) }
                    }
                    @if let Some(d) = &s.description {
                        div class="meta line-clamp-2" style="margin-top:4px" {
                            (d.lines().next().unwrap_or(""))
                        }
                    }
                    div class="meta mono" style="margin-top:4px;word-break:break-all" { (s.id) }
                    @if !s.deployments.is_empty() {
                        div class="row-gap" style="margin-top:6px" {
                            @for (ide, roots) in &s.deployments {
                                @for r in roots {
                                    @if r == "~" {
                                        (tag(&format!("{ide} · global"), TagKind::Success))
                                    } @else {
                                        (tag(&format!("{ide} · {}", scope_label(r)), TagKind::Success))
                                    }
                                }
                            }
                        }
                    }
                }

                div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap;justify-content:flex-end" {
                    form hx-post=(format!("/skills/{}/deploy", s.id)) hx-swap="none"
                         style="display:flex;gap:6px;align-items:center" {
                        select name="ide" class="field" style="width:auto;min-width:130px" {
                            @for i in aiem_core::ide::IDES {
                                option value=(i.id) { (i.display_name) }
                            }
                        }
                        select name="project" class="field" style="width:auto;min-width:130px" {
                            option value="" { "Global" }
                            @for (path, label) in projects {
                                option value=(path) { (label) }
                            }
                        }
                        (btn_primary("Deploy"))
                    }
                    form hx-post=(format!("/skills/{}/undeploy", s.id)) hx-swap="none"
                         hx-include="previous form"
                         hx-confirm="Undeploy this skill from the selected IDE/scope?"
                         title="Undeploy from the IDE + scope selected in the deploy form" {
                        (btn_secondary("Undeploy"))
                    }
                    @if is_local {
                        form hx-post=(format!("/skills/{}/link-github", s.id)) hx-swap="none"
                             style="display:flex;gap:6px" {
                            input name="source" class="field" placeholder="owner/repo" style="width:180px";
                            (btn_secondary("Link GitHub"))
                        }
                    } @else {
                        form hx-post=(format!("/skills/{}/update", s.id)) hx-swap="none" {
                            (btn_secondary("Update"))
                        }
                    }
                    form hx-post=(format!("/skills/{}/remove", s.id)) hx-swap="none"
                         hx-confirm="Remove this skill and delete local files?" {
                        (btn_danger("Remove"))
                    }
                }
            }

            // Chips row: projects that currently reference this skill.
            @if !in_projects.is_empty() {
                div style="margin-top:8px;display:flex;gap:6px;align-items:center;flex-wrap:wrap" {
                    span class="meta" { "In projects:" }
                    @for n in &in_projects { (tag(n, TagKind::Neutral)) }
                }
            }
        }
    }
}

fn short_id(id: &str) -> &str {
    let tail = if let Some(pos) = id.rfind("__") { &id[pos + 2..] } else { id };
    tail.rsplit(|c: char| c == '/' || c == '\\' || c == '_')
        .find(|s| !s.is_empty())
        .unwrap_or(tail)
}

fn short_ver(v: &str) -> String {
    if v.len() > 12 { v[..12].to_string() } else { v.to_string() }
}

fn scope_label(root: &str) -> String {
    std::path::Path::new(root)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string())
}

// ─── Handlers ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AddForm {
    source: String,
    #[serde(default)] subdir: Option<String>,
    #[serde(default)] reference: Option<String>,
    #[serde(default)] name: Option<String>,
}

async fn add(State(st): State<AppState>, Form(f): Form<AddForm>) -> Response {
    let tx = st.events.clone();
    let id = st.next_task_id().await;
    let mut source = f.source.trim().to_string();
    if source.is_empty() {
        toast_error(&tx, "source is required");
        return (axum::http::StatusCode::BAD_REQUEST, "empty").into_response();
    }
    if let Some(sub) = f.subdir.as_deref() {
        let sub = sub.trim();
        if !sub.is_empty() && !source.contains("//") { source.push_str(&format!("//{sub}")); }
    }
    if let Some(r) = f.reference.as_deref() {
        let r = r.trim();
        if !r.is_empty() && !source.contains('@') { source.push_str(&format!("@{r}")); }
    }
    let name = f.name.filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string());
    tokio::spawn(async move {
        let _guard = st.write_lock.lock().await;
        task_started(&tx, id, format!("add {source}"));
        let parsed = match SkillSource::parse_github(&source) {
            Some(s) => s,
            None => { task_finished(&tx, id, false, format!("invalid source: {source}")); return; }
        };
        let (owner, repo, reff, subdir) = match parsed {
            SkillSource::GitHub { owner, repo, r#ref, subdir } => (owner, repo, r#ref, subdir),
            _ => { task_finished(&tx, id, false, "only github sources supported"); return; }
        };
        match github::fetch_github_auto(&owner, &repo, reff.as_deref(), subdir.as_deref(), name.as_deref()).await {
            Ok(result) => {
                let mut reg = match SkillRegistry::load() {
                    Ok(r) => r,
                    Err(e) => { task_finished(&tx, id, false, format!("registry: {e}")); return; }
                };
                let n = result.skills.len();
                for s in result.skills { reg.upsert(s); }
                if let Err(e) = reg.save() { task_finished(&tx, id, false, format!("save: {e}")); return; }
                if !result.mcp_servers.is_empty() {
                    if let Ok(mut m) = aiem_core::mcp::McpRegistry::load() {
                        for s in result.mcp_servers { m.upsert(s); }
                        let _ = m.save();
                        invalidate(&tx, ResourceKind::Mcp);
                    }
                }
                task_finished(&tx, id, true, format!("added {n} skill(s) from {owner}/{repo}"));
                invalidate(&tx, ResourceKind::Skills);
            }
            Err(e) => task_finished(&tx, id, false, format!("fetch: {e}")),
        }
    });
    (axum::http::StatusCode::ACCEPTED, "ok").into_response()
}

async fn clear_global(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => { toast_error(&tx, format!("{e}")); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response(); }
    };
    match install::undeploy_all_global(&mut reg) {
        Ok(n) => {
            let _ = reg.save();
            toast_info(&tx, format!("cleared {n} global deployment(s)"));
            invalidate(&tx, ResourceKind::Skills);
        }
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}

async fn update_one(State(st): State<AppState>, Path(id): Path<String>) -> Response {
    let tx = st.events.clone();
    let task_id = st.next_task_id().await;
    tokio::spawn(async move {
        let _guard = st.write_lock.lock().await;
        task_started(&tx, task_id, format!("update {id}"));
        let reg = match SkillRegistry::load() { Ok(r) => r, Err(e) => { task_finished(&tx, task_id, false, format!("{e}")); return; } };
        let existing = match reg.get(&id).cloned() {
            Some(s) => s,
            None => { task_finished(&tx, task_id, false, format!("{id} not found")); return; }
        };
        let SkillSource::GitHub { owner, repo, subdir, .. } = existing.source.clone() else {
            task_finished(&tx, task_id, false, "not a github skill"); return;
        };
        match github::fetch_github_to_temp(&owner, &repo, None, subdir.as_deref()).await {
            Ok((temp_dir, new_version, actual_subdir)) => {
                let target = existing.path.clone();
                let skipped = crate::fs_merge::smart_merge(temp_dir.path(), &target, &existing.file_hashes);
                let mut skill = existing.clone();
                skill.version = new_version;
                skill.file_hashes = crate::fs_merge::hash_files(&target);
                if let SkillSource::GitHub { r#ref: ref mut stored_ref, subdir: ref mut stored_subdir, .. } = skill.source {
                    *stored_ref = None;
                    if let Some(new_sub) = actual_subdir { *stored_subdir = Some(new_sub); }
                }
                let mut reg = match SkillRegistry::load() { Ok(r) => r, Err(e) => { task_finished(&tx, task_id, false, format!("{e}")); return; } };
                reg.upsert(skill);
                if let Err(e) = reg.save() { task_finished(&tx, task_id, false, format!("save: {e}")); return; }
                let msg = if skipped.is_empty() {
                    format!("updated {id}")
                } else {
                    format!("updated {id} (skipped {} locally modified: {})", skipped.len(), skipped.join(", "))
                };
                task_finished(&tx, task_id, true, msg);
                invalidate(&tx, ResourceKind::Skills);
            }
            Err(e) => task_finished(&tx, task_id, false, format!("fetch: {e}")),
        }
    });
    (axum::http::StatusCode::ACCEPTED, "ok").into_response()
}

async fn remove_one(State(st): State<AppState>, Path(id): Path<String>) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => { toast_error(&tx, format!("{e}")); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response(); }
    };
    match install::remove_skill(&mut reg, &id) {
        Ok(()) => {
            let _ = reg.save();
            toast_info(&tx, format!("removed {id}"));
            invalidate(&tx, ResourceKind::Skills);
        }
        Err(e) => toast_error(&tx, format!("remove: {e}")),
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}

async fn sync_group(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
) -> Response {
    let tx = st.events.clone();
    let task_id = st.next_task_id().await;
    tokio::spawn(async move {
        let _guard = st.write_lock.lock().await;
        task_started(&tx, task_id, format!("sync {owner}/{repo}"));
        let reg = match SkillRegistry::load() { Ok(r) => r, Err(e) => { task_finished(&tx, task_id, false, format!("{e}")); return; } };
        let skills: Vec<Skill> = reg.list().filter(|s| {
            matches!(&s.source, SkillSource::GitHub { owner: o, repo: r, .. } if *o == owner && *r == repo)
        }).cloned().collect();
        match github::sync_github_group(&owner, &repo, &skills).await {
            Ok(r) => {
                let msg = if r.added.is_empty() {
                    format!("synced {owner}/{repo}: {} updated, no new skills", r.updated.len())
                } else {
                    let names: Vec<_> = r.added.iter().map(|s| s.name.as_str()).collect();
                    format!("synced {owner}/{repo}: {} updated, {} new: {}",
                        r.updated.len(), r.added.len(), names.join(", "))
                };
                task_finished(&tx, task_id, true, msg);
                invalidate(&tx, ResourceKind::Skills);
            }
            Err(e) => task_finished(&tx, task_id, false, format!("sync: {e}")),
        }
    });
    (axum::http::StatusCode::ACCEPTED, "ok").into_response()
}

#[derive(Deserialize)]
struct DeployForm {
    ide: String,
    #[serde(default)] project: Option<String>,
}

fn project_path(raw: Option<&str>) -> Option<std::path::PathBuf> {
    raw.map(|s| s.trim()).filter(|s| !s.is_empty()).map(std::path::PathBuf::from)
}

async fn deploy(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Form(f): Form<DeployForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,"err").into_response(); } };
    let mut skill = match reg.get(&id).cloned() {
        Some(s) => s,
        None => { toast_error(&tx, format!("{id} not found")); return (axum::http::StatusCode::NOT_FOUND,"nf").into_response(); }
    };
    let project = project_path(f.project.as_deref());
    match install::deploy(&mut skill, &f.ide, project.as_deref()) {
        Ok((link, _)) => {
            reg.upsert(skill);
            let _ = reg.save();
            toast_info(&tx, format!("deployed → {}", link.display()));
            invalidate(&tx, ResourceKind::Skills);
        }
        Err(e) => toast_error(&tx, format!("deploy: {e}")),
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}

async fn undeploy(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Form(f): Form<DeployForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,"err").into_response(); } };
    let mut skill = match reg.get(&id).cloned() {
        Some(s) => s,
        None => { toast_error(&tx, format!("{id} not found")); return (axum::http::StatusCode::NOT_FOUND,"nf").into_response(); }
    };
    let project = project_path(f.project.as_deref());
    match install::undeploy(&mut skill, &f.ide, project.as_deref()) {
        Ok(_) => {
            reg.upsert(skill);
            let _ = reg.save();
            toast_info(&tx, format!("undeployed {id} from {}", f.ide));
            invalidate(&tx, ResourceKind::Skills);
        }
        Err(e) => toast_error(&tx, format!("undeploy: {e}")),
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}

#[derive(Deserialize)]
struct LinkGithubForm { source: String }

async fn link_github(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Form(f): Form<LinkGithubForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let new_source = match SkillSource::parse_github(&f.source) {
        Some(s) => s,
        None => { toast_error(&tx, "invalid GitHub source (use owner/repo)"); return (axum::http::StatusCode::BAD_REQUEST, "bad").into_response(); }
    };
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => { toast_error(&tx, format!("{e}")); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,"err").into_response(); }
    };
    let Some(mut skill) = reg.get(&id).cloned() else {
        toast_error(&tx, format!("{id} not found"));
        return (axum::http::StatusCode::NOT_FOUND, "nf").into_response();
    };
    skill.source = new_source;
    reg.upsert(skill);
    if let Err(e) = reg.save() {
        toast_error(&tx, format!("save: {e}"));
    } else {
        toast_info(&tx, format!("linked {id} to GitHub"));
        invalidate(&tx, ResourceKind::Skills);
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}

// ─── Group batch actions ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct GroupScopeForm {
    ide: String,
    #[serde(default)] project: Option<String>,
}

async fn group_deploy_all(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    Form(f): Form<GroupScopeForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,"err").into_response(); } };
    let ids: Vec<String> = reg.list().filter_map(|s| {
        if let SkillSource::GitHub { owner: o, repo: r, .. } = &s.source {
            if *o == owner && *r == repo { return Some(s.id.clone()); }
        }
        None
    }).collect();
    let project = project_path(f.project.as_deref());
    let mut ok = 0usize;
    let mut last_err: Option<String> = None;
    for id in &ids {
        if let Some(mut skill) = reg.get(id).cloned() {
            match install::deploy(&mut skill, &f.ide, project.as_deref()) {
                Ok(_) => { reg.upsert(skill); ok += 1; }
                Err(e) => last_err = Some(format!("{}: {e}", skill.name)),
            }
        }
    }
    let _ = reg.save();
    if ok > 0 {
        toast_info(&tx, format!("deployed {ok}/{} to {}", ids.len(), f.ide));
        invalidate(&tx, ResourceKind::Skills);
    }
    if let Some(e) = last_err { toast_error(&tx, e); }
    (axum::http::StatusCode::OK, "ok").into_response()
}

async fn group_undeploy_all(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    Form(f): Form<GroupScopeForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,"err").into_response(); } };
    let ids: Vec<String> = reg.list().filter_map(|s| {
        if let SkillSource::GitHub { owner: o, repo: r, .. } = &s.source {
            if *o == owner && *r == repo { return Some(s.id.clone()); }
        }
        None
    }).collect();
    let project = project_path(f.project.as_deref());
    let mut ok = 0usize;
    for id in &ids {
        if let Some(mut skill) = reg.get(id).cloned() {
            if skill.deployments.contains_key(f.ide.as_str()) {
                if install::undeploy(&mut skill, &f.ide, project.as_deref()).is_ok() {
                    reg.upsert(skill);
                    ok += 1;
                }
            }
        }
    }
    let _ = reg.save();
    if ok > 0 {
        toast_info(&tx, format!("undeployed {ok} from {}", f.ide));
        invalidate(&tx, ResourceKind::Skills);
    } else {
        toast_info(&tx, "nothing to undeploy");
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}

async fn group_remove_all(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,"err").into_response(); } };
    let ids: Vec<String> = reg.list().filter_map(|s| {
        if let SkillSource::GitHub { owner: o, repo: r, .. } = &s.source {
            if *o == owner && *r == repo { return Some(s.id.clone()); }
        }
        None
    }).collect();
    let mut ok = 0usize;
    for id in &ids {
        if install::remove_skill(&mut reg, id).is_ok() { ok += 1; }
    }
    let _ = reg.save();
    if ok > 0 {
        toast_info(&tx, format!("removed {ok} skill(s) from {owner}/{repo}"));
        invalidate(&tx, ResourceKind::Skills);
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}
