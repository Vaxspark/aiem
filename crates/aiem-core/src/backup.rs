//! Backup and restore of aiem config files (skills index + MCP servers).
//!
//! Two backends are supported:
//!
//! * **Local snapshot** – copies the three JSON config files into a
//!   timestamped directory under `~/.aiem/snapshots/<ts>/`.
//!
//! * **GitHub** – maintains a git working tree at `~/.aiem/backup-git/`,
//!   commits the three files, and pushes to a user-supplied HTTPS GitHub
//!   repository.  A PAT is embedded in the remote URL so that no interactive
//!   auth is needed.  Falls back to the `GITHUB_TOKEN` env var when no token
//!   is passed explicitly.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::skills::{SkillIndex, SkillSource};
use crate::{paths, Error, Result};

// ─── Config ─────────────────────────────────────────────────────────────────

/// How often the GUI should auto-trigger a backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoInterval {
    #[default]
    Never,
    Daily,
    Weekly,
}

impl AutoInterval {
    pub fn as_secs(self) -> Option<u64> {
        match self {
            AutoInterval::Never => None,
            AutoInterval::Daily => Some(86_400),
            AutoInterval::Weekly => Some(604_800),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            AutoInterval::Never => "Never",
            AutoInterval::Daily => "Daily",
            AutoInterval::Weekly => "Weekly",
        }
    }
}

/// Persisted backup preferences (`~/.aiem/backup.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackupConfig {
    /// HTTPS GitHub repo URL, e.g. `https://github.com/user/my-aiem-backup`.
    pub github_repo: Option<String>,
    /// How often the GUI auto-runs a snapshot.
    #[serde(default)]
    pub auto_interval: AutoInterval,
    /// Unix timestamp (seconds) of the last successful backup of any kind.
    #[serde(default)]
    pub last_backup_ts: Option<u64>,
    /// Optional HTTP/HTTPS proxy for git network operations,
    /// e.g. `http://127.0.0.1:7890` or `socks5://127.0.0.1:1080`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<String>,
}

impl BackupConfig {
    pub fn load() -> Result<Self> {
        let p = paths::backup_config_file()?;
        if !p.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(&p)?;
        // If the stored JSON is missing fields (e.g. old format or manually
        // reset to `{}`), fall back to defaults rather than propagating a
        // parse error that would silently block all backup operations.
        Ok(serde_json::from_str(&s).unwrap_or_default())
    }

    pub fn save(&self) -> Result<()> {
        let p = paths::backup_config_file()?;
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// `true` when the configured interval has elapsed since the last backup.
    pub fn is_due(&self) -> bool {
        let Some(interval_secs) = self.auto_interval.as_secs() else {
            return false;
        };
        let now = now_secs();
        match self.last_backup_ts {
            None => true,
            Some(last) => now.saturating_sub(last) >= interval_secs,
        }
    }
}

/// Save the GitHub backup token to `~/.aiem/.backup-token` with `0600` perms
/// on Unix.  This is a fallback for environments where the OS keyring cannot
/// persist secrets across process restarts (notably Linux systemd user
/// services using the session keyring).
pub fn save_backup_token_file(token: &str) -> Result<()> {
    let p = paths::backup_token_file()?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Load the GitHub backup token from `~/.aiem/.backup-token` if present.
pub fn load_backup_token_file() -> Option<String> {
    let p = paths::backup_token_file().ok()?;
    let s = std::fs::read_to_string(&p).ok()?;
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// Delete the fallback token file if present.
pub fn delete_backup_token_file() -> Result<()> {
    let p = paths::backup_token_file()?;
    if p.exists() {
        std::fs::remove_file(&p)?;
    }
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// The config files we back up, together with their archive names.
fn config_file_map() -> Result<Vec<(PathBuf, &'static str)>> {
    Ok(vec![
        (paths::skills_dir()?.join("index.json"), "skills_index.json"),
        (paths::mcp_dir()?.join("servers.json"), "mcp_servers.json"),
        (paths::projects_file()?, "projects.json"),
        (paths::secrets_index_file()?, "secrets.json"),
    ])
}

/// Load the skill index (returns empty index if file is missing).
fn load_skill_index() -> Result<SkillIndex> {
    let p = paths::skills_dir()?.join("index.json");
    if !p.exists() {
        return Ok(SkillIndex::default());
    }
    let data = std::fs::read(&p)?;
    Ok(serde_json::from_slice(crate::fs_util::strip_utf8_bom(
        &data,
    ))?)
}

/// Copy every `SkillSource::Local` skill directory into `dest_dir/custom_skills/<id>/`.
/// Silently skips skills whose `path` no longer exists on disk.
fn copy_local_skills_to_dir(dest_dir: &Path) -> Result<()> {
    let index = load_skill_index()?;
    let custom_dir = dest_dir.join("custom_skills");
    if custom_dir.exists() {
        crate::fs_util::remove_path(&custom_dir)?;
    }
    for (id, skill) in &index.skills {
        let SkillSource::Local { path } = &skill.source else {
            continue;
        };
        if !path.exists() {
            continue;
        }
        copy_dir_all(path, &custom_dir.join(id))?;
    }
    Ok(())
}

/// Copy every installed skill directory into `dest_dir/skill_contents/<id>/`.
///
/// `custom_skills/` is kept for backwards compatibility with older backups,
/// but `skill_contents/` makes GitHub-sourced skills portable too.  Without
/// this, a restored Linux server may keep Windows absolute paths in
/// `skills_index.json` and later deploy broken project links.
fn copy_skill_contents_to_dir(dest_dir: &Path) -> Result<()> {
    let index = load_skill_index()?;
    let contents_dir = dest_dir.join("skill_contents");
    if contents_dir.exists() {
        crate::fs_util::remove_path(&contents_dir)?;
    }
    for (id, skill) in &index.skills {
        let Some(source_dir) = exportable_skill_dir(id, skill)? else {
            continue;
        };
        copy_dir_all(&source_dir, &contents_dir.join(id))?;
    }
    Ok(())
}

fn exportable_skill_dir(id: &str, skill: &crate::skills::Skill) -> Result<Option<PathBuf>> {
    for candidate in [skill.path.clone(), paths::skills_dir()?.join(id)] {
        if candidate.is_dir()
            && crate::skills::github::ensure_canonical_skill_manifest(&candidate).is_ok()
        {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// Recursively copy a directory tree from `src` to `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    // Do not follow symlinks — avoids pulling arbitrary paths into backups.
    for entry in walkdir::WalkDir::new(src).follow_links(false).min_depth(1) {
        let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

// ─── ZIP export ──────────────────────────────────────────────────────────────

/// Export a self-contained zip archive to `dest`.
///
/// The archive contains:
/// * `skills_index.json` — the skill registry
/// * `mcp_servers.json`  — the MCP server list
/// * `custom_skills/<id>/…` — full on-disk content of every locally-sourced skill
///
/// This is the recommended backup path when no GitHub repo is configured.
pub fn export_zip(dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(dest)
        .map_err(|e| Error::Invalid(format!("cannot create zip {}: {e}", dest.display())))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Config files.
    for (src, name) in config_file_map()? {
        if src.exists() {
            zip.start_file(name, opts)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let data = std::fs::read(&src)?;
            zip.write_all(&data)?;
        }
    }

    // Installed skill directories.
    let index = load_skill_index()?;
    for (id, skill) in &index.skills {
        let Some(source_dir) = exportable_skill_dir(id, skill)? else {
            continue;
        };
        add_dir_to_zip(&mut zip, &source_dir, &format!("skill_contents/{id}"), opts)?;
        if matches!(skill.source, SkillSource::Local { .. }) {
            add_dir_to_zip(&mut zip, &source_dir, &format!("custom_skills/{id}"), opts)?;
        }
    }

    zip.finish().map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(())
}

/// Append a directory tree into an open `ZipWriter` under `zip_prefix`.
fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    src_dir: &Path,
    zip_prefix: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<()> {
    for entry in walkdir::WalkDir::new(src_dir).follow_links(true) {
        let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
        let rel = entry
            .path()
            .strip_prefix(src_dir)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let zip_path = if rel.as_os_str().is_empty() {
            zip_prefix.to_string()
        } else {
            format!("{zip_prefix}/{}", rel.to_string_lossy().replace('\\', "/"))
        };
        if entry.file_type().is_dir() {
            if !rel.as_os_str().is_empty() {
                zip.add_directory(&zip_path, opts)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
            }
        } else {
            zip.start_file(&zip_path, opts)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let data = std::fs::read(entry.path())?;
            zip.write_all(&data)?;
        }
    }
    Ok(())
}

// ─── Local snapshot ──────────────────────────────────────────────────────────

/// Copy config files into `dest_dir/`, creating it if necessary.
/// Only copies files that exist; returns the list of destination paths written.
pub fn export_to_dir(dest_dir: &Path) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dest_dir)?;
    let mut copied = Vec::new();
    for (src, name) in config_file_map()? {
        if src.exists() {
            let dst = dest_dir.join(name);
            std::fs::copy(&src, &dst)?;
            copied.push(dst);
        }
    }
    // Also snapshot every MCP bundle directory so user-made Python/Node
    // scripts travel with the backup.
    let bundles_src = paths::mcp_bundles_dir()?;
    if bundles_src.exists() {
        let bundles_dst = dest_dir.join("mcp_bundles");
        // Wipe any stale copy so deletions propagate.
        if bundles_dst.exists() {
            let _ = std::fs::remove_dir_all(&bundles_dst);
        }
        for entry in std::fs::read_dir(&bundles_src)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            copy_dir_all(&entry.path(), &bundles_dst.join(&name))?;
            copied.push(bundles_dst.join(&name));
        }
    }
    copy_skill_contents_to_dir(dest_dir)?;
    let contents_dst = dest_dir.join("skill_contents");
    if contents_dst.exists() {
        copied.push(contents_dst);
    }
    copy_local_skills_to_dir(dest_dir)?;
    let custom_dst = dest_dir.join("custom_skills");
    if custom_dst.exists() {
        copied.push(custom_dst);
    }
    Ok(copied)
}

/// Restore config files from `src_dir/`, overwriting current config.
/// Only restores files present in the snapshot; returns restored paths.
///
/// Automatically creates a safety snapshot of the current state before
/// overwriting, so the user can revert if the restore goes wrong.
/// The pre-restore snapshot path (if any) is logged via `tracing::info`.
pub fn import_from_dir(src_dir: &Path) -> Result<Vec<PathBuf>> {
    auto_safety_snapshot("pre-restore");
    let mut restored = Vec::new();
    for (dst, name) in config_file_map()? {
        let src = src_dir.join(name);
        if src.exists() {
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&src, &dst)?;
            restored.push(dst);
        }
    }
    // Restore bundle directories.
    let bundles_src = src_dir.join("mcp_bundles");
    if bundles_src.is_dir() {
        let bundles_dst = paths::mcp_bundles_dir()?;
        std::fs::create_dir_all(&bundles_dst)?;
        for entry in std::fs::read_dir(&bundles_src)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let dst = bundles_dst.join(&name);
            if dst.exists() {
                let _ = std::fs::remove_dir_all(&dst);
            }
            copy_dir_all(&entry.path(), &dst)?;
            restored.push(dst);
        }
    }
    restored.extend(restore_skill_contents_from_dir(src_dir)?);
    normalize_restored_skill_index()?;
    Ok(restored)
}

/// Restore installed skill directories from modern `skill_contents/<id>/` and
/// legacy `custom_skills/<id>/` backups.
fn restore_skill_contents_from_dir(src_dir: &Path) -> Result<Vec<PathBuf>> {
    let source_roots = [
        src_dir.join("skill_contents"),
        src_dir.join("custom_skills"),
    ];
    if !source_roots.iter().any(|p| p.is_dir()) {
        return Ok(Vec::new());
    }

    let mut restored = Vec::new();
    for source_root in source_roots.iter().filter(|p| p.is_dir()) {
        for entry in std::fs::read_dir(source_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            let dst = paths::skills_dir()?.join(&id);
            if dst.exists() {
                crate::fs_util::remove_path(&dst)?;
            }
            copy_dir_all(&entry.path(), &dst)?;
            restored.push(dst);
        }
    }
    Ok(restored)
}

fn normalize_restored_skill_index() -> Result<()> {
    let index_path = paths::skills_dir()?.join("index.json");
    if !index_path.exists() {
        return Ok(());
    }
    let mut reg = crate::skills::SkillRegistry::load()?;
    let ids: Vec<String> = reg.list().map(|skill| skill.id.clone()).collect();
    for id in ids {
        let Some(skill) = reg.get_mut(&id) else {
            continue;
        };
        let local = paths::skills_dir()?.join(&id);
        if local.is_dir() && crate::skills::github::ensure_canonical_skill_manifest(&local).is_ok()
        {
            skill.path = local.clone();
            if matches!(skill.source, SkillSource::Local { .. }) {
                skill.source = SkillSource::Local { path: local };
            }
        } else if is_foreign_windows_path(&skill.path)
            && skill.path.is_dir()
            && crate::skills::github::ensure_canonical_skill_manifest(&skill.path).is_ok()
        {
            if local.exists() || crate::fs_util::is_link(&local) {
                crate::fs_util::remove_path(&local)?;
            }
            copy_dir_all(&skill.path, &local)?;
            skill.path = local.clone();
            if matches!(skill.source, SkillSource::Local { .. }) {
                skill.source = SkillSource::Local { path: local };
            }
        } else if !skill.path.exists() {
            skill.path = local;
        }
        for roots in skill.deployments.values_mut() {
            roots.retain(|root| root == "~" || Path::new(root).is_dir());
        }
        skill.deployments.retain(|_, roots| !roots.is_empty());
    }
    reg.save()?;
    Ok(())
}

#[cfg(unix)]
fn is_foreign_windows_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(":\\") || s.contains(":/")
}

#[cfg(not(unix))]
fn is_foreign_windows_path(_path: &Path) -> bool {
    false
}

/// Create a timestamped local snapshot in `~/.aiem/snapshots/<unix_ts>/`.
/// Updates `BackupConfig::last_backup_ts`.
/// Returns the snapshot directory path.
pub fn snapshot_local() -> Result<PathBuf> {
    let ts = now_secs();
    let dest = paths::snapshots_dir()?.join(ts.to_string());
    export_to_dir(&dest)?;
    update_last_ts(ts)?;
    Ok(dest)
}

/// List existing local snapshots (sorted newest-first).
pub fn list_snapshots() -> Result<Vec<PathBuf>> {
    let dir = paths::snapshots_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Sort by directory name (which is a unix timestamp) descending.
    entries.sort_by(|a, b| {
        let ta = ts_from_path(a);
        let tb = ts_from_path(b);
        tb.cmp(&ta)
    });
    Ok(entries)
}

fn ts_from_path(p: &Path) -> u64 {
    p.file_name()
        .and_then(|n| n.to_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

// ─── GitHub git push / pull ───────────────────────────────────────────────────

/// Commit the current config files and push to a GitHub repository.
///
/// **Multi-machine safe**: before pushing, this function always fetches and
/// rebases onto `origin/main` so that a remote instance's commits are
/// preserved.  Conflict resolution is "ours-wins" — if the rebase cannot
/// apply cleanly, we abort the rebase and fall back to a force-push of the
/// local state (last-writer-wins, acceptable for a single-user tool).
///
/// * `repo_url` – HTTPS URL, e.g. `https://github.com/user/my-backup`.
/// * `token`    – Optional PAT; falls back to `GITHUB_TOKEN` env var.
///
/// Uses `~/.aiem/backup-git/` as the git working tree.
pub fn push_github(repo_url: &str, token: Option<&str>) -> Result<()> {
    let auth_url = build_auth_url(repo_url, token)?;
    let work_dir = paths::backup_git_dir()?;
    std::fs::create_dir_all(&work_dir)?;

    // Read saved proxy setting.
    let proxy_owned = BackupConfig::load().ok().and_then(|c| c.http_proxy);
    let proxy = proxy_owned.as_deref().filter(|s| !s.is_empty());

    // Init repo on first run.
    if !work_dir.join(".git").exists() {
        git_init_main(&work_dir)?;
    }

    // Export config files into the working tree.
    export_to_dir(&work_dir)?;
    // Also copy full directories of every locally-sourced (custom) skill.
    copy_local_skills_to_dir(&work_dir)?;

    // Always write a README so there is always at least one file to commit,
    // even on a fresh machine with no skills/MCP servers configured yet.
    let readme = work_dir.join("README.md");
    if !readme.exists() {
        let _ = std::fs::write(
            &readme,
            "# aiem backup\n\nAuto-generated by [aiem](https://github.com/Vaxspark/aiem). \
             Do not edit by hand — contents are regenerated on every push.\n",
        );
    }

    // Upsert remote.
    let remotes = git_run(&work_dir, &["remote"], None)?;
    if remotes.lines().any(|l| l.trim() == "origin") {
        git_run(&work_dir, &["remote", "set-url", "origin", &auth_url], None)?;
    } else {
        git_run(&work_dir, &["remote", "add", "origin", &auth_url], None)?;
    }
    let branch = remote_default_branch(&work_dir, proxy).unwrap_or_else(|| "main".to_string());
    let push_ref = format!("HEAD:{branch}");

    let _ = git_run(
        &work_dir,
        &["config", "user.email", "aiem-backup@localhost"],
        None,
    );
    let _ = git_run(&work_dir, &["config", "user.name", "aiem"], None);

    git_run(&work_dir, &["add", "."], None)?;

    // Only commit when there is something new.
    let status = git_run(&work_dir, &["status", "--porcelain"], None)?;
    if !status.trim().is_empty() {
        let host = hostname();
        let msg = format!("aiem backup {} ({})", now_secs(), host);
        git_run(&work_dir, &["commit", "-m", &msg], None)?;
    }

    // Try to fetch remote history and rebase local commits on top so we
    // don't lose commits made from another machine (e.g. the remote server).
    let fetch_ok = git_run_network(&work_dir, &["fetch", "origin", &branch], proxy).is_ok();
    if fetch_ok {
        // Check if the remote branch exists (may be an empty / brand-new repo).
        let remote_ref = format!("origin/{branch}");
        let has_remote = git_run(&work_dir, &["rev-parse", "--verify", &remote_ref], None).is_ok();
        if has_remote {
            let rebase_ok = git_run(&work_dir, &["rebase", &remote_ref], None).is_ok();
            if !rebase_ok {
                // Rebase conflict — abort and fall back to force.
                let _ = git_run(&work_dir, &["rebase", "--abort"], None);
                git_run_network(
                    &work_dir,
                    &["push", "--set-upstream", "origin", &push_ref, "--force"],
                    proxy,
                )?;
            } else {
                git_run_network(
                    &work_dir,
                    &["push", "--set-upstream", "origin", &push_ref],
                    proxy,
                )?;
            }
        } else {
            // First push to an empty repo.
            git_run_network(
                &work_dir,
                &["push", "--set-upstream", "origin", &push_ref],
                proxy,
            )?;
        }
    } else {
        // No network / repo doesn't exist yet — force push.
        git_run_network(
            &work_dir,
            &["push", "--set-upstream", "origin", &push_ref, "--force"],
            proxy,
        )?;
    }

    let ts = now_secs();
    let mut cfg = BackupConfig::load()?;
    cfg.github_repo = Some(repo_url.to_owned());
    cfg.last_backup_ts = Some(ts);
    cfg.save()?;
    Ok(())
}

/// Pull the latest backup from a GitHub repository and restore config files.
///
/// * `repo_url` – HTTPS URL.
/// * `token`    – Optional PAT; falls back to `GITHUB_TOKEN` env var.
///
/// Clones on first run; pulls on subsequent runs.
pub fn pull_github(repo_url: &str, token: Option<&str>) -> Result<()> {
    let auth_url = build_auth_url(repo_url, token)?;
    let work_dir = paths::backup_git_dir()?;

    let proxy_owned = BackupConfig::load().ok().and_then(|c| c.http_proxy);
    let proxy = proxy_owned.as_deref().filter(|s| !s.is_empty());

    if !work_dir.join(".git").exists() {
        std::fs::create_dir_all(&work_dir)?;
        git_init_main(&work_dir)?;
        git_run(&work_dir, &["remote", "add", "origin", &auth_url], None)?;
    } else {
        git_run(&work_dir, &["remote", "set-url", "origin", &auth_url], None)?;
    }
    let branch = remote_default_branch(&work_dir, proxy).unwrap_or_else(|| "main".to_string());
    git_run_network(&work_dir, &["fetch", "origin", &branch], proxy)?;
    git_run(
        &work_dir,
        &["reset", "--hard", &format!("origin/{branch}")],
        None,
    )?;

    // import_from_dir already takes a safety snapshot internally.
    import_from_dir(&work_dir)?;
    Ok(())
}

/// Quickly verify that the given repository is reachable (and token is valid)
/// by running `git ls-remote --heads <url> HEAD`.
///
/// Returns `Ok(())` when the remote responds successfully.
/// Returns an `Err` with a descriptive message on failure (network, auth, etc.).
pub fn check_connectivity(repo_url: &str, token: Option<&str>) -> Result<()> {
    let auth_url = build_auth_url(repo_url, token)?;
    let proxy_owned = BackupConfig::load().ok().and_then(|c| c.http_proxy);
    let proxy = proxy_owned.as_deref().filter(|s| !s.is_empty());

    // Use a temp dir so we don't need an initialised repo.
    let tmp = tempfile::tempdir().map_err(|e| Error::Invalid(format!("temp dir: {e}")))?;
    git_run_network(tmp.path(), &["ls-remote", "--heads", &auth_url], proxy).map(|_| ())
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

fn build_auth_url(url: &str, token: Option<&str>) -> Result<String> {
    let effective = token
        .map(|t| t.to_owned())
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .filter(|t| !t.is_empty());

    let Some(tok) = effective else {
        return Ok(url.to_owned());
    };

    if let Some(rest) = url.strip_prefix("https://") {
        // Remove any existing auth segment.
        let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
        Ok(format!("https://{tok}@{rest}"))
    } else if url.starts_with("http://") {
        Err(Error::Invalid("Use an HTTPS URL for GitHub backup".into()))
    } else {
        // SSH URL — PAT doesn't apply; return as-is.
        Ok(url.to_owned())
    }
}

/// `git init` with `main` as the default branch, portable across git versions
/// (`git init -b main` only works on git 2.28+; Ubuntu 20.04 ships 2.25).
fn git_init_main(dir: &Path) -> Result<()> {
    git_run(dir, &["init"], None)?;
    // Repoint HEAD to refs/heads/main regardless of the host's default.
    let _ = git_run(dir, &["symbolic-ref", "HEAD", "refs/heads/main"], None);
    Ok(())
}

fn remote_default_branch(dir: &Path, proxy: Option<&str>) -> Option<String> {
    let out = git_run_network(dir, &["ls-remote", "--symref", "origin", "HEAD"], proxy).ok()?;
    for line in out.lines() {
        let Some(rest) = line.strip_prefix("ref: refs/heads/") else {
            continue;
        };
        let branch = rest.split_whitespace().next()?.trim();
        if !branch.is_empty() {
            return Some(branch.to_string());
        }
    }
    None
}

/// Run a git network command through the configured proxy, but retry directly
/// when the proxy is stale/unreachable. This is important for headless servers
/// whose WebUI config can outlive a local proxy daemon.
fn git_run_network(dir: &Path, args: &[&str], proxy: Option<&str>) -> Result<String> {
    let Some(proxy) = proxy.filter(|s| !s.is_empty()) else {
        return git_run(dir, args, None);
    };
    match git_run(dir, args, Some(proxy)) {
        Ok(out) => Ok(out),
        Err(proxy_err) => {
            tracing::warn!(
                error = %proxy_err,
                "git command through configured proxy failed; retrying without proxy"
            );
            git_run(dir, args, None).map_err(|direct_err| {
                Error::Invalid(format!(
                    "{direct_err} (also failed via proxy {proxy}: {proxy_err})"
                ))
            })
        }
    }
}

/// Run a git subcommand, optionally routing through an HTTP proxy.
/// When `proxy` is `Some(p)` the `HTTPS_PROXY` and `HTTP_PROXY` env vars are
/// set on the child process so that both HTTPS and HTTP remotes are proxied.
fn git_run(dir: &Path, args: &[&str], proxy: Option<&str>) -> Result<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(dir).args(args);
    if let Some(p) = proxy.filter(|s| !s.is_empty()) {
        cmd.env("HTTPS_PROXY", p);
        cmd.env("HTTP_PROXY", p);
        cmd.env("https_proxy", p);
        cmd.env("http_proxy", p);
    }
    let out = cmd
        .output()
        .map_err(|e| Error::Invalid(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Invalid(format!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            msg.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

/// Best-effort: snapshot current config before a destructive operation.
/// Logs and swallows errors — callers should not fail if this does.
fn auto_safety_snapshot(label: &str) {
    let ts = now_secs();
    let dir = match paths::snapshots_dir() {
        Ok(d) => d.join(format!("{ts}-{label}")),
        Err(_) => return,
    };
    match export_to_dir(&dir) {
        Ok(files) if !files.is_empty() => {
            tracing::info!(snapshot = %dir.display(), "created safety snapshot before restore");
        }
        _ => {}
    }
}

fn update_last_ts(ts: u64) -> Result<()> {
    let mut cfg = BackupConfig::load()?;
    cfg.last_backup_ts = Some(ts);
    cfg.save()
}

// ─── Friendly time formatting ─────────────────────────────────────────────────

/// Return a human-readable description of a unix timestamp relative to now
/// (e.g. "2 hours ago").
pub fn time_ago(ts: u64) -> String {
    let now = now_secs();
    let delta = now.saturating_sub(ts);
    if delta < 60 {
        return "just now".into();
    }
    if delta < 3600 {
        let m = delta / 60;
        return format!("{m} minute{} ago", if m == 1 { "" } else { "s" });
    }
    if delta < 86_400 {
        let h = delta / 3600;
        return format!("{h} hour{} ago", if h == 1 { "" } else { "s" });
    }
    let d = delta / 86_400;
    format!("{d} day{} ago", if d == 1 { "" } else { "s" })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::model::{McpServer, McpTransport};
    use crate::mcp::McpRegistry;
    use crate::projects::{Project, ProjectStore};
    use crate::skills::{Skill, SkillRegistry, SkillSource};
    use std::collections::BTreeMap;

    #[test]
    fn auto_interval_secs() {
        assert_eq!(AutoInterval::Never.as_secs(), None);
        assert_eq!(AutoInterval::Daily.as_secs(), Some(86_400));
        assert_eq!(AutoInterval::Weekly.as_secs(), Some(604_800));
    }

    #[test]
    fn export_import_restores_projects_and_local_skills_portably() {
        let _guard = crate::test_support::lock();
        let home1 = tempfile::tempdir().unwrap();
        std::env::set_var("AIEM_HOME", home1.path());

        let source_skill = home1.path().join("source-skill");
        std::fs::create_dir_all(&source_skill).unwrap();
        std::fs::write(source_skill.join("SKILL.md"), "# Local Skill").unwrap();

        let mut skills = SkillRegistry::load().unwrap();
        skills.upsert(Skill {
            id: "local__demo".into(),
            name: "demo".into(),
            source: SkillSource::Local {
                path: source_skill.clone(),
            },
            version: "imported".into(),
            path: source_skill.clone(),
            description: None,
            installed_at: None,
            deployments: BTreeMap::new(),
            file_hashes: BTreeMap::new(),
        });
        skills.save().unwrap();

        let project_dir = tempfile::tempdir().unwrap();
        let mut projects = ProjectStore::load().unwrap();
        projects.upsert(Project {
            name: "demo-project".into(),
            path: project_dir.path().to_string_lossy().to_string(),
            ides: vec!["cursor".into()],
            skills: vec!["local__demo".into()],
            mcp_servers: vec![],
        });
        projects.save().unwrap();

        let export_dir = tempfile::tempdir().unwrap();
        export_to_dir(export_dir.path()).unwrap();
        assert!(export_dir.path().join("projects.json").is_file());
        assert!(export_dir
            .path()
            .join("custom_skills/local__demo/SKILL.md")
            .is_file());
        assert!(export_dir
            .path()
            .join("skill_contents/local__demo/SKILL.md")
            .is_file());

        let home2 = tempfile::tempdir().unwrap();
        std::env::set_var("AIEM_HOME", home2.path());
        import_from_dir(export_dir.path()).unwrap();

        let restored = SkillRegistry::load().unwrap();
        let skill = restored.get("local__demo").unwrap();
        let expected_path = home2.path().join("skills/local__demo");
        assert_eq!(skill.path, expected_path);
        assert_eq!(
            skill.source,
            SkillSource::Local {
                path: expected_path.clone()
            }
        );
        assert!(expected_path.join("SKILL.md").is_file());

        let restored_projects = ProjectStore::load().unwrap();
        assert!(restored_projects
            .list()
            .any(|p| p.name == "demo-project" && p.skills == vec!["local__demo".to_string()]));
    }

    #[test]
    fn export_handles_bom_prefixed_skill_index() {
        let _guard = crate::test_support::lock();
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("AIEM_HOME", home.path());

        let source_skill = home.path().join("source-skill");
        std::fs::create_dir_all(&source_skill).unwrap();
        std::fs::write(source_skill.join("SKILL.md"), "# BOM Skill").unwrap();

        let mut skills = SkillRegistry::load().unwrap();
        skills.upsert(Skill {
            id: "local__bom".into(),
            name: "bom".into(),
            source: SkillSource::Local {
                path: source_skill.clone(),
            },
            version: "local".into(),
            path: source_skill,
            description: None,
            installed_at: None,
            deployments: BTreeMap::new(),
            file_hashes: BTreeMap::new(),
        });
        skills.save().unwrap();

        let index_path = paths::skills_dir().unwrap().join("index.json");
        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend(std::fs::read(&index_path).unwrap());
        std::fs::write(&index_path, with_bom).unwrap();

        let export_dir = tempfile::tempdir().unwrap();
        export_to_dir(export_dir.path()).unwrap();
        assert!(export_dir
            .path()
            .join("skill_contents/local__bom/SKILL.md")
            .is_file());
    }

    #[test]
    fn export_import_restores_mcp_bundle_scripts() {
        let _guard = crate::test_support::lock();
        let home1 = tempfile::tempdir().unwrap();
        std::env::set_var("AIEM_HOME", home1.path());

        let bundle_dir = paths::mcp_bundles_dir().unwrap().join("demo-mcp");
        std::fs::create_dir_all(&bundle_dir).unwrap();
        std::fs::write(bundle_dir.join("server.py"), "print('ok')\n").unwrap();

        let mut reg = McpRegistry::load().unwrap();
        reg.upsert(McpServer {
            name: "demo-mcp".into(),
            transport: McpTransport::Stdio {
                command: "python".into(),
                args: vec!["server.py".into()],
                env: BTreeMap::new(),
                cwd: Some("{BUNDLE}".into()),
                bundle: Some("demo-mcp".into()),
            },
            targets: vec!["codex".into()],
            description: None,
            tags: vec![],
            disabled: false,
            source: None,
            runtime: None,
            auth_mode: Default::default(),
        });
        reg.save().unwrap();

        let export_dir = tempfile::tempdir().unwrap();
        export_to_dir(export_dir.path()).unwrap();
        assert!(export_dir
            .path()
            .join("mcp_bundles/demo-mcp/server.py")
            .is_file());

        let home2 = tempfile::tempdir().unwrap();
        std::env::set_var("AIEM_HOME", home2.path());
        import_from_dir(export_dir.path()).unwrap();

        assert!(home2
            .path()
            .join("mcp/bundles/demo-mcp/server.py")
            .is_file());
        let restored = McpRegistry::load().unwrap();
        let server = restored.get("demo-mcp").unwrap();
        assert!(matches!(
            &server.transport,
            McpTransport::Stdio {
                bundle: Some(bundle),
                ..
            } if bundle == "demo-mcp"
        ));
    }

    #[test]
    fn import_reanchors_github_skill_paths_and_prunes_foreign_deployments() {
        let _guard = crate::test_support::lock();
        let home1 = tempfile::tempdir().unwrap();
        std::env::set_var("AIEM_HOME", home1.path());

        let skill_dir = home1.path().join("skills").join("owner__repo__portable");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# Portable").unwrap();

        let mut deployments = BTreeMap::new();
        deployments.insert(
            "claude-code".to_string(),
            vec![
                r"C:\definitely-not-existing-aiem-root".to_string(),
                home1.path().to_string_lossy().to_string(),
            ],
        );

        let mut skills = SkillRegistry::load().unwrap();
        skills.upsert(Skill {
            id: "owner__repo__portable".into(),
            name: "portable".into(),
            source: SkillSource::GitHub {
                owner: "owner".into(),
                repo: "repo".into(),
                r#ref: None,
                subdir: Some("skills/portable".into()),
            },
            version: "old".into(),
            path: PathBuf::from(r"C:\Users\buaa\.aiem\skills\owner__repo__portable"),
            description: None,
            installed_at: None,
            deployments,
            file_hashes: BTreeMap::new(),
        });
        skills.save().unwrap();

        let export_dir = tempfile::tempdir().unwrap();
        export_to_dir(export_dir.path()).unwrap();
        assert!(export_dir
            .path()
            .join("skill_contents/owner__repo__portable/SKILL.md")
            .is_file());

        let home2 = tempfile::tempdir().unwrap();
        std::env::set_var("AIEM_HOME", home2.path());
        import_from_dir(export_dir.path()).unwrap();

        let restored = SkillRegistry::load().unwrap();
        let skill = restored.get("owner__repo__portable").unwrap();
        let expected = home2.path().join("skills/owner__repo__portable");
        assert_eq!(skill.path, expected);
        assert!(expected.join("SKILL.md").is_file());
        assert!(skill
            .deployments
            .get("claude-code")
            .map(|roots| roots
                .iter()
                .all(|root| !root.contains("definitely-not-existing-aiem-root")))
            .unwrap_or(true));
    }

    #[test]
    fn build_auth_url_injects_token() {
        let url = build_auth_url("https://github.com/user/repo", Some("mytoken")).unwrap();
        assert_eq!(url, "https://mytoken@github.com/user/repo");
    }

    #[test]
    fn build_auth_url_replaces_existing_auth() {
        let url =
            build_auth_url("https://oldtoken@github.com/user/repo", Some("newtoken")).unwrap();
        assert_eq!(url, "https://newtoken@github.com/user/repo");
    }

    #[test]
    fn build_auth_url_no_token_passthrough() {
        // With no token and no env var, returns original.
        let url = "https://github.com/user/repo";
        // Ensure GITHUB_TOKEN isn't set for this test.
        let _ = std::env::remove_var("GITHUB_TOKEN");
        let result = build_auth_url(url, None).unwrap();
        assert_eq!(result, url);
    }

    #[test]
    fn is_due_never() {
        let cfg = BackupConfig {
            auto_interval: AutoInterval::Never,
            ..Default::default()
        };
        assert!(!cfg.is_due());
    }

    #[test]
    fn is_due_daily_no_last() {
        let cfg = BackupConfig {
            auto_interval: AutoInterval::Daily,
            last_backup_ts: None,
            ..Default::default()
        };
        assert!(cfg.is_due());
    }

    #[test]
    fn is_due_daily_recent() {
        let recent = now_secs() - 100; // 100 seconds ago < 1 day
        let cfg = BackupConfig {
            auto_interval: AutoInterval::Daily,
            last_backup_ts: Some(recent),
            ..Default::default()
        };
        assert!(!cfg.is_due());
    }

    #[test]
    fn time_ago_formats() {
        let now = now_secs();
        assert_eq!(time_ago(now - 30), "just now");
        assert_eq!(time_ago(now - 90), "1 minute ago");
        assert_eq!(time_ago(now - 7200), "2 hours ago");
        assert_eq!(time_ago(now - 86400), "1 day ago");
    }
}
