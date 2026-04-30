//! MCP view — compact table with inline detail expand.

use std::collections::{BTreeMap, BTreeSet};

use axum::extract::{Form, Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::ide;
use aiem_core::mcp::model::{McpServer, McpTransport};
use aiem_core::mcp::{self, deploy as mcp_deploy, McpRegistry};
use aiem_core::projects::ProjectStore;

use crate::events::ResourceKind;
use crate::layout::{
    btn_danger, btn_primary, btn_secondary, empty_state, page, page_header, tag, TagKind,
};
use crate::state::AppState;
use crate::tasks::{invalidate, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/mcp", get(index))
        .route("/mcp/fragment", get(fragment))
        .route("/mcp/add-json", post(add_json))
        .route("/mcp/add-quick", post(add_quick))
        .route("/mcp/add-url", post(add_url))
        .route("/mcp/:name/toggle", post(toggle))
        .route("/mcp/:name/remove", post(remove))
        .route("/mcp/:name/deploy-action", post(deploy_action))
        .route("/mcp/sync", post(sync_all))
        .route("/mcp/deploy-all-project", post(deploy_all_project))
        .route("/mcp/bundle/import", post(bundle_import))
        .route("/mcp/bundle/:name/remove", post(bundle_remove))
}

const TEMPLATE: &str = r#"{
  "server-name": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\"],
    "env": {},
    "targets": ["claude-code", "codex", "cursor", "vscode", "windsurf", "trae", "qoder", "kiro"]
  }
}"#;

#[derive(Deserialize, Default)]
struct ListQuery {
    #[serde(default)]
    q: String,
}

async fn index(State(st): State<AppState>, Query(q): Query<ListQuery>) -> Markup {
    let reg = st.mcp().ok();
    let body = html! {
        (page_header("MCP Servers", "", html! {
            form hx-post="/mcp/sync" hx-swap="none" { (btn_secondary("Sync all IDEs")) }
            button type="button" class="btn-primary"
                   onclick="document.getElementById('mcp-add').toggleAttribute('hidden')" { "Add server" }
            button type="button" class="btn-ghost"
                   onclick="document.getElementById('mcp-bundles').toggleAttribute('hidden')" { "Bundles" }
        }))

        div class="content-padding wide-content mcp-content" {
            div id="mcp-add" hidden { (add_forms()) }
            div id="mcp-bundles" hidden { (bundles_panel()) }

            form style="display:flex;gap:8px;align-items:center;margin-bottom:16px"
                 hx-get="/mcp/fragment" hx-target="#mcp-list"
                 hx-trigger="input changed delay:200ms from:input[name=q], refresh"
                 hx-swap="innerHTML" {
                input name="q" class="field" placeholder="Filter servers\u{2026}" value=(q.q)
                      style="max-width:320px";
            }

            div id="mcp-list" data-resource="mcp"
                hx-get="/mcp/fragment" hx-trigger="refresh from:body"
                hx-swap="innerHTML" { (render(reg.as_ref(), &q.q)) }
        }

        script { "function updateMcpDeployBtn(sel){const opt=sel.options[sel.selectedIndex];const state=opt.dataset.state||'sync';const btn=sel.form.querySelector('button.aiem-mcp-deploy-btn');if(!btn)return;btn.textContent=state==='sync'?'Sync':state==='deploy'?'Deploy':'Undeploy';btn.className='aiem-mcp-deploy-btn '+(state==='undeploy'?'btn-danger':'btn-primary');}" }
    };
    page("MCP", "/mcp", body)
}

async fn fragment(State(st): State<AppState>, Query(q): Query<ListQuery>) -> Markup {
    render(st.mcp().ok().as_ref(), &q.q)
}

fn add_forms() -> Markup {
    html! {
        div class="group-panel" style="margin-bottom:16px" {
            div style="padding:16px" {
                div style="font-size:14px;font-weight:600;margin-bottom:4px" { "Add MCP server \u{2014} paste JSON" }
                div class="meta" style="margin-bottom:12px" { "Supports a single server or a map of name \u{2192} config." }
                form hx-post="/mcp/add-json" hx-swap="none"
                     hx-on--after-request="this.reset();document.getElementById('mcp-add').setAttribute('hidden','');" {
                    textarea name="json" class="field" rows="8" required { (TEMPLATE) }
                    div class="flex items-center gap-2" style="margin-top:8px" {
                        (btn_primary("Save"))
                        button type="button" class="btn-ghost"
                               onclick="document.getElementById('mcp-quick').toggleAttribute('hidden')" { "Quick form" }
                        button type="button" class="btn-ghost"
                               onclick="document.getElementById('mcp-add').setAttribute('hidden','')" { "Cancel" }
                    }
                }

                div id="mcp-quick" hidden style="margin-top:12px;border-top:1px solid var(--stroke-light);padding-top:12px" {
                    div style="font-size:13px;font-weight:600;margin-bottom:8px" { "Stdio server" }
                    form hx-post="/mcp/add-quick" hx-swap="none" hx-on--after-request="this.reset()"
                         class="grid gap-3" style="grid-template-columns:repeat(4,1fr)" {
                        div { label class="label" { "Name *" } input name="name" required class="field"; }
                        div { label class="label" { "Command *" } input name="command" required placeholder="npx" class="field"; }
                        div style="grid-column:span 2" { label class="label" { "Args" } input name="args" placeholder="-y @mcp/server C:\\" class="field"; }
                        div style="grid-column:span 2" { label class="label" { "Targets (comma)" } input name="targets" value="claude-code,codex,cursor,vscode,windsurf,trae,qoder,kiro" class="field"; }
                        div style="grid-column:span 2" { label class="label" { "Bundle (optional)" } input name="bundle" class="field"; }
                        div class="flex items-end" { (btn_primary("Add stdio")) }
                    }

                    div style="font-size:13px;font-weight:600;margin:16px 0 8px" { "SSE / HTTP server" }
                    form hx-post="/mcp/add-url" hx-swap="none" hx-on--after-request="this.reset()"
                         class="grid gap-3" style="grid-template-columns:repeat(3,1fr)" {
                        div { label class="label" { "Name *" } input name="name" required class="field"; }
                        div style="grid-column:span 2" { label class="label" { "URL *" } input name="url" required class="field" placeholder="http://localhost:8080/sse"; }
                        div { label class="label" { "Type" }
                            select name="transport_type" class="field" { option value="sse" { "SSE" } option value="http" { "HTTP" } }
                        }
                        div style="grid-column:span 2" { label class="label" { "Targets (comma)" } input name="targets" value="claude-code,codex,cursor,vscode,windsurf,trae,qoder,kiro" class="field"; }
                        div class="flex items-end" { (btn_primary("Add URL server")) }
                    }
                }
            }
        }
    }
}

fn render(reg: Option<&McpRegistry>, filter: &str) -> Markup {
    let Some(reg) = reg else {
        return html! { div style="color:var(--danger);padding:16px" { "Failed to load MCP registry." } };
    };
    let fl = filter.trim().to_ascii_lowercase();
    let items: Vec<&McpServer> = reg
        .list()
        .filter(|s| fl.is_empty() || s.name.to_ascii_lowercase().contains(&fl))
        .collect();
    if reg.list().count() == 0 {
        return empty_state(
            "No MCP servers yet",
            "Click \"Add server\" to register one.",
        );
    }
    if items.is_empty() {
        return empty_state("No matches", "Try a different filter.");
    }
    let projects: Vec<(String, String)> = ProjectStore::load()
        .map(|s| s.list().map(|p| (p.path.clone(), p.name.clone())).collect())
        .unwrap_or_default();

    html! {
        div class="group-panel" {
            table class="aiem" {
                thead { tr {
                    th { "Name" }
                    th { "Transport" }
                    th { "Targets" }
                    th { "Status" }
                    th style="text-align:right" { "Actions" }
                }}
                tbody {
                    @for s in &items { (render_row(s, &projects)) }
                }
            }
        }
    }
}

fn render_row(s: &McpServer, projects: &[(String, String)]) -> Markup {
    let (kind, detail) = match &s.transport {
        McpTransport::Stdio {
            command,
            args,
            env,
            cwd,
            bundle,
        } => {
            let mut d = format!("{command} {}", args.join(" "));
            if let Some(cwd) = cwd {
                d.push_str(&format!("  (cwd: {cwd})"));
            }
            if !env.is_empty() {
                d.push_str(&format!(
                    "  [env: {}]",
                    env.keys().cloned().collect::<Vec<_>>().join(",")
                ));
            }
            if let Some(b) = bundle {
                d.push_str(&format!("  [bundle: {b}]"));
            }
            ("stdio", d)
        }
        McpTransport::Http { url, .. } => ("http", url.clone()),
        McpTransport::Sse { url, .. } => ("sse", url.clone()),
    };
    let deployed_on: Vec<String> = mcp_deploy::projects_with(&s.name).unwrap_or_default();
    let global_synced_count = ide::IDES
        .iter()
        .filter(|target| mcp_is_synced(&s.name, target.id, None))
        .count();
    let default_ide = default_mcp_ide(s);
    let action_url = format!("/mcp/{}/deploy-action", s.name);
    let row_id = s.name.replace(|c: char| !c.is_alphanumeric(), "-");

    html! {
        tr {
            td {
                span style="font-weight:500" { (s.name) }
                @if let Some(d) = &s.description {
                    div class="meta" style="margin-top:1px" { (d) }
                }
            }
            td { (tag(kind, TagKind::Neutral)) }
            td {
                div class="row-gap" {
                    @for t in &s.targets { (tag(t, TagKind::Success)) }
                }
            }
            td {
                @if s.disabled { (tag("disabled", TagKind::Danger)) }
                @else {
                    div class="row-gap" {
                        @if global_synced_count > 0 {
                            (tag(&format!("{global_synced_count} IDE"), TagKind::Success))
                        }
                        @if !deployed_on.is_empty() {
                            (tag(&format!("{} project(s)", deployed_on.len()), TagKind::Neutral))
                        }
                        @if global_synced_count == 0 && deployed_on.is_empty() {
                            span class="meta" { "\u{2014}" }
                        }
                    }
                }
            }
            td style="text-align:right;white-space:nowrap" {
                div style="display:flex;gap:4px;justify-content:flex-end;align-items:center" {
                    form hx-post=(action_url) hx-swap="none"
                         class="mcp-action-bar" {
                        select name="ide" class="field" style="width:auto;min-width:106px" {
                            @for ide_def in ide::IDES {
                                option value=(ide_def.id) selected[ide_def.id == default_ide.as_str()] { (ide_def.display_name) }
                            }
                        }
                        select name="scope" class="field" style="width:auto;min-width:94px" {
                            option value="global" { "Global" }
                            @for (path, name) in projects {
                                option value=(path) { (name) }
                            }
                        }
                        button type="submit" name="action" value="deploy" class="btn-primary" { "Deploy" }
                        button type="submit" name="action" value="remove" class="btn-danger"
                               hx-confirm="Remove from the selected IDE/scope?" { "Remove" }
                    }
                    button type="button" class="btn-ghost"
                           onclick=(format!("document.getElementById('mcp-detail-{row_id}').toggleAttribute('hidden')")) { "More" }
                }
            }
        }
        // Expandable detail row
        tr id=(format!("mcp-detail-{row_id}")) hidden {
            td colspan="5" style="padding:12px 16px;background:var(--surface-alt);border-bottom:1px solid var(--stroke-light)" {
                div class="detail-split" {
                    div class="detail-stack" {
                        div {
                            div class="label" { "Transport" }
                            div class="mono meta" style="word-break:break-all" { (detail) }
                        }
                        div {
                            div class="label" { "Deployment records" }
                            (deployment_records_table(&mcp_deployment_records(s)))
                        }
                        div class="detail-action-row" {
                            form hx-post=(format!("/mcp/{}/toggle", s.name)) hx-swap="none" style="display:inline" {
                                (btn_secondary(if s.disabled { "Enable" } else { "Disable" }))
                            }
                            form hx-post=(format!("/mcp/{}/remove", s.name)) hx-swap="none"
                                 hx-confirm="Remove this MCP server?" style="display:inline" {
                                (btn_danger("Remove"))
                            }
                        }
                    }
                    div {
                        div class="label" { "Project scope" }
                        @if !deployed_on.is_empty() {
                            div style="display:flex;gap:4px;align-items:center;flex-wrap:wrap" {
                                @for n in &deployed_on { (tag(n, TagKind::Neutral)) }
                            }
                        } @else {
                            div class="meta" { "No projects deployed." }
                        }
                    }
                }
            }
        }
    }
}

// ─── Handlers ────────────────────────────────────────────────────────────

fn normalized_targets(targets: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for target in targets {
        let canonical = aiem_core::mcp::adapters::canonical_id(target).to_string();
        if ide::find(&canonical).is_some() && seen.insert(canonical.clone()) {
            out.push(canonical);
        }
    }
    out
}

fn default_mcp_ide(server: &McpServer) -> String {
    normalized_targets(&server.targets)
        .into_iter()
        .find(|target| ide::find(target).is_some())
        .unwrap_or_else(|| "claude-code".to_string())
}

fn mcp_is_synced(name: &str, ide_id: &str, project: Option<&std::path::Path>) -> bool {
    aiem_core::mcp::adapters::read(ide_id, project)
        .map(|servers| servers.iter().any(|s| s.name == name))
        .unwrap_or(false)
}

fn mcp_deployment_records(s: &McpServer) -> Vec<(String, String, String, TagKind)> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    let global_targets = normalized_targets(&s.targets);

    for ide_id in &global_targets {
        let ide_label = ide::find(ide_id)
            .map(|i| i.display_name.to_string())
            .unwrap_or_else(|| ide_id.clone());
        let synced = mcp_is_synced(&s.name, ide_id, None);
        let (status, kind) = if s.disabled {
            ("Disabled".to_string(), TagKind::Danger)
        } else if synced {
            ("Synced".to_string(), TagKind::Success)
        } else {
            ("Not synced".to_string(), TagKind::Neutral)
        };
        if seen.insert(("Global".to_string(), ide_label.clone())) {
            rows.push(("Global".to_string(), ide_label, status, kind));
        }
    }

    if let Ok(store) = ProjectStore::load() {
        for project in store
            .list()
            .filter(|p| p.mcp_servers.iter().any(|name| name == &s.name))
        {
            let project_path = std::path::Path::new(&project.path);
            let mut found_synced = false;
            for ide_def in ide::IDES {
                if mcp_is_synced(&s.name, ide_def.id, Some(project_path)) {
                    found_synced = true;
                    if seen.insert((project.name.clone(), ide_def.display_name.to_string())) {
                        rows.push((
                            project.name.clone(),
                            ide_def.display_name.to_string(),
                            "Deployed".to_string(),
                            TagKind::Success,
                        ));
                    }
                }
            }

            if !found_synced {
                let ides = if project.ides.is_empty() {
                    global_targets.clone()
                } else {
                    project.ides.clone()
                };
                for ide_id in ides {
                    let ide_label = ide::find(&ide_id)
                        .map(|i| i.display_name.to_string())
                        .unwrap_or(ide_id);
                    if seen.insert((project.name.clone(), ide_label.clone())) {
                        rows.push((
                            project.name.clone(),
                            ide_label,
                            "Not synced".to_string(),
                            TagKind::Neutral,
                        ));
                    }
                }
            }
        }
    }

    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    rows
}

fn deployment_records_table(rows: &[(String, String, String, TagKind)]) -> Markup {
    html! {
        @if rows.is_empty() {
            div class="meta" { "No deployment records." }
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

#[derive(Deserialize)]
struct JsonForm {
    json: String,
}

async fn add_json(State(st): State<AppState>, Form(f): Form<JsonForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let servers = match parse_json_servers(&f.json) {
        Ok(v) => v,
        Err(e) => {
            toast_error(&tx, e);
            return http_ok();
        }
    };
    let mut reg = match McpRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return http_ok();
        }
    };
    let n = servers.len();
    for s in servers {
        reg.upsert(s);
    }
    if let Err(e) = reg.save() {
        toast_error(&tx, format!("save: {e}"));
        return http_ok();
    }
    toast_info(&tx, format!("saved {n} server(s)"));
    invalidate(&tx, ResourceKind::Mcp);
    http_ok()
}

#[derive(Deserialize)]
struct QuickForm {
    name: String,
    command: String,
    #[serde(default)]
    args: String,
    #[serde(default)]
    targets: String,
    #[serde(default)]
    bundle: String,
}

async fn add_quick(State(st): State<AppState>, Form(f): Form<QuickForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut reg = match McpRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return http_ok();
        }
    };
    let args: Vec<String> = f.args.split_whitespace().map(|s| s.to_string()).collect();
    let targets: Vec<String> = f
        .targets
        .split(',')
        .filter_map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .collect();
    let bundle = {
        let b = f.bundle.trim();
        if b.is_empty() {
            None
        } else {
            Some(b.to_string())
        }
    };
    let name = f.name.clone();
    reg.upsert(McpServer {
        name: name.clone(),
        transport: McpTransport::Stdio {
            command: f.command,
            args,
            env: Default::default(),
            cwd: None,
            bundle,
        },
        targets,
        description: None,
        tags: vec![],
        disabled: false,
        source: None,
        runtime: None,
        auth_mode: Default::default(),
    });
    if let Err(e) = reg.save() {
        toast_error(&tx, format!("save: {e}"));
        return http_ok();
    }
    toast_info(&tx, format!("added {name}"));
    invalidate(&tx, ResourceKind::Mcp);
    http_ok()
}

#[derive(Deserialize)]
struct UrlForm {
    name: String,
    url: String,
    #[serde(default)]
    transport_type: String,
    #[serde(default)]
    targets: String,
}

async fn add_url(State(st): State<AppState>, Form(f): Form<UrlForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut reg = match McpRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return http_ok();
        }
    };
    let targets: Vec<String> = f
        .targets
        .split(',')
        .filter_map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .collect();
    let transport = if f.transport_type == "http" {
        McpTransport::Http {
            url: f.url,
            headers: Default::default(),
        }
    } else {
        McpTransport::Sse {
            url: f.url,
            headers: Default::default(),
        }
    };
    let name = f.name.clone();
    reg.upsert(McpServer {
        name: name.clone(),
        transport,
        targets,
        description: None,
        tags: vec![],
        disabled: false,
        source: None,
        runtime: None,
        auth_mode: Default::default(),
    });
    if let Err(e) = reg.save() {
        toast_error(&tx, format!("save: {e}"));
        return http_ok();
    }
    toast_info(&tx, format!("added {name}"));
    invalidate(&tx, ResourceKind::Mcp);
    http_ok()
}

async fn toggle(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut reg = match McpRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return http_ok();
        }
    };
    let Some(s) = reg.get_mut(&name) else {
        toast_error(&tx, "not found");
        return http_ok();
    };
    s.disabled = !s.disabled;
    let disabled = s.disabled;
    if let Err(e) = reg.save() {
        toast_error(&tx, format!("{e}"));
        return http_ok();
    }
    toast_info(
        &tx,
        format!("{name} {}", if disabled { "disabled" } else { "enabled" }),
    );
    invalidate(&tx, ResourceKind::Mcp);
    http_ok()
}

async fn remove(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut reg = match McpRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return http_ok();
        }
    };
    match reg.remove(&name) {
        Ok(_) => {
            let _ = reg.save();
            toast_info(&tx, format!("removed {name}"));
            invalidate(&tx, ResourceKind::Mcp);
        }
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    http_ok()
}

async fn sync_all(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let reg = match McpRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return http_ok();
        }
    };
    let plan = mcp::sync::plan(&reg, &[], None);
    match mcp::sync::execute(&reg, &plan, None, None) {
        Ok(touched) => {
            let msg = if touched.is_empty() {
                "nothing to sync".to_string()
            } else {
                touched
                    .iter()
                    .map(|(ide, p)| format!("{ide}\u{2192}{}", p.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            toast_info(&tx, format!("synced: {msg}"));
            invalidate(&tx, ResourceKind::Mcp);
        }
        Err(e) => toast_error(&tx, format!("sync: {e}")),
    }
    http_ok()
}

fn http_ok() -> Response {
    (axum::http::StatusCode::OK, "ok").into_response()
}

#[derive(Deserialize)]
struct ScopeForm {
    scope: String,
    #[serde(default)]
    ide: String,
    #[serde(default)]
    action: String,
}

async fn deploy_action(
    State(st): State<AppState>,
    Path(name): Path<String>,
    Form(f): Form<ScopeForm>,
) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let ide = if f.ide.trim().is_empty() {
        "claude-code".to_string()
    } else {
        aiem_core::mcp::adapters::canonical_id(f.ide.trim()).to_string()
    };
    let ides = vec![ide.clone()];
    let remove = f.action == "remove";
    if f.scope == "global" {
        let result = if remove {
            mcp::sync::retract_one_global_from_ides(&name, &ides)
        } else {
            mcp::sync::sync_one_global(&name, &ides)
        };
        match result {
            Ok(touched) => {
                let verb = if remove { "removed" } else { "synced" };
                toast_info(
                    &tx,
                    format!("{verb} {name} \u{2192} {ide} ({} file(s))", touched.len()),
                );
                invalidate(&tx, ResourceKind::Mcp);
            }
            Err(e) => toast_error(&tx, format!("sync: {e}")),
        }
        return http_ok();
    }
    let project = std::path::PathBuf::from(&f.scope);
    if remove {
        match mcp_deploy::undeploy_from_project_for_ides(&name, &project, &ides) {
            Ok(_) => {
                toast_info(&tx, format!("undeployed {name} from {ide}"));
                invalidate(&tx, ResourceKind::Mcp);
            }
            Err(e) => toast_error(&tx, format!("{e}")),
        }
    } else {
        match mcp_deploy::deploy_to_project_for_ides(&name, &project, &ides) {
            Ok(touched) => {
                toast_info(
                    &tx,
                    format!(
                        "deployed {name} \u{2192} {ide} project ({} file(s))",
                        touched.len()
                    ),
                );
                invalidate(&tx, ResourceKind::Mcp);
            }
            Err(e) => toast_error(&tx, format!("{e}")),
        }
    }
    http_ok()
}

#[derive(Deserialize)]
struct DeployAllForm {
    scope: String,
}

async fn deploy_all_project(State(st): State<AppState>, Form(f): Form<DeployAllForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    if f.scope == "global" {
        let reg = match McpRegistry::load() {
            Ok(r) => r,
            Err(e) => {
                toast_error(&tx, format!("{e}"));
                return http_ok();
            }
        };
        let plan = mcp::sync::plan(&reg, &[], None);
        match mcp::sync::execute(&reg, &plan, None, None) {
            Ok(touched) => {
                toast_info(
                    &tx,
                    format!("synced all \u{2192} global ({} file(s))", touched.len()),
                );
                invalidate(&tx, ResourceKind::Mcp);
            }
            Err(e) => toast_error(&tx, format!("sync: {e}")),
        }
        return http_ok();
    }
    let project = std::path::PathBuf::from(&f.scope);
    let reg = match McpRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return http_ok();
        }
    };
    let mut ok = 0usize;
    for srv in reg.list() {
        if srv.disabled {
            continue;
        }
        if mcp_deploy::deploy_to_project(&srv.name, &project).is_ok() {
            ok += 1;
        }
    }
    if ok > 0 {
        toast_info(&tx, format!("deployed {ok} server(s) to project"));
        invalidate(&tx, ResourceKind::Mcp);
    } else {
        toast_info(&tx, "no servers to deploy");
    }
    http_ok()
}

// ─── JSON parsing ────────────────────────────────────────────────────────

fn parse_json_servers(input: &str) -> Result<Vec<McpServer>, String> {
    let val: serde_json::Value =
        serde_json::from_str(input.trim()).map_err(|e| format!("JSON parse error: {e}"))?;
    let obj = val.as_object().ok_or("Expected a JSON object")?;
    let mut servers = Vec::new();
    if obj.contains_key("command") || obj.contains_key("url") {
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        servers.push(json_to_server(&name, &val)?);
    } else {
        for (name, config) in obj {
            servers.push(json_to_server(name, config)?);
        }
    }
    if servers.is_empty() {
        return Err("No servers found in JSON".into());
    }
    Ok(servers)
}

fn json_to_server(name: &str, val: &serde_json::Value) -> Result<McpServer, String> {
    let obj = val.as_object().ok_or(format!("{name}: expected object"))?;
    let transport = if let Some(cmd) = obj.get("command").and_then(|v| v.as_str()) {
        let args: Vec<String> = obj
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let env: BTreeMap<String, String> = obj
            .get("env")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let cwd = obj.get("cwd").and_then(|v| v.as_str()).map(String::from);
        let bundle = obj
            .get("bundle")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);
        McpTransport::Stdio {
            command: cmd.to_string(),
            args,
            env,
            cwd,
            bundle,
        }
    } else if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
        let headers: BTreeMap<String, String> = obj
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("sse");
        if kind == "http" {
            McpTransport::Http {
                url: url.to_string(),
                headers,
            }
        } else {
            McpTransport::Sse {
                url: url.to_string(),
                headers,
            }
        }
    } else {
        return Err(format!(
            "{name}: need 'command' (stdio) or 'url' (http/sse)"
        ));
    };
    let targets: Vec<String> = obj
        .get("targets")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                "claude-code".into(),
                "codex".into(),
                "cursor".into(),
                "vscode".into(),
                "windsurf".into(),
                "trae".into(),
                "qoder".into(),
                "kiro".into(),
            ]
        });
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);
    Ok(McpServer {
        name: name.to_string(),
        transport,
        targets,
        description,
        tags: vec![],
        disabled: false,
        source: None,
        runtime: None,
        auth_mode: Default::default(),
    })
}

// ─── Bundle management ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct BundleImportForm {
    name: String,
    src_path: String,
}

async fn bundle_import(State(st): State<AppState>, Form(f): Form<BundleImportForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let src = std::path::PathBuf::from(f.src_path.trim());
    match mcp::bundles::import_bundle(f.name.trim(), &src) {
        Ok(p) => {
            toast_info(&tx, format!("bundle imported to {}", p.display()));
            invalidate(&tx, ResourceKind::Mcp);
        }
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    http_ok()
}

async fn bundle_remove(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    match mcp::bundles::remove_bundle(&name) {
        Ok(_) => {
            toast_info(&tx, format!("bundle `{name}` moved to trash"));
            invalidate(&tx, ResourceKind::Mcp);
        }
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    http_ok()
}

fn bundles_panel() -> Markup {
    let bundles = mcp::bundles::list_bundles().unwrap_or_default();
    let bundle_dir = aiem_core::paths::mcp_bundles_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    html! {
        div class="group-panel" style="margin-bottom:16px" {
            div style="padding:16px" {
                div style="font-size:14px;font-weight:600;margin-bottom:8px" { "MCP Bundles" }
                p class="meta" style="margin-bottom:12px" {
                    "Copy a local script directory into " code class="mono" { (bundle_dir) } ". "
                    "Reference the deployed path via " code class="mono" { "{BUNDLE}" } "."
                }
                form hx-post="/mcp/bundle/import" hx-swap="none" hx-on--after-request="this.reset()"
                     class="grid gap-3" style="grid-template-columns:repeat(3,1fr);align-items:end;margin-bottom:12px" {
                    div { label class="label" { "Bundle name *" } input name="name" required placeholder="my-mcp" class="field"; }
                    div style="grid-column:span 2" { label class="label" { "Source directory *" } input name="src_path" required placeholder="/path/to/local/mcp" class="field"; }
                    div { (btn_primary("Import bundle")) }
                }
                @if bundles.is_empty() {
                    p class="meta" { "No bundles yet." }
                } @else {
                    table class="aiem" {
                        thead { tr { th { "Name" } th { "Path" } th style="text-align:right" { "Actions" } } }
                        tbody {
                            @for b in &bundles {
                                tr {
                                    td class="mono" { (b) }
                                    td class="meta" { (format!("{bundle_dir}/{b}")) }
                                    td style="text-align:right" {
                                        form hx-post={"/mcp/bundle/" (urlencoding::encode(b)) "/remove"}
                                             hx-swap="none" hx-confirm={"Move bundle " (b) " to trash?"}
                                             style="display:inline" {
                                            (btn_danger("Delete"))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
