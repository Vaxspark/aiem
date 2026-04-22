//! MCP view — feature parity with the desktop GUI.

use std::collections::BTreeMap;

use axum::extract::{Form, Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::mcp::model::{McpServer, McpTransport};
use aiem_core::mcp::{self, deploy as mcp_deploy, McpRegistry};
use aiem_core::projects::ProjectStore;

use crate::events::ResourceKind;
use crate::layout::{btn_danger, btn_primary, btn_secondary, card, empty_state, page, page_header, tag, TagKind};
use crate::state::AppState;
use crate::tasks::{invalidate, toast_error, toast_info};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/mcp", get(index))
        .route("/mcp/fragment", get(fragment))
        .route("/mcp/add-json", post(add_json))
        .route("/mcp/add-quick", post(add_quick))
        .route("/mcp/:name/toggle", post(toggle))
        .route("/mcp/:name/remove", post(remove))
        .route("/mcp/:name/deploy-action", post(deploy_action))
        .route("/mcp/sync", post(sync_all))
        .route("/mcp/bundle/import", post(bundle_import))
        .route("/mcp/bundle/:name/remove", post(bundle_remove))
}

const TEMPLATE: &str = r#"{
  "server-name": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\"],
    "env": {},
    "targets": ["codex", "claude-code", "copilot"]
  }
}"#;

#[derive(Deserialize, Default)]
struct ListQuery { #[serde(default)] q: String }

async fn index(State(st): State<AppState>, Query(q): Query<ListQuery>) -> Markup {
    let reg = st.mcp().ok();
    let body = html! {
        (page_header("MCP Servers", "Register Model Context Protocol servers and sync them to every IDE.", html! {
            form hx-post="/mcp/sync" hx-swap="none" { (btn_secondary("Sync all IDEs")) }
            button type="button" class="btn-primary" onclick="toggleHidden('mcp-add')" { "+ Add server" }
            button type="button" class="btn-ghost" onclick="toggleHidden('mcp-bundles')" { "Bundles" }
        }))

        div id="mcp-add" hidden { (add_forms()) }
        div id="mcp-bundles" hidden { (bundles_panel()) }

        form class="aiem-card" style="display:flex;gap:8px;align-items:center;margin-bottom:12px"
             hx-get="/mcp/fragment" hx-target="#mcp-list" hx-trigger="input changed delay:200ms from:input[name=q], refresh"
             hx-swap="innerHTML" {
            label class="label" style="margin:0;min-width:40px" { "Filter" }
            input name="q" class="field" placeholder="name" value=(q.q);
        }

        div id="mcp-list" data-resource="mcp"
            hx-get="/mcp/fragment" hx-trigger="refresh from:body"
            hx-swap="innerHTML" { (render(reg.as_ref(), &q.q)) }

        script { "function toggleHidden(id){const el=document.getElementById(id);el.toggleAttribute('hidden');}\nfunction updateMcpDeployBtn(sel){const opt=sel.options[sel.selectedIndex];const state=opt.dataset.state||'sync';const btn=sel.form.querySelector('button.aiem-mcp-deploy-btn');if(!btn)return;btn.textContent=state==='sync'?'Sync':state==='deploy'?'Deploy':'Undeploy';btn.className='aiem-mcp-deploy-btn '+(state==='undeploy'?'btn-danger':'btn-primary');}" }
    };
    page("MCP", "/mcp", body)
}

async fn fragment(State(st): State<AppState>, Query(q): Query<ListQuery>) -> Markup {
    render(st.mcp().ok().as_ref(), &q.q)
}

fn add_forms() -> Markup {
    card(html! {
        div class="text-sm font-semibold mb-1" { "Add MCP server — paste JSON (Claude / Codex format)" }
        div class="meta mb-3" { "Supports a single server, or a map of name → config. Use command+args for stdio, or url for http/sse." }
        form hx-post="/mcp/add-json" hx-swap="none"
             hx-on--after-request="this.reset();document.getElementById('mcp-add').setAttribute('hidden','');" {
            textarea name="json" class="field" rows="10" required { (TEMPLATE) }
            div class="flex items-center gap-2" style="margin-top:8px" {
                (btn_primary("Save"))
                button type="button" class="btn-ghost" onclick="toggleHidden('mcp-quick')" { "Or use quick form" }
                button type="button" class="btn-ghost"
                       onclick="document.getElementById('mcp-add').setAttribute('hidden','')" { "Cancel" }
            }
        }

        div id="mcp-quick" hidden style="margin-top:12px;border-top:1px solid var(--stroke);padding-top:12px" {
            div class="text-xs font-semibold mb-2" { "Quick stdio server" }
            form hx-post="/mcp/add-quick" hx-swap="none"
                 hx-on--after-request="this.reset()"
                 class="grid gap-3" style="grid-template-columns:repeat(4,1fr)" {
                div { label class="label" { "Name *" } input name="name" required class="field"; }
                div { label class="label" { "Command *" } input name="command" required placeholder="npx" class="field"; }
                div style="grid-column:span 2" { label class="label" { "Args" } input name="args" placeholder="-y @modelcontextprotocol/server-filesystem C:\\" class="field"; }
                div style="grid-column:span 2" { label class="label" { "Targets (comma)" } input name="targets" value="codex,claude-code,copilot" class="field"; }
                div style="grid-column:span 2" { label class="label" { "Bundle (optional)" } input name="bundle" placeholder="my-mcp" class="field"; }
                div class="flex items-end" { (btn_primary("Add")) }
            }
        }
    })
}

fn render(reg: Option<&McpRegistry>, filter: &str) -> Markup {
    let Some(reg) = reg else {
        return html! { div class="aiem-card" style="color:var(--danger)" { "Failed to load MCP registry." } };
    };
    let fl = filter.trim().to_ascii_lowercase();
    let items: Vec<&McpServer> = reg.list()
        .filter(|s| fl.is_empty() || s.name.to_ascii_lowercase().contains(&fl))
        .collect();
    if reg.list().count() == 0 {
        return empty_state("No MCP servers yet", "Click \"+ Add server\" to register one.");
    }
    if items.is_empty() {
        return empty_state("No matches", "Try a different filter.");
    }
    // Registered projects: (path, display_name) — used by the per-card scope picker.
    let projects: Vec<(String, String)> = ProjectStore::load()
        .map(|s| s.list().map(|p| (p.path.clone(), p.name.clone())).collect())
        .unwrap_or_default();
    html! {
        @for s in &items { (render_card(s, &projects)) }
    }
}

fn render_card(s: &McpServer, projects: &[(String, String)]) -> Markup {
    let (kind, detail) = match &s.transport {
        McpTransport::Stdio { command, args, env, cwd, bundle } => {
            let mut d = format!("{command} {}", args.join(" "));
            if let Some(cwd) = cwd { d.push_str(&format!("  (cwd: {cwd})")); }
            if !env.is_empty() {
                d.push_str(&format!("  [env: {}]", env.keys().cloned().collect::<Vec<_>>().join(",")));
            }
            if let Some(b) = bundle { d.push_str(&format!("  [bundle: {b}]")); }
            ("stdio", d)
        }
        McpTransport::Http { url, .. } => ("http", url.clone()),
        McpTransport::Sse  { url, .. } => ("sse",  url.clone()),
    };
    // Reverse-lookup: which registered projects already have this server attached.
    let deployed_on: Vec<String> = mcp_deploy::projects_with(&s.name).unwrap_or_default();
    let deployed_set: std::collections::HashSet<&str> =
        deployed_on.iter().map(|s| s.as_str()).collect();
    let action_url = format!("/mcp/{}/deploy-action", s.name);
    html! {
        div class="aiem-card" {
            div style="display:flex;gap:12px;align-items:flex-start;flex-wrap:wrap" {
                div style="flex:1;min-width:260px" {
                    div class="row-gap" {
                        span style="font-weight:600;font-size:14px" { (s.name) }
                        (tag(kind, TagKind::Neutral))
                        @if s.disabled { (tag("disabled", TagKind::Danger)) }
                    }
                    div class="meta mono" style="margin-top:4px;word-break:break-all" { (detail) }
                    @if let Some(d) = &s.description { div class="meta" style="margin-top:2px" { (d) } }
                    @if !s.targets.is_empty() {
                        div class="row-gap" style="margin-top:6px" {
                            @for t in &s.targets { (tag(t, TagKind::Success)) }
                        }
                    }
                }
                // ── All actions in ONE row (matches skills card layout) ──
                div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap" {
                    // Scope picker + state-aware Deploy/Sync/Undeploy button.
                    form hx-post=(action_url) hx-swap="none"
                         style="display:inline-flex;gap:6px;align-items:center;margin:0" {
                        select name="scope" class="field"
                                style="width:auto;min-width:140px"
                                onchange="updateMcpDeployBtn(this)" {
                            // Global → "Sync"
                            option value="global" data-state="sync" { "Global" }
                            // Projects → "Undeploy" if already attached, else "Deploy"
                            @for (path, name) in projects {
                                @if deployed_set.contains(name.as_str()) {
                                    option value=(path) data-state="undeploy" { (format!("{name}  \u{2713}")) }
                                } @else {
                                    option value=(path) data-state="deploy" { (name) }
                                }
                            }
                        }
                        button type="submit" class="btn-primary aiem-mcp-deploy-btn" { "Sync" }
                    }
                    form hx-post=(format!("/mcp/{}/toggle", s.name)) hx-swap="none" style="margin:0" {
                        (btn_secondary(if s.disabled { "Enable" } else { "Disable" }))
                    }
                    form hx-post=(format!("/mcp/{}/remove", s.name)) hx-swap="none"
                         hx-confirm="Remove this MCP server from the registry?" style="margin:0" {
                        (btn_danger("Remove"))
                    }
                }
            }

            // Chips row: which projects currently have this server attached.
            @if !deployed_on.is_empty() {
                div style="margin-top:8px;display:flex;gap:6px;align-items:center;flex-wrap:wrap" {
                    span class="meta" { "Deployed:" }
                    @for n in &deployed_on { (tag(n, TagKind::Neutral)) }
                }
            }
        }
    }
}

// ─── Handlers ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JsonForm { json: String }

async fn add_json(State(st): State<AppState>, Form(f): Form<JsonForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let servers = match parse_json_servers(&f.json) {
        Ok(v) => v,
        Err(e) => { toast_error(&tx, e); return http_ok(); }
    };
    let mut reg = match McpRegistry::load() {
        Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return http_ok(); }
    };
    let n = servers.len();
    for s in servers { reg.upsert(s); }
    if let Err(e) = reg.save() { toast_error(&tx, format!("save: {e}")); return http_ok(); }
    toast_info(&tx, format!("saved {n} server(s)"));
    invalidate(&tx, ResourceKind::Mcp);
    http_ok()
}

#[derive(Deserialize)]
struct QuickForm {
    name: String,
    command: String,
    #[serde(default)] args: String,
    #[serde(default)] targets: String,
    #[serde(default)] bundle: String,
}

async fn add_quick(State(st): State<AppState>, Form(f): Form<QuickForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut reg = match McpRegistry::load() {
        Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return http_ok(); }
    };
    let args: Vec<String> = f.args.split_whitespace().map(|s| s.to_string()).collect();
    let targets: Vec<String> = f.targets.split(',')
        .filter_map(|s| { let t = s.trim(); if t.is_empty() { None } else { Some(t.to_string()) } })
        .collect();
    let bundle = {
        let b = f.bundle.trim();
        if b.is_empty() { None } else { Some(b.to_string()) }
    };
    let name = f.name.clone();
    reg.upsert(McpServer {
        name: name.clone(),
        transport: McpTransport::Stdio { command: f.command, args, env: Default::default(), cwd: None, bundle },
        targets,
        description: None,
        tags: vec![],
        disabled: false,
    });
    if let Err(e) = reg.save() { toast_error(&tx, format!("save: {e}")); return http_ok(); }
    toast_info(&tx, format!("added {name}"));
    invalidate(&tx, ResourceKind::Mcp);
    http_ok()
}

async fn toggle(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut reg = match McpRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return http_ok(); } };
    let Some(s) = reg.get_mut(&name) else { toast_error(&tx, "not found"); return http_ok(); };
    s.disabled = !s.disabled;
    let disabled = s.disabled;
    if let Err(e) = reg.save() { toast_error(&tx, format!("{e}")); return http_ok(); }
    toast_info(&tx, format!("{name} {}", if disabled {"disabled"} else {"enabled"}));
    invalidate(&tx, ResourceKind::Mcp);
    http_ok()
}

async fn remove(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut reg = match McpRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return http_ok(); } };
    match reg.remove(&name) {
        Ok(_) => { let _ = reg.save(); toast_info(&tx, format!("removed {name}")); invalidate(&tx, ResourceKind::Mcp); }
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    http_ok()
}

async fn sync_all(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let reg = match McpRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return http_ok(); } };
    let plan = mcp::sync::plan(&reg, &[]);
    match mcp::sync::execute(&reg, &plan, None) {
        Ok(touched) => {
            let msg = if touched.is_empty() {
                "nothing to sync".to_string()
            } else {
                touched.iter().map(|(ide,p)| format!("{ide}→{}", p.display())).collect::<Vec<_>>().join(", ")
            };
            toast_info(&tx, format!("synced: {msg}"));
            invalidate(&tx, ResourceKind::Mcp);
        }
        Err(e) => toast_error(&tx, format!("sync: {e}")),
    }
    http_ok()
}

fn http_ok() -> Response { (axum::http::StatusCode::OK, "ok").into_response() }

#[derive(Deserialize)]
struct ScopeForm { scope: String }

/// State-aware single-button deploy action (mirrors the skills card UX).
///
/// - `scope == "global"` → full sync to user-scope IDE configs.
/// - `scope == <project path>`:
///     * if the server is already in the project's `mcp_servers` → undeploy
///     * otherwise → deploy
async fn deploy_action(
    State(st): State<AppState>,
    Path(name): Path<String>,
    Form(f): Form<ScopeForm>,
) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    if f.scope == "global" {
        let reg = match McpRegistry::load() { Ok(r) => r, Err(e) => { toast_error(&tx, format!("{e}")); return http_ok(); } };
        let plan = mcp::sync::plan(&reg, &[]);
        match mcp::sync::execute(&reg, &plan, None) {
            Ok(_) => { toast_info(&tx, format!("synced {name} → global")); invalidate(&tx, ResourceKind::Mcp); }
            Err(e) => toast_error(&tx, format!("sync: {e}")),
        }
        return http_ok();
    }

    let project = std::path::PathBuf::from(&f.scope);
    let already_attached = ProjectStore::load()
        .ok()
        .and_then(|s| s.get(&f.scope).cloned())
        .map(|p| p.mcp_servers.iter().any(|n| n == &name))
        .unwrap_or(false);

    if already_attached {
        match mcp_deploy::undeploy_from_project(&name, &project) {
            Ok(_) => { toast_info(&tx, format!("undeployed {name} from project")); invalidate(&tx, ResourceKind::Mcp); }
            Err(e) => toast_error(&tx, format!("{e}")),
        }
    } else {
        match mcp_deploy::deploy_to_project(&name, &project) {
            Ok(touched) => {
                toast_info(&tx, format!("deployed {name} → project ({} file(s))", touched.len()));
                invalidate(&tx, ResourceKind::Mcp);
            }
            Err(e) => toast_error(&tx, format!("{e}")),
        }
    }
    http_ok()
}

// ─── JSON parsing (mirrors GUI) ──────────────────────────────────────────

fn parse_json_servers(input: &str) -> Result<Vec<McpServer>, String> {
    let val: serde_json::Value = serde_json::from_str(input.trim())
        .map_err(|e| format!("JSON parse error: {e}"))?;
    let obj = val.as_object().ok_or("Expected a JSON object")?;
    let mut servers = Vec::new();
    if obj.contains_key("command") || obj.contains_key("url") {
        let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed").to_string();
        servers.push(json_to_server(&name, &val)?);
    } else {
        for (name, config) in obj { servers.push(json_to_server(name, config)?); }
    }
    if servers.is_empty() { return Err("No servers found in JSON".into()); }
    Ok(servers)
}

fn json_to_server(name: &str, val: &serde_json::Value) -> Result<McpServer, String> {
    let obj = val.as_object().ok_or(format!("{name}: expected object"))?;
    let transport = if let Some(cmd) = obj.get("command").and_then(|v| v.as_str()) {
        let args: Vec<String> = obj.get("args").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let env: BTreeMap<String, String> = obj.get("env").and_then(|v| v.as_object())
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default();
        let cwd = obj.get("cwd").and_then(|v| v.as_str()).map(String::from);
        McpTransport::Stdio { command: cmd.to_string(), args, env, cwd, bundle: None }
    } else if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
        let headers: BTreeMap<String, String> = obj.get("headers").and_then(|v| v.as_object())
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default();
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("sse");
        if kind == "http" { McpTransport::Http { url: url.to_string(), headers } }
        else { McpTransport::Sse { url: url.to_string(), headers } }
    } else {
        return Err(format!("{name}: need 'command' (stdio) or 'url' (http/sse)"));
    };
    let targets: Vec<String> = obj.get("targets").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["codex".into(), "claude-code".into(), "copilot".into()]);
    let description = obj.get("description").and_then(|v| v.as_str()).map(String::from);
    Ok(McpServer {
        name: name.to_string(),
        transport,
        targets,
        description,
        tags: vec![],
        disabled: false,
    })
}

// ─── Bundle management ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BundleImportForm {
    name: String,
    src_path: String,
}

async fn bundle_import(
    State(st): State<AppState>,
    Form(f): Form<BundleImportForm>,
) -> Response {
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

async fn bundle_remove(
    State(st): State<AppState>,
    Path(name): Path<String>,
) -> Response {
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
        (card(html! {
            div class="text-sm font-semibold mb-2" { "MCP Bundles" }
            p class="meta" style="margin-bottom:12px" {
                "Copy a local script directory into " code { (bundle_dir) } ". \
                 Reference the deployed path in a server's command/args as " code { "{BUNDLE}" } "."
            }
            form hx-post="/mcp/bundle/import" hx-swap="none"
                 hx-on--after-request="this.reset()"
                 class="grid gap-3" style="grid-template-columns:repeat(3,1fr);align-items:end;margin-bottom:12px" {
                div { label class="label" { "Bundle name *" } input name="name" required placeholder="my-mcp" class="field"; }
                div style="grid-column:span 2" { label class="label" { "Source directory *" } input name="src_path" required placeholder="/abs/path/to/local/mcp" class="field"; }
                div { (btn_primary("Import bundle")) }
            }

            @if bundles.is_empty() {
                p class="meta" { "No bundles yet." }
            } @else {
                table class="table" style="width:100%" {
                    thead { tr { th { "Name" } th { "Path" } th { "Actions" } } }
                    tbody {
                        @for b in &bundles {
                            tr {
                                td { code { (b) } }
                                td class="muted" style="font-size:12px" { (format!("{}/{}", bundle_dir, b)) }
                                td {
                                    form method="post"
                                         action={"/mcp/bundle/" (urlencoding::encode(b)) "/remove"}
                                         hx-post={"/mcp/bundle/" (urlencoding::encode(b)) "/remove"}
                                         hx-swap="none"
                                         hx-confirm={"Move bundle " (b) " to trash?"}
                                         style="display:inline" {
                                        (btn_danger("Delete"))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }))
    }
}
