use axum::extract::{Form, Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::backup::{AutoInterval, BackupConfig};
use aiem_core::secrets::Vault;

use crate::events::ResourceKind;
use crate::layout::{
    btn_danger, btn_primary, card, empty_state, page, page_header, settings_group, settings_row,
    tag, TagKind,
};
use crate::state::AppState;
use crate::tasks::{invalidate, task_finished, task_started, toast_error, toast_info};

const GH_TOKEN_NAME: &str = "github_token";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(index))
        .route("/settings/github-token", post(save_token))
        .route("/settings/github-token/clear", post(clear_token))
        .route("/settings/backup/save-config", post(backup_save_config))
        .route("/settings/backup/snapshot", post(backup_snapshot))
        .route("/settings/backup/export", post(backup_export))
        .route("/settings/backup/import", post(backup_import))
        .route("/settings/backup/push", post(backup_push))
        .route("/settings/backup/pull", post(backup_pull))
        .route("/settings/backup/interval", post(backup_set_interval))
        .route("/settings/trash", get(trash_page))
        .route("/settings/trash/empty", post(trash_empty))
        .route("/settings/trash/:name/delete", post(trash_delete_entry))
}

async fn index(State(_st): State<AppState>) -> Markup {
    let has_token = std::env::var("GITHUB_TOKEN")
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let aiem_home = aiem_core::paths::home()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into());
    let backup_cfg = BackupConfig::load().unwrap_or_default();
    let last_backup = backup_cfg
        .last_backup_ts
        .map(aiem_core::backup::time_ago)
        .unwrap_or_else(|| "never".into());

    page(
        "Settings",
        "/settings",
        html! {
            (page_header("Settings", "", html!{}))
            div class="content-padding wide-content" {
                (settings_group("GitHub Token", html! {
                    (settings_row(
                        "Personal Access Token",
                        "Stored in the OS keyring. Avoids 60-req/h anonymous rate limit.",
                        html! {
                            @if has_token { (tag("configured", TagKind::Success)) }
                            @else { span class="meta" { "not set" } }
                        },
                    ))
                    div style="padding:12px 16px;border-top:1px solid var(--stroke-light)" {
                        form hx-post="/settings/github-token" hx-swap="none" class="flex gap-2 items-end" {
                            input name="token" type="password" placeholder="ghp_..." required class="field" style="flex:1";
                            (btn_primary("Save"))
                        }
                        form hx-post="/settings/github-token/clear" hx-swap="none" hx-confirm="Clear stored token?" style="margin-top:8px" {
                            (btn_danger("Clear"))
                        }
                    }
                }))

                (backup_card(&backup_cfg, &last_backup))

                (settings_group("Host Info", html! {
                    (settings_row("AIEM_HOME", "", html! { span class="mono" { (aiem_home) } }))
                    (settings_row("Hostname", "", html! { span class="mono" { (hostname()) } }))
                    (settings_row("User", "", html! { span class="mono" { (whoami()) } }))
                    (settings_row("OS", "", html! { span class="mono" { (std::env::consts::OS) " / " (std::env::consts::ARCH) } }))
                }))

                (settings_group("Trash", html! {
                    (settings_row(
                        "Removed content",
                        "Items are moved to a local trash folder instead of being hard-deleted.",
                        html! { a href="/settings/trash" class="btn-secondary" style="text-decoration:none" { "Open trash" } },
                    ))
                }))

                (settings_group("About", html! {
                    (settings_row(
                        "Version",
                        "aiem-web — headless management for skills & MCP across IDEs.",
                        html! { span class="mono" { (env!("CARGO_PKG_VERSION")) } },
                    ))
                }))
            }
        },
    )
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
struct TokenForm {
    token: String,
}

async fn save_token(State(st): State<AppState>, Form(f): Form<TokenForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;
    let mut vault = match Vault::load() {
        Ok(v) => v,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    match vault.set(
        GH_TOKEN_NAME,
        &f.token,
        Some("GitHub Personal Access Token".into()),
    ) {
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
    let mut vault = match Vault::load() {
        Ok(v) => v,
        Err(e) => {
            toast_error(&tx, format!("{e}"));
            return ok();
        }
    };
    match vault.delete(GH_TOKEN_NAME) {
        Ok(()) => {
            std::env::remove_var("GITHUB_TOKEN");
            toast_info(&tx, "cleared");
        }
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    ok()
}

fn ok() -> Response {
    (axum::http::StatusCode::OK, "ok").into_response()
}

// ─── Backup card ─────────────────────────────────────────────────────────────

fn backup_card(cfg: &BackupConfig, last_backup: &str) -> Markup {
    let repo_val = cfg.github_repo.as_deref().unwrap_or("");
    let has_token = std::env::var("GITHUB_TOKEN")
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        || aiem_core::backup::load_backup_token_file().is_some();
    card(html! {
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
                "Commits skills, MCP, projects, and local skill content to a private GitHub repo. "
                "Uses " code class="mono" { "~/.aiem/backup-git/" } " as git working tree. Requires " code class="mono" { "git" } " in PATH."
            }

            // Save repo + token (persistent)
            form hx-post="/settings/backup/save-config" hx-swap="none" class="mb-3" {
                p class="meta text-xs mb-1" { "Repo URL (HTTPS)" }
                input name="repo" type="text"
                    value=(repo_val)
                    placeholder="https://github.com/you/my-aiem-backup"
                    class="field w-full mb-2";
                p class="meta text-xs mb-1" {
                    "GitHub Token (saved to OS keyring as " code class="mono" { "github_token" } ")"
                    @if has_token { " " (tag("● saved", TagKind::Success)) }
                }
                input name="token" type="password"
                    placeholder="ghp_... (leave blank to keep existing)"
                    class="field w-full mb-2";
                p class="meta text-xs mb-1" {
                    "Proxy (optional, for GitHub access, e.g. "
                    code class="mono" { "socks5h://127.0.0.1:1080" }
                    ")"
                }
                input name="proxy" type="text"
                    value=(cfg.http_proxy.as_deref().unwrap_or(""))
                    placeholder="socks5h://127.0.0.1:1080"
                    class="field w-full mb-2";
                div class="flex gap-2" {
                    (btn_primary("Save config"))
                }
            }

            // Push / Pull — use saved config, no fields needed
            div class="flex gap-2" {
                form hx-post="/settings/backup/push" hx-swap="none" class="flex gap-2" {
                    (btn_primary("Push to GitHub"))
                }
                form hx-post="/settings/backup/pull"
                    hx-swap="none"
                    hx-confirm="Restore config from GitHub? This overwrites current data."
                    class="flex gap-2" {
                    (btn_danger("Pull from GitHub"))
                }
            }
            p class="meta text-xs mt-2" {
                "Push/Pull use the saved repo URL and token. Click " b { "Save config" } " first to persist them."
            }
        }
    })
}

// ─── Backup handlers ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SaveConfigForm {
    repo: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    proxy: String,
}

/// Persist the GitHub backup repo URL and (optionally) the PAT.
/// Token is saved to the OS keyring under `github_token` (same slot used by
/// the top-level GitHub Token form). Empty token leaves the existing one
/// untouched so the user can update the URL without re-typing the secret.
async fn backup_save_config(State(st): State<AppState>, Form(f): Form<SaveConfigForm>) -> Response {
    let tx = st.events.clone();
    let _g = st.write_lock.lock().await;

    let repo = f.repo.trim().to_string();
    let token = f.token.trim().to_string();

    match BackupConfig::load() {
        Ok(mut cfg) => {
            cfg.github_repo = if repo.is_empty() {
                None
            } else {
                Some(repo.clone())
            };
            cfg.http_proxy = if f.proxy.trim().is_empty() {
                None
            } else {
                Some(f.proxy.trim().to_string())
            };
            if let Err(e) = cfg.save() {
                toast_error(&tx, format!("save config: {e}"));
                return ok();
            }
        }
        Err(e) => {
            toast_error(&tx, format!("load config: {e}"));
            return ok();
        }
    }

    if !token.is_empty() {
        // Write the filesystem fallback first so persistence is guaranteed
        // even when the OS keyring is unavailable (e.g. Linux systemd user
        // services whose session keyring does not survive restart).
        if let Err(e) = aiem_core::backup::save_backup_token_file(&token) {
            toast_error(&tx, format!("token file: {e}"));
            return ok();
        }
        // Best-effort keyring storage: failures here don't block persistence.
        match Vault::load() {
            Ok(mut vault) => {
                if let Err(e) = vault.set(
                    GH_TOKEN_NAME,
                    &token,
                    Some("GitHub Personal Access Token".into()),
                ) {
                    tracing::warn!("keyring set failed (using file fallback): {e}");
                }
            }
            Err(e) => {
                tracing::warn!("vault load failed (using file fallback): {e}");
            }
        }
        std::env::set_var("GITHUB_TOKEN", &token);
        toast_info(&tx, "Backup config and token saved");
    } else {
        toast_info(&tx, "Backup config saved");
    }
    ok()
}

async fn backup_snapshot(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let id = st.next_task_id().await;
    tokio::task::spawn_blocking(move || {
        task_started(&tx, id, "Taking local snapshot");
        match aiem_core::backup::snapshot_local() {
            Ok(p) => task_finished(&tx, id, true, format!("Snapshot saved: {}", p.display())),
            Err(e) => task_finished(&tx, id, false, format!("Snapshot failed: {e}")),
        }
    });
    ok()
}

#[derive(Deserialize)]
struct ExportForm {
    dest: String,
}
async fn backup_export(State(st): State<AppState>, Form(f): Form<ExportForm>) -> Response {
    let tx = st.events.clone();
    let id = st.next_task_id().await;
    tokio::task::spawn_blocking(move || {
        let dest = std::path::PathBuf::from(f.dest.trim());
        task_started(&tx, id, format!("Exporting to {}", dest.display()));
        match aiem_core::backup::export_to_dir(&dest) {
            Ok(files) => task_finished(
                &tx,
                id,
                true,
                format!("Exported {} file(s) to {}", files.len(), dest.display()),
            ),
            Err(e) => task_finished(&tx, id, false, format!("Export failed: {e}")),
        }
    });
    ok()
}

#[derive(Deserialize)]
struct ImportForm {
    src: String,
}
async fn backup_import(State(st): State<AppState>, Form(f): Form<ImportForm>) -> Response {
    let tx = st.events.clone();
    let id = st.next_task_id().await;
    tokio::task::spawn_blocking(move || {
        let src = std::path::PathBuf::from(f.src.trim());
        task_started(&tx, id, format!("Restoring from {}", src.display()));
        match aiem_core::backup::import_from_dir(&src) {
            Ok(files) => {
                task_finished(&tx, id, true, format!("Restored {} file(s)", files.len()));
                invalidate(&tx, ResourceKind::Skills);
                invalidate(&tx, ResourceKind::Mcp);
                invalidate(&tx, ResourceKind::Projects);
            }
            Err(e) => task_finished(&tx, id, false, format!("Restore failed: {e}")),
        }
    });
    ok()
}

async fn backup_push(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let id = st.next_task_id().await;
    tokio::task::spawn_blocking(move || {
        let (repo, token) = match load_backup_target() {
            Ok(v) => v,
            Err(e) => {
                task_finished(&tx, id, false, e);
                return;
            }
        };
        task_started(&tx, id, format!("Pushing to {}", repo));
        match aiem_core::backup::push_github(&repo, token.as_deref()) {
            Ok(()) => task_finished(&tx, id, true, format!("Pushed to {}", repo)),
            Err(e) => task_finished(&tx, id, false, format!("Push failed: {e}")),
        }
    });
    ok()
}

async fn backup_pull(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let id = st.next_task_id().await;
    tokio::task::spawn_blocking(move || {
        let (repo, token) = match load_backup_target() {
            Ok(v) => v,
            Err(e) => {
                task_finished(&tx, id, false, e);
                return;
            }
        };
        task_started(&tx, id, format!("Pulling from {}", repo));
        match aiem_core::backup::pull_github(&repo, token.as_deref()) {
            Ok(()) => {
                task_finished(&tx, id, true, format!("Restored from {}", repo));
                invalidate(&tx, ResourceKind::Skills);
                invalidate(&tx, ResourceKind::Mcp);
                invalidate(&tx, ResourceKind::Projects);
            }
            Err(e) => task_finished(&tx, id, false, format!("Pull failed: {e}")),
        }
    });
    ok()
}

/// Load the saved repo URL + effective token for push/pull.
/// Token resolution order: `GITHUB_TOKEN` env var → OS keyring slot
/// (`github_token`).  Reading the keyring here means the token survives
/// process restarts (e.g. when the systemd service is restarted after
/// the user saved it earlier).
fn load_backup_target() -> std::result::Result<(String, Option<String>), String> {
    let cfg = BackupConfig::load().map_err(|e| format!("load config: {e}"))?;
    let repo = cfg.github_repo.unwrap_or_default();
    if repo.trim().is_empty() {
        return Err(
            "No backup repo URL configured. Fill the form and click 'Save config' first."
                .to_string(),
        );
    }
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| Vault::load().ok().and_then(|v| v.get(GH_TOKEN_NAME).ok()))
        .filter(|s| !s.is_empty())
        .or_else(aiem_core::backup::load_backup_token_file);
    Ok((repo, token))
}

#[derive(Deserialize)]
struct IntervalForm {
    interval: String,
}
async fn backup_set_interval(State(_st): State<AppState>, Form(f): Form<IntervalForm>) -> Response {
    let interval = match f.interval.as_str() {
        "daily" => AutoInterval::Daily,
        "weekly" => AutoInterval::Weekly,
        _ => AutoInterval::Never,
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

// ─── Trash ───────────────────────────────────────────────────────────────────

fn list_trash_entries() -> Vec<(String, std::path::PathBuf)> {
    let Ok(trash) = aiem_core::paths::trash_dir() else {
        return vec![];
    };
    if !trash.exists() {
        return vec![];
    }
    let mut out: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(&trash)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            Some((name, e.path()))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

async fn trash_page(State(_st): State<AppState>) -> Markup {
    let entries = list_trash_entries();
    let trash_dir = aiem_core::paths::trash_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let body = html! {
        (page_header("Trash", "Items removed from aiem's managed content. Delete entries here to reclaim disk space.", html!{}))

        @if entries.is_empty() {
            (empty_state("Trash is empty", &format!("Location: {trash_dir}")))
        } @else {
            div class="row" style="margin-bottom: 12px; gap: 8px;" {
                form method="post" action="/settings/trash/empty" hx-post="/settings/trash/empty" hx-swap="none" {
                    (btn_danger("Empty trash"))
                }
                span class="muted" style="align-self: center;" {
                    (format!("{} entries • {}", entries.len(), trash_dir))
                }
            }
            (card(html! {
                table class="table" style="width: 100%;" {
                    thead { tr { th { "Name" } th { "Path" } th { "Actions" } } }
                    tbody {
                        @for (name, path) in &entries {
                            tr {
                                td { code { (name) } }
                                td class="muted" style="font-size: 12px;" { (path.display().to_string()) }
                                td {
                                    form method="post"
                                         action={"/settings/trash/" (urlencoding::encode(name)) "/delete"}
                                         hx-post={"/settings/trash/" (urlencoding::encode(name)) "/delete"}
                                         hx-swap="none" style="display: inline;" {
                                        (btn_danger("Delete"))
                                    }
                                }
                            }
                        }
                    }
                }
            }))
        }
    };
    page("Trash — aiem", "/settings", body)
}

async fn trash_empty(State(st): State<AppState>) -> Response {
    let tx = st.events.clone();
    let Ok(trash) = aiem_core::paths::trash_dir() else {
        toast_error(&tx, "no trash dir");
        return axum::response::Redirect::to("/settings/trash").into_response();
    };
    let mut removed = 0usize;
    if trash.exists() {
        if let Ok(entries) = std::fs::read_dir(&trash) {
            for e in entries.flatten() {
                if aiem_core::fs_util::remove_path(&e.path()).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    toast_info(&tx, format!("deleted {removed} trash entries"));
    axum::response::Redirect::to("/settings/trash").into_response()
}

async fn trash_delete_entry(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let tx = st.events.clone();
    let Ok(trash) = aiem_core::paths::trash_dir() else {
        toast_error(&tx, "no trash dir");
        return axum::response::Redirect::to("/settings/trash").into_response();
    };
    // Guard against path traversal.
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        toast_error(&tx, "invalid name");
        return axum::response::Redirect::to("/settings/trash").into_response();
    }
    let target = trash.join(&name);
    if !target.exists() {
        toast_error(&tx, "not found");
        return axum::response::Redirect::to("/settings/trash").into_response();
    }
    match aiem_core::fs_util::remove_path(&target) {
        Ok(_) => toast_info(&tx, format!("deleted `{name}`")),
        Err(e) => toast_error(&tx, format!("{e}")),
    }
    axum::response::Redirect::to("/settings/trash").into_response()
}
