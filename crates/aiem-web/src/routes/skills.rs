//! Skills view — compact resource list with inline detail expand.

use axum::extract::{Form, Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::projects::ProjectStore;
use aiem_core::skills::model::{Skill, SkillSource};
use aiem_core::skills::{
    apply_github_proxy_env, deploy as skill_deploy, github, install, SkillRegistry,
};

use crate::events::ResourceKind;
use crate::layout::{
    btn_danger, btn_primary, btn_secondary, empty_state, page, page_header, tag, TagKind,
};
use crate::state::AppState;
use crate::tasks::{invalidate, task_finished, task_started, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/skills", get(index_page))
        .route("/skills/fragment", get(list_fragment))
        .route("/skills/add", post(add))
        .route("/skills/create", post(create_skill))
        .route("/skills/clear-global", post(clear_global))
        .route("/skills/view", get(view_skill))
        .route("/skills/files", get(view_skill_files))
        .route("/skills/:id/update", post(update_one))
        .route("/skills/:id/remove", post(remove_one))
        .route("/skills/:id/deploy", post(deploy))
        .route("/skills/:id/undeploy", post(undeploy))
        .route("/skills/:id/link-github", post(link_github))
        .route("/skills/group/:owner/:repo/sync", post(sync_group))
        .route(
            "/skills/group/:owner/:repo/deploy-all",
            post(group_deploy_all),
        )
        .route(
            "/skills/group/:owner/:repo/undeploy-all",
            post(group_undeploy_all),
        )
        .route(
            "/skills/group/:owner/:repo/remove-all",
            post(group_remove_all),
        )
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
        (page_header("Skills", "", html! {
            form hx-post="/skills/clear-global" hx-swap="none"
                 hx-confirm="Remove every symlinked skill from every IDE's global config?" {
                (btn_danger("Clear global"))
            }
            button type="button" class="btn-secondary"
                   onclick="document.getElementById('create-skill').toggleAttribute('hidden')" {
                "New local"
            }
            button type="button" class="btn-primary"
                   onclick="document.getElementById('add-skill').toggleAttribute('hidden')" {
                "Add from GitHub"
            }
        }))

        div class="content-padding skills-content" {
            div id="add-skill" hidden { (add_form()) }
            div id="create-skill" hidden { (create_form()) }

            // Filter bar
            form style="display:flex;gap:8px;align-items:center;margin-bottom:16px"
                 hx-get="/skills/fragment" hx-target="#skills-list"
                 hx-trigger="input changed delay:200ms from:input[name=q], refresh, submit"
                 hx-swap="innerHTML" {
                input name="q" class="field" placeholder="Filter skills\u{2026}" value=(q.q)
                      style="max-width:320px";
            }

            div id="skills-list" data-resource="skills"
                hx-get="/skills/fragment"
                hx-trigger="refresh from:body"
                hx-swap="innerHTML" {
                (render_list(skills.as_ref(), projects.as_ref(), &q.q))
            }
        }
    };
    page("Skills", "/skills", body)
}

async fn list_fragment(State(st): State<AppState>, Query(q): Query<ListQuery>) -> Markup {
    let skills = st.skills().ok();
    let projects = ProjectStore::load().ok();
    render_list(skills.as_ref(), projects.as_ref(), &q.q)
}

// ─── Rendering ────────────────────────────────────────────────────────────

fn add_form() -> Markup {
    html! {
        div class="group-panel" style="margin-bottom:16px" {
            div style="padding:16px" {
                div style="font-size:14px;font-weight:600;margin-bottom:4px" { "Add skill from GitHub" }
                div class="meta" style="margin-bottom:12px" { "owner/repo · owner/repo//subdir · owner/repo@v1.2 · or a full GitHub URL" }
                form hx-post="/skills/add" hx-swap="none"
                     hx-on--after-request="this.reset();document.getElementById('add-skill').setAttribute('hidden','')"
                     class="grid gap-3" style="grid-template-columns:1fr 1fr" {
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
            }
        }
    }
}

fn create_form() -> Markup {
    html! {
        div class="group-panel" style="margin-bottom:16px" {
            div style="padding:16px" {
                div style="font-size:14px;font-weight:600;margin-bottom:4px" { "Create a new local skill" }
                div class="meta" style="margin-bottom:12px" { "Write your own SKILL.md content to create a skill." }
                form hx-post="/skills/create" hx-swap="none"
                     hx-on--after-request="this.reset();document.getElementById('create-skill').setAttribute('hidden','')"
                     class="grid gap-3" {
                    div {
                        label class="label" { "Skill name *" }
                        input name="name" required class="field" placeholder="my-awesome-skill";
                    }
                    div {
                        label class="label" { "SKILL.md content *" }
                        textarea name="content" required class="field" rows="10"
                                 placeholder="# My Skill\n\nDescribe what this skill does." {}
                    }
                    div class="flex items-end gap-2" {
                        (btn_primary("Create skill"))
                        button type="button" class="btn-ghost"
                               onclick="document.getElementById('create-skill').setAttribute('hidden','')" { "Cancel" }
                    }
                }
            }
        }
    }
}

fn render_list(
    reg: Option<&SkillRegistry>,
    projects: Option<&ProjectStore>,
    filter: &str,
) -> Markup {
    let Some(reg) = reg else {
        return html! { div style="color:var(--danger);padding:16px" { "Failed to load skill registry." } };
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
        return empty_state(
            "No skills installed",
            "Click \"Add from GitHub\" to install one.",
        );
    }
    if groups.is_empty() {
        return empty_state("No matches", "Try a different filter.");
    }

    let project_list: Vec<(String, String)> = projects
        .map(|p| {
            p.list()
                .map(|proj| {
                    let label = std::path::Path::new(&proj.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| proj.path.clone());
                    (proj.path.clone(), label)
                })
                .collect()
        })
        .unwrap_or_default();

    html! {
        @for (group_key, skills) in &groups {
            (render_group(group_key, skills, &project_list))
        }
    }
}

fn render_group(group_key: &str, skills: &[&Skill], projects: &[(String, String)]) -> Markup {
    let is_github = group_key != "(local)";
    let owner_repo = if is_github {
        group_key.split_once('/')
    } else {
        None
    };

    html! {
        details class="group-box" open {
            summary {
                span class="chev" { "\u{25b8}" }
                span { (group_key) }
                span class="meta" style="margin-left:6px;font-weight:400" { "(" (skills.len()) ")" }
            }

            @if let Some((owner, repo)) = owner_repo {
                div class="group-actions" {
                    (scope_selectors(&format!("grp-{group_key}"), projects))
                    @if skills.len() > 1 {
                        form style="display:inline" hx-post=(format!("/skills/group/{owner}/{repo}/deploy-all"))
                             hx-swap="none" hx-include="closest .group-actions" {
                            (btn_primary("Deploy all"))
                        }
                        form style="display:inline" hx-post=(format!("/skills/group/{owner}/{repo}/undeploy-all"))
                             hx-swap="none" hx-include="closest .group-actions" {
                            (btn_secondary("Undeploy all"))
                        }
                    }
                    form style="display:inline" hx-post=(format!("/skills/group/{owner}/{repo}/sync")) hx-swap="none" {
                        (btn_secondary("Update all"))
                    }
                    form style="display:inline" hx-post=(format!("/skills/group/{owner}/{repo}/remove-all"))
                         hx-swap="none"
                         hx-confirm=(format!("Remove all {} skills from this group?", skills.len())) {
                        (btn_danger("Remove all"))
                    }
                }
            }

            div class="group-body" {
                table class="aiem" {
                    thead { tr {
                        th { "Name" }
                        th { "Version" }
                        th { "Deployed" }
                        th style="text-align:right" { "Actions" }
                    }}
                    tbody {
                        @for s in skills {
                            (render_skill_row(s, projects))
                        }
                    }
                }
            }
        }
    }
}

fn scope_selectors(id_prefix: &str, projects: &[(String, String)]) -> Markup {
    let ide_id = format!("{id_prefix}-ide");
    let proj_id = format!("{id_prefix}-project");
    html! {
        select id=(ide_id) name="ide" class="field" style="width:auto;min-width:120px" {
            @for i in aiem_core::ide::IDES {
                option value=(i.id) { (i.display_name) }
            }
        }
        select id=(proj_id) name="project" class="field" style="width:auto;min-width:120px" {
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
    let deploy_count: usize = s.deployments.values().map(|v| v.len()).sum();
    let row_id = s.id.replace(|c: char| !c.is_alphanumeric(), "-");

    html! {
        tr {
            td {
                div style="display:flex;align-items:center;gap:6px" {
                    span style="font-weight:500" { (short) }
                    @if is_local { (tag("local", TagKind::Neutral)) }
                }
                @if let Some(d) = &s.description {
                    div class="meta" style="margin-top:2px;max-width:300px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" {
                        (d.lines().next().unwrap_or(""))
                    }
                }
            }
            td { span class="tag tag-neutral mono" { (short_ver(&s.version)) } }
            td {
                @if deploy_count > 0 {
                    span class="tag tag-success" { (deploy_count) " target(s)" }
                } @else {
                    span class="meta" { "\u{2014}" }
                }
            }
            td style="text-align:right;white-space:nowrap" {
                div class="skill-row-actions" {
                    // Deploy inline
                    form hx-post=(format!("/skills/{}/deploy", s.id)) hx-swap="none"
                         style="display:inline-flex;gap:4px;align-items:center" {
                        select name="ide" class="field" style="width:auto;min-width:100px;font-size:11px;padding:3px 6px;min-height:24px" {
                            @for i in aiem_core::ide::IDES {
                                option value=(i.id) { (i.display_name) }
                            }
                        }
                        select name="project" class="field" style="width:auto;min-width:90px;font-size:11px;padding:3px 6px;min-height:24px" {
                            option value="" { "Global" }
                            @for (path, label) in projects {
                                option value=(path) { (label) }
                            }
                        }
                        button type="submit" class="btn-primary" style="min-height:24px;padding:2px 8px;font-size:11px" { "Deploy" }
                    }
                    @if !is_local {
                        form hx-post=(format!("/skills/{}/update", s.id)) hx-swap="none" style="display:inline" {
                            button type="submit" class="btn-ghost" { "Update" }
                        }
                    }
                    // Expand detail
                    button type="button" class="btn-ghost"
                           onclick=(format!("document.getElementById('detail-{row_id}').toggleAttribute('hidden')")) { "More" }
                }
            }
        }
        // Expandable detail row
        tr id=(format!("detail-{row_id}")) hidden {
            td colspan="4" style="padding:12px 16px;background:var(--surface-alt);border-bottom:1px solid var(--stroke-light)" {
                div class="detail-split" {
                    div class="detail-stack skills-detail-stack" {
                        div class="label" { "ID" }
                        div class="mono meta" style="word-break:break-all" { (s.id) }

                        div class="deployment-records-panel" {
                            div class="label" { "Deployment records" }
                            (deployment_records_table(&skill_deployment_records(s)))
                        }

                        div class=(if is_local { "detail-action-row" } else { "detail-action-row detail-action-row-linked" }) {
                            form hx-post=(format!("/skills/{}/undeploy", s.id)) hx-swap="none"
                                 hx-confirm="Undeploy this skill?" class="skill-undeploy-form" {
                                select name="ide" class="field" style="width:auto;min-width:100px;font-size:11px;padding:3px 6px;min-height:24px" {
                                    @for i in aiem_core::ide::IDES { option value=(i.id) { (i.display_name) } }
                                }
                                select name="project" class="field" style="width:auto;min-width:90px;font-size:11px;padding:3px 6px;min-height:24px" {
                                    option value="" { "Global" }
                                    @for (path, label) in projects { option value=(path) { (label) } }
                                }
                                (btn_secondary("Undeploy"))
                            }
                            @if is_local {
                                form hx-post=(format!("/skills/{}/link-github", s.id)) hx-swap="none" {
                                    input name="source" class="field" placeholder="owner/repo"
                                          style="width:160px;font-size:11px;padding:3px 6px;min-height:24px";
                                    (btn_secondary("Link GitHub"))
                                }
                            }
                            a href=(format!("/skills/files?id={}", urlencoding::encode(&s.id)))
                              class="btn-ghost" target="_blank" { "View all files" }
                            form hx-post=(format!("/skills/{}/remove", s.id)) hx-swap="none"
                                 hx-confirm="Remove this skill and delete local files?" style="display:inline" {
                                (btn_danger("Remove"))
                            }
                        }
                    }
                    div {
                        div class="label" { "SKILL.md" }
                        div class="skill-md-slot"
                             hx-get=(format!("/skills/view?id={}", urlencoding::encode(&s.id)))
                             hx-trigger="intersect once" hx-swap="innerHTML" {
                            div class="skill-md-preview" {
                                span class="meta" { "Loading\u{2026}" }
                            }
                        }
                    }
                }
            }
        }
    }
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

fn short_ver(v: &str) -> String {
    v.chars().take(12).collect()
}

fn scope_label(root: &str) -> String {
    std::path::Path::new(root)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string())
}

fn ide_label(ide_id: &str) -> String {
    aiem_core::ide::find(ide_id)
        .map(|i| i.display_name.to_string())
        .unwrap_or_else(|| ide_id.to_string())
}

fn skill_deployment_records(skill: &Skill) -> Vec<(String, String, String, TagKind)> {
    let mut rows = Vec::new();
    for (ide_id, roots) in &skill.deployments {
        for root in roots {
            let project = if root == "~" {
                "Global".to_string()
            } else {
                scope_label(root)
            };
            rows.push((
                project,
                ide_label(ide_id),
                "Deployed".to_string(),
                TagKind::Success,
            ));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    rows
}

fn deployment_records_table(rows: &[(String, String, String, TagKind)]) -> Markup {
    html! {
        @if rows.is_empty() {
            div class="deploy-records deploy-records-empty" {
                span class="meta" { "No deployment records." }
            }
        } @else {
            div class="deploy-records" {
                table {
                    thead { tr {
                        th { "Deploy project" }
                        th { "Target IDE" }
                        th { "Deployment status" }
                    }}
                    tbody {
                        @for (project, ide, status, kind) in rows {
                            tr {
                                td { (project) }
                                td { (ide) }
                                td { (tag(status, *kind)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── Handlers (unchanged) ────────────────────────────────────────────────

#[derive(Deserialize)]
struct AddForm {
    source: String,
    #[serde(default)]
    subdir: Option<String>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    name: Option<String>,
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
        if !sub.is_empty() && !source.contains("//") {
            source.push_str(&format!("//{sub}"));
        }
    }
    if let Some(r) = f.reference.as_deref() {
        let r = r.trim();
        if !r.is_empty() && !source.contains('@') {
            source.push_str(&format!("@{r}"));
        }
    }
    let name = f
        .name
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());
    tokio::spawn(async move {
        let _guard = st.write_lock.lock().await;
        task_started(&tx, id, format!("add {source}"));
        let normalized = apply_github_proxy_env(&source);
        let parsed = match SkillSource::parse_github(normalized) {
            Some(s) => s,
            None => {
                task_finished(&tx, id, false, format!("invalid source: {source}"));
                return;
            }
        };
        let (owner, repo, reff, subdir) = match parsed {
            SkillSource::GitHub {
                owner,
                repo,
                r#ref,
                subdir,
            } => (owner, repo, r#ref, subdir),
            _ => {
                task_finished(&tx, id, false, "only github sources supported");
                return;
            }
        };
        match github::fetch_github_auto(
            &owner,
            &repo,
            reff.as_deref(),
            subdir.as_deref(),
            name.as_deref(),
        )
        .await
        {
            Ok(result) => {
                let mut reg = match SkillRegistry::load() {
                    Ok(r) => r,
                    Err(e) => {
                        task_finished(&tx, id, false, format!("registry: {e}"));
                        return;
                    }
                };
                let n = result.skills.len();
                for s in result.skills {
                    reg.upsert(s);
                }
                if let Err(e) = reg.save() {
                    task_finished(&tx, id, false, format!("save: {e}"));
                    return;
                }
                if !result.mcp_servers.is_empty() {
                    if let Ok(mut m) = aiem_core::mcp::McpRegistry::load() {
                        for s in result.mcp_servers {
                            m.upsert(s);
                        }
                        let _ = m.save();
                        invalidate(&tx, ResourceKind::Mcp);
                    }
                }
                task_finished(
                    &tx,
                    id,
                    true,
                    format!("added {n} skill(s) from {owner}/{repo}"),
                );
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
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response();
        }
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
        let reg = match SkillRegistry::load() {
            Ok(r) => r,
            Err(e) => {
                task_finished(&tx, task_id, false, format!("{e}"));
                return;
            }
        };
        let existing = match reg.get(&id).cloned() {
            Some(s) => s,
            None => {
                task_finished(&tx, task_id, false, format!("{id} not found"));
                return;
            }
        };
        let SkillSource::GitHub {
            owner,
            repo,
            subdir,
            ..
        } = existing.source.clone()
        else {
            task_finished(&tx, task_id, false, "not a github skill");
            return;
        };
        toast_info(&tx, format!("fetching {owner}/{repo}\u{2026}"));
        match github::fetch_github_to_temp(&owner, &repo, None, subdir.as_deref()).await {
            Ok((temp_dir, new_version, actual_subdir)) => {
                if new_version == existing.version {
                    task_finished(&tx, task_id, true, format!("{id} already up to date"));
                    return;
                }
                let target = existing.path.clone();
                let skipped =
                    crate::fs_merge::smart_merge(temp_dir.path(), &target, &existing.file_hashes);
                let mut skill = existing.clone();
                skill.version = new_version;
                skill.file_hashes = crate::fs_merge::hash_files(&target);
                if let SkillSource::GitHub {
                    r#ref: ref mut stored_ref,
                    subdir: ref mut stored_subdir,
                    ..
                } = skill.source
                {
                    *stored_ref = None;
                    if let Some(new_sub) = actual_subdir {
                        *stored_subdir = Some(new_sub);
                    }
                }
                let mut reg = match SkillRegistry::load() {
                    Ok(r) => r,
                    Err(e) => {
                        task_finished(&tx, task_id, false, format!("{e}"));
                        return;
                    }
                };
                reg.upsert(skill);
                if let Err(e) = reg.save() {
                    task_finished(&tx, task_id, false, format!("save: {e}"));
                    return;
                }
                let msg = if skipped.is_empty() {
                    format!("updated {id}")
                } else {
                    format!(
                        "updated {id} (skipped {} locally modified: {})",
                        skipped.len(),
                        skipped.join(", ")
                    )
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
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response();
        }
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
        let reg = match SkillRegistry::load() {
            Ok(r) => r,
            Err(e) => {
                task_finished(&tx, task_id, false, format!("{e}"));
                return;
            }
        };
        let skills: Vec<Skill> = reg.list().filter(|s| {
            matches!(&s.source, SkillSource::GitHub { owner: o, repo: r, .. } if *o == owner && *r == repo)
        }).cloned().collect();
        match github::sync_github_group(&owner, &repo, &skills).await {
            Ok(r) => {
                let msg = if r.added.is_empty() {
                    format!("synced {owner}/{repo}: {} updated", r.updated.len())
                } else {
                    format!(
                        "synced {owner}/{repo}: {} updated, {} new",
                        r.updated.len(),
                        r.added.len()
                    )
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
    #[serde(default)]
    project: Option<String>,
}

fn project_path(raw: Option<&str>) -> Option<std::path::PathBuf> {
    raw.map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
}

async fn deploy(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Form(f): Form<DeployForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let project = project_path(f.project.as_deref());
    if let Some(project) = project {
        match skill_deploy::deploy_to_project(&id, &f.ide, &project) {
            Ok(link) => {
                toast_info(&tx, format!("deployed \u{2192} {}", link.display()));
                invalidate(&tx, ResourceKind::Skills);
                invalidate(&tx, ResourceKind::Projects);
            }
            Err(e) => toast_error(&tx, format!("deploy: {e}")),
        }
        return (axum::http::StatusCode::OK, "ok").into_response();
    }
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response();
        }
    };
    let mut skill = match reg.get(&id).cloned() {
        Some(s) => s,
        None => {
            toast_error(&tx, format!("{id} not found"));
            return (axum::http::StatusCode::NOT_FOUND, "nf").into_response();
        }
    };
    match install::deploy(&mut skill, &f.ide, None) {
        Ok((link, _)) => {
            reg.upsert(skill);
            let _ = reg.save();
            toast_info(&tx, format!("deployed \u{2192} {}", link.display()));
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
    let project = project_path(f.project.as_deref());
    if let Some(project) = project {
        match skill_deploy::undeploy_from_project(&id, &f.ide, &project) {
            Ok(()) => {
                toast_info(&tx, format!("undeployed {id} from project"));
                invalidate(&tx, ResourceKind::Skills);
                invalidate(&tx, ResourceKind::Projects);
            }
            Err(e) => toast_error(&tx, format!("undeploy: {e}")),
        }
        return (axum::http::StatusCode::OK, "ok").into_response();
    }
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response();
        }
    };
    let mut skill = match reg.get(&id).cloned() {
        Some(s) => s,
        None => {
            toast_error(&tx, format!("{id} not found"));
            return (axum::http::StatusCode::NOT_FOUND, "nf").into_response();
        }
    };
    match install::undeploy(&mut skill, &f.ide, None) {
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
struct LinkGithubForm {
    source: String,
}

async fn link_github(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Form(f): Form<LinkGithubForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let normalized = apply_github_proxy_env(&f.source);
    let new_source = match SkillSource::parse_github(normalized) {
        Some(s) => s,
        None => {
            toast_error(&tx, "invalid GitHub source (use owner/repo)");
            return (axum::http::StatusCode::BAD_REQUEST, "bad").into_response();
        }
    };
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response();
        }
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

// ─── View / Create handlers ──────────────────────────────────────────────

#[derive(Deserialize)]
struct ViewQuery {
    id: String,
}

async fn view_skill(Query(q): Query<ViewQuery>) -> Markup {
    let id = q.id;
    match aiem_core::skills::read_skill_content(&id) {
        Ok(content) => {
            html! {
                pre class="skill-md-preview" {
                    (content)
                }
            }
        }
        Err(e) => html! {
            div style="color:var(--danger);font-size:13px" { "Failed to read: " (e) }
        },
    }
}

async fn view_skill_files(Query(q): Query<ViewQuery>) -> Markup {
    let id = q.id;
    let files = aiem_core::skills::list_skill_files(&id).unwrap_or_default();
    let reg = aiem_core::skills::SkillRegistry::load().ok();
    let skill = reg.as_ref().and_then(|r| r.get(&id));
    html! {
        div style="max-width:900px;margin:0 auto;padding:24px" {
            h2 style="font-size:18px;font-weight:600;margin-bottom:4px" { "Files: " (id) }
            @if let Some(s) = skill {
                p class="meta" style="margin-bottom:16px" { (s.path.display()) }
            }
            @if files.is_empty() {
                p class="meta" { "No files found." }
            } @else {
                table class="aiem" {
                    thead { tr { th { "File" } th style="text-align:right" { "Size" } } }
                    tbody {
                        @for (path, size) in &files {
                            tr {
                                td class="mono" { (path) }
                                td style="text-align:right;white-space:nowrap" class="meta" { (format_size(*size)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[derive(Deserialize)]
struct CreateForm {
    name: String,
    content: String,
}

async fn create_skill(State(st): State<AppState>, Form(f): Form<CreateForm>) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let name = f.name.trim().to_string();
    let content = f.content.clone();
    if name.is_empty() || content.trim().is_empty() {
        toast_error(&tx, "name and content are required");
        return (axum::http::StatusCode::BAD_REQUEST, "empty").into_response();
    }
    match aiem_core::skills::create_local_skill(&name, &content) {
        Ok(skill) => {
            toast_info(&tx, format!("created skill: {}", skill.name));
            invalidate(&tx, ResourceKind::Skills);
        }
        Err(e) => toast_error(&tx, format!("create: {e}")),
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}

// ─── Group batch actions ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct GroupScopeForm {
    ide: String,
    #[serde(default)]
    project: Option<String>,
}

async fn group_deploy_all(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    Form(f): Form<GroupScopeForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response();
        }
    };
    let ids: Vec<String> = reg
        .list()
        .filter_map(|s| {
            if let SkillSource::GitHub {
                owner: o, repo: r, ..
            } = &s.source
            {
                if *o == owner && *r == repo {
                    return Some(s.id.clone());
                }
            }
            None
        })
        .collect();
    let project = project_path(f.project.as_deref());
    let mut ok = 0usize;
    let mut last_err: Option<String> = None;
    if let Some(project) = project {
        for id in &ids {
            match skill_deploy::deploy_to_project(id, &f.ide, &project) {
                Ok(_) => ok += 1,
                Err(e) => last_err = Some(format!("{id}: {e}")),
            }
        }
    } else {
        for id in &ids {
            if let Some(mut skill) = reg.get(id).cloned() {
                match install::deploy(&mut skill, &f.ide, None) {
                    Ok(_) => {
                        reg.upsert(skill);
                        ok += 1;
                    }
                    Err(e) => last_err = Some(format!("{}: {e}", skill.name)),
                }
            }
        }
        let _ = reg.save();
    }
    if ok > 0 {
        toast_info(&tx, format!("deployed {ok}/{} to {}", ids.len(), f.ide));
        invalidate(&tx, ResourceKind::Skills);
        invalidate(&tx, ResourceKind::Projects);
    }
    if let Some(e) = last_err {
        toast_error(&tx, e);
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}

async fn group_undeploy_all(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    Form(f): Form<GroupScopeForm>,
) -> Response {
    let tx = st.events.clone();
    let _guard = st.write_lock.lock().await;
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response();
        }
    };
    let ids: Vec<String> = reg
        .list()
        .filter_map(|s| {
            if let SkillSource::GitHub {
                owner: o, repo: r, ..
            } = &s.source
            {
                if *o == owner && *r == repo {
                    return Some(s.id.clone());
                }
            }
            None
        })
        .collect();
    let project = project_path(f.project.as_deref());
    let mut ok = 0usize;
    if let Some(project) = project {
        for id in &ids {
            if skill_deploy::undeploy_from_project(id, &f.ide, &project).is_ok() {
                ok += 1;
            }
        }
    } else {
        for id in &ids {
            if let Some(mut skill) = reg.get(id).cloned() {
                if skill.deployments.contains_key(f.ide.as_str()) {
                    if install::undeploy(&mut skill, &f.ide, None).is_ok() {
                        reg.upsert(skill);
                        ok += 1;
                    }
                }
            }
        }
        let _ = reg.save();
    }
    if ok > 0 {
        toast_info(&tx, format!("undeployed {ok} from {}", f.ide));
        invalidate(&tx, ResourceKind::Skills);
        invalidate(&tx, ResourceKind::Projects);
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
    let mut reg = match SkillRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err").into_response();
        }
    };
    let ids: Vec<String> = reg
        .list()
        .filter_map(|s| {
            if let SkillSource::GitHub {
                owner: o, repo: r, ..
            } = &s.source
            {
                if *o == owner && *r == repo {
                    return Some(s.id.clone());
                }
            }
            None
        })
        .collect();
    let mut ok = 0usize;
    for id in &ids {
        if install::remove_skill(&mut reg, id).is_ok() {
            ok += 1;
        }
    }
    let _ = reg.save();
    if ok > 0 {
        toast_info(&tx, format!("removed {ok} skill(s) from {owner}/{repo}"));
        invalidate(&tx, ResourceKind::Skills);
    }
    (axum::http::StatusCode::OK, "ok").into_response()
}
