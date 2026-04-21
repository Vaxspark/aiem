use axum::extract::{Form, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::backup::{AutoInterval, BackupConfig};
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
        .route("/settings/backup/snapshot", post(backup_snapshot))
        .route("/settings/backup/export", post(backup_export))
        .route("/settings/backup/import", post(backup_import))
        .route("/settings/backup/push", post(backup_push))
        .route("/settings/backup/pull", post(backup_pull))
        .route("/settings/backup/interval", post(backup_set_interval))
}

async fn index(State(_st): State<AppState>) -> Markup {
    let has_token = std::env::var("GITHUB_TOKEN").map(|s| !s.is_empty()).unwrap_or(false);
    let aiem_home = aiem_core::paths::home().map(|p| p.display().to_string()).unwrap_or_else(|_| "?".into());
    let backup_cfg = BackupConfig::load().unwrap_or_default();
    let last_backup = backup_cfg.last_backup_ts
        .map(aiem_core::backup::time_ago)
        .unwrap_or_else(|| "never".into());

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
        (backup_card(&backup_cfg, &last_backup))
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

// ─── Backup card ─────────────────────────────────────────────────────────────

fn backup_card(cfg: &BackupConfig, last_backup: &str) -> Markup {
    let repo_val = cfg.github_repo.as_deref().unwrap_or("");
    card(html!{
        div class="flex items-center justify-between mb-3" {
            div class="text-sm font-semibold" { "Backup & Restore" }
            span class="meta text-xs" { "Last backup: " (last_backup) }
        }

        // Auto-interval
        div class="mb-4" {
            p class="meta text-xs mb-2" { "Auto-backup interval" }
            form hx-post="/settings/backup/interval" hx-swap="none" class="flex gap-2 flex-wrap" {
                @for (val, label) in [("never","Never"),("daily","Daily"),("weekly","Weekly")] {
                    @let selected = cfg.auto_interval == match val {
                        "daily"  => AutoInterval::Daily,
                        "weekly" => AutoInterval::Weekly,
                        _        => AutoInterval::Never,
                    };
                    button type="submit" name="interval" value=(val)
                        class={
                            "px-3 py-1 rounded text-xs border "
                            @if selected { "border-[var(--accent)] text-[var(--accent)] font-semibold" }
                            @else        { "border-[var(--border)] text-[var(--muted)]" }
                        }
                    { (label) }
                }
            }
        }

        hr class="border-[var(--border)] mb-4";

        // Local snapshot
        div class="mb-4" {
            p class="text-xs font-semibold mb-2" { "Local snapshot" }
            p class="meta text-xs mb-2" {
                "Saves skills_index.json, mcp_servers.json, projects.json into "
                code class="mono" { "~/.aiem/snapshots/<ts>/" }
            }
            form hx-post="/settings/backup/snapshot" hx-swap="none" class="mb-3" {
                (btn_primary("Snapshot now"))
            }
            p class="meta text-xs mb-1" { "Export to directory" }
            form hx-post="/settings/backup/export" hx-swap="none" class="flex gap-2" {
                input name="dest" type="text" placeholder="/path/to/export" class="field" style="flex:1";
                (btn_primary("Export"))
            }
            p class="meta text-xs mb-1 mt-3" { "Restore from directory" }
            form hx-post="/settings/backup/import"
                hx-swap="none"
                hx-confirm="Overwrite current config from this snapshot?"
                class="flex gap-2" {
                input name="src" type="text" placeholder="/path/to/snapshot" class="field" style="flex:1";
                (btn_danger("Restore"))
            }
        }

        hr class="border-[var(--border)] mb-4";

        // GitHub backup
        div {
            p class="text-xs font-semibold mb-2" { "GitHub backup" }
            p class="meta text-xs mb-2" {
                "Commits the three config files to a private GitHub repo. "
                "Uses " code class="mono" { "~/.aiem/backup-git/" } " as git working tree. Requires " code class="mono" { "git" } " in PATH."
            }
            p class="meta text-xs mb-1" { "Repo URL (HTTPS)" }
            div class="mb-2" {
                input id="backup-repo" name="repo" type="text"
                    value=(repo_val)
                    placeholder="https://github.com/you/my-aiem-backup"
                    class="field w-full";
            }
            p class="meta text-xs mb-1" { "Token override (leave empty to use saved GITHUB_TOKEN)" }
            div class="mb-3" {
                input id="backup-token" name="token" type="password"
                    placeholder="ghp_... (optional)"
                    class="field w-full";
            }
            div class="flex gap-2" {
                // Push button submits a hidden form using JS to collect the shared fields
                form id="backup-push-form"
                    hx-post="/settings/backup/push"
                    hx-swap="none"
                    class="flex gap-2" {
                    input type="hidden" name="_method" value="push";
                    input id="bpf-repo"  type="hidden" name="repo"  value=(repo_val);
                    input id="bpf-token" type="hidden" name="token" value="";
                    (btn_primary("Push to GitHub"))
                }
                form id="backup-pull-form"
                    hx-post="/settings/backup/pull"
                    hx-swap="none"
                    hx-confirm="Restore config from GitHub? This overwrites current data."
                    class="flex gap-2" {
                    input type="hidden" name="_method" value="pull";
                    input id="bplf-repo"  type="hidden" name="repo"  value=(repo_val);
                    input id="bplf-token" type="hidden" name="token" value="";
                    (btn_danger("Pull from GitHub"))
                }
            }
            // Sync shared field values before submit
            script { r#"
(function(){
  function syncBackupForms(){
    var repo  = document.getElementById('backup-repo')?.value  || '';
    var token = document.getElementById('backup-token')?.value || '';
    ['bpf-repo','bplf-repo'].forEach(function(id){ var el=document.getElementById(id); if(el) el.value=repo; });
    ['bpf-token','bplf-token'].forEach(function(id){ var el=document.getElementById(id); if(el) el.value=token; });
  }
  document.getElementById('backup-push-form')?.addEventListener('htmx:beforeRequest', syncBackupForms);
  document.getElementById('backup-pull-form')?.addEventListener('htmx:beforeRequest', syncBackupForms);
})();
            "# }
        }
    })
}

// ─── Backup handlers ─────────────────────────────────────────────────────────

async fn backup_snapshot(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    tokio::task::spawn_blocking(move || {
        match aiem_core::backup::snapshot_local() {
            Ok(p)  => toast_info(&tx, format!("Snapshot saved: {}", p.display())),
            Err(e) => toast_error(&tx, format!("Snapshot failed: {e}")),
        }
    });
    ok()
}

#[derive(Deserialize)]
struct ExportForm { dest: String }
async fn backup_export(State(st): State<AppState>, Form(f): Form<ExportForm>) -> Response {
    let tx = st.events.clone();
    tokio::task::spawn_blocking(move || {
        let dest = std::path::PathBuf::from(f.dest.trim());
        match aiem_core::backup::export_to_dir(&dest) {
            Ok(files) => toast_info(&tx, format!("Exported {} file(s) to {}", files.len(), dest.display())),
            Err(e)    => toast_error(&tx, format!("Export failed: {e}")),
        }
    });
    ok()
}

#[derive(Deserialize)]
struct ImportForm { src: String }
async fn backup_import(State(st): State<AppState>, Form(f): Form<ImportForm>) -> Response {
    let tx = st.events.clone();
    tokio::task::spawn_blocking(move || {
        let src = std::path::PathBuf::from(f.src.trim());
        match aiem_core::backup::import_from_dir(&src) {
            Ok(files) => toast_info(&tx, format!("Restored {} file(s)", files.len())),
            Err(e)    => toast_error(&tx, format!("Restore failed: {e}")),
        }
    });
    ok()
}

#[derive(Deserialize)]
struct GithubBackupForm {
    repo:  String,
    #[serde(default)]
    token: String,
}
async fn backup_push(State(st): State<AppState>, Form(f): Form<GithubBackupForm>) -> Response {
    let tx = st.events.clone();
    tokio::task::spawn_blocking(move || {
        let tok = if f.token.trim().is_empty() { None } else { Some(f.token.trim().to_owned()) };
        match aiem_core::backup::push_github(f.repo.trim(), tok.as_deref()) {
            Ok(())  => toast_info(&tx, format!("Pushed to {}", f.repo.trim())),
            Err(e)  => toast_error(&tx, format!("Push failed: {e}")),
        }
    });
    ok()
}

async fn backup_pull(State(st): State<AppState>, Form(f): Form<GithubBackupForm>) -> Response {
    let tx = st.events.clone();
    tokio::task::spawn_blocking(move || {
        let tok = if f.token.trim().is_empty() { None } else { Some(f.token.trim().to_owned()) };
        match aiem_core::backup::pull_github(f.repo.trim(), tok.as_deref()) {
            Ok(())  => toast_info(&tx, format!("Restored from {}", f.repo.trim())),
            Err(e)  => toast_error(&tx, format!("Pull failed: {e}")),
        }
    });
    ok()
}

#[derive(Deserialize)]
struct IntervalForm { interval: String }
async fn backup_set_interval(State(_st): State<AppState>, Form(f): Form<IntervalForm>) -> Response {
    let interval = match f.interval.as_str() {
        "daily"  => AutoInterval::Daily,
        "weekly" => AutoInterval::Weekly,
        _        => AutoInterval::Never,
    };
    match BackupConfig::load() {
        Ok(mut cfg) => {
            cfg.auto_interval = interval;
            let _ = cfg.save();
        }
        Err(_) => {}
    }
    // Redirect back to settings to reflect the updated selection.
    axum::response::Redirect::to("/settings").into_response()
}
