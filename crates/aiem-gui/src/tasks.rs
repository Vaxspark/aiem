//! Background task plumbing: run async jobs (GitHub fetches, sync, etc.) on a
//! tokio runtime and deliver their results back to the UI thread via a channel.

use std::path::PathBuf;
use std::sync::mpsc;

use aiem_core::mcp::{self, McpRegistry};
use aiem_core::registry::RegistryItem;
use aiem_core::skills::{github, install, model::SkillSource, SkillRegistry};
use walkdir;

/// A message produced by a background task. Each variant is tagged with a short
/// message the UI can toast to the user.
#[derive(Debug)]
pub enum TaskMsg {
    Info(String),
    Error(String),
    /// A skill was added / updated; the UI should reload its skill list.
    SkillsChanged,
    /// MCP registry or IDE configs changed; UI should reload.
    McpChanged,
    /// Registry search results arrived.
    RegistryResults(Vec<RegistryItem>),
    /// Registry search failed.
    RegistryError(String),
    /// Popular items loaded.
    PopularResults(Vec<RegistryItem>),
}

#[derive(Clone)]
pub struct TaskBus {
    tx: mpsc::Sender<TaskMsg>,
    runtime: std::sync::Arc<tokio::runtime::Runtime>,
}

impl TaskBus {
    pub fn new() -> (Self, mpsc::Receiver<TaskMsg>) {
        let (tx, rx) = mpsc::channel();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build tokio runtime");
        (
            Self {
                tx,
                runtime: std::sync::Arc::new(rt),
            },
            rx,
        )
    }

    pub fn info(&self, s: impl Into<String>) { let _ = self.tx.send(TaskMsg::Info(s.into())); }
    pub fn error(&self, s: impl Into<String>) { let _ = self.tx.send(TaskMsg::Error(s.into())); }

    pub fn add_skill_from_github(&self, source: String, name: Option<String>) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let parsed = match SkillSource::parse_github(&source) {
                Some(s) => s,
                None => {
                    let _ = tx.send(TaskMsg::Error(format!("invalid GitHub source: {source}")));
                    return;
                }
            };
            let (owner, repo, reff, subdir) = match parsed {
                SkillSource::GitHub { owner, repo, r#ref, subdir } => (owner, repo, r#ref, subdir),
                _ => {
                    let _ = tx.send(TaskMsg::Error("only github sources are supported".into()));
                    return;
                }
            };
            let _ = tx.send(TaskMsg::Info(format!("downloading {owner}/{repo}...")));
            match github::fetch_github_auto(&owner, &repo, reff.as_deref(), subdir.as_deref(), name.as_deref()).await {
                Ok(result) => {
                    // Save skills
                    match SkillRegistry::load() {
                        Ok(mut reg) => {
                            let skill_count = result.skills.len();
                            for skill in result.skills {
                                reg.upsert(skill);
                            }
                            if let Err(e) = reg.save() {
                                let _ = tx.send(TaskMsg::Error(format!("save registry: {e}")));
                                return;
                            }
                            if skill_count > 0 {
                                let _ = tx.send(TaskMsg::Info(format!("added {skill_count} skill(s)")));
                            }
                            let _ = tx.send(TaskMsg::SkillsChanged);
                        }
                        Err(e) => { let _ = tx.send(TaskMsg::Error(format!("load registry: {e}"))); }
                    }
                    // Save detected MCP servers
                    if !result.mcp_servers.is_empty() {
                        match aiem_core::mcp::McpRegistry::load() {
                            Ok(mut mcp_reg) => {
                                let mcp_count = result.mcp_servers.len();
                                for s in result.mcp_servers {
                                    mcp_reg.upsert(s);
                                }
                                if let Err(e) = mcp_reg.save() {
                                    let _ = tx.send(TaskMsg::Error(format!("save MCP: {e}")));
                                } else {
                                    let _ = tx.send(TaskMsg::Info(format!("detected {mcp_count} MCP server(s)")));
                                }
                            }
                            Err(e) => { let _ = tx.send(TaskMsg::Error(format!("load MCP: {e}"))); }
                        }
                    }
                }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("fetch: {e}"))); }
            }
        });
    }

    pub fn update_skill(&self, id: String) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let reg_res = SkillRegistry::load();
            let Ok(reg) = reg_res else {
                let _ = tx.send(TaskMsg::Error(format!("load registry failed")));
                return;
            };
            let existing = match reg.get(&id).cloned() {
                Some(s) => s,
                None => { let _ = tx.send(TaskMsg::Error(format!("skill {id} not found"))); return; }
            };
            let SkillSource::GitHub { owner, repo, r#ref, subdir } = existing.source.clone() else {
                let _ = tx.send(TaskMsg::Error("skill not from github".into()));
                return;
            };
            let _ = tx.send(TaskMsg::Info(format!("updating {id}...")));
            // Always update from the default branch (latest), not the pinned ref.
            match github::fetch_github_to_temp(&owner, &repo, None, subdir.as_deref()).await {
                Ok((temp_dir, new_version, actual_subdir)) => {
                    // Smart merge: 3-way comparison using stored file hashes
                    let target = &existing.path;
                    let skipped = smart_merge(temp_dir.path(), target, &existing.file_hashes);
                    let mut skill = existing.clone();
                    skill.version = new_version;
                    // Record new hashes for the updated files
                    skill.file_hashes = hash_files(target);
                    // Clear the pinned ref so future updates always pull latest.
                    // Also update subdir if the directory moved in the upstream repo.
                    if let SkillSource::GitHub { r#ref: ref mut stored_ref, subdir: ref mut stored_subdir, .. } = skill.source {
                        *stored_ref = None;
                        if let Some(new_sub) = actual_subdir {
                            *stored_subdir = Some(new_sub);
                        }
                    }
                    let mut reg = match SkillRegistry::load() {
                        Ok(r) => r,
                        Err(e) => { let _ = tx.send(TaskMsg::Error(format!("{e}"))); return; }
                    };
                    reg.upsert(skill);
                    if let Err(e) = reg.save() {
                        let _ = tx.send(TaskMsg::Error(format!("save: {e}")));
                        return;
                    }
                    if skipped.is_empty() {
                        let _ = tx.send(TaskMsg::Info(format!("updated {id} (all files)")));
                    } else {
                        let _ = tx.send(TaskMsg::Info(format!(
                            "updated {id} — skipped {} locally modified file(s): {}",
                            skipped.len(),
                            skipped.join(", ")
                        )));
                    }
                    let _ = tx.send(TaskMsg::SkillsChanged);
                }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("fetch: {e}"))); }
            }
        });
    }

    /// Sync an entire owner/repo group: update existing skills AND install newly added ones.
    /// `skills` is the current list of installed skills in that group.
    pub fn sync_group(&self, owner: String, repo: String, skills: Vec<aiem_core::skills::model::Skill>) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(TaskMsg::Info(format!("syncing {owner}/{repo}...")));
            match github::sync_github_group(&owner, &repo, &skills).await {
                Ok(result) => {
                    let n_updated = result.updated.len();
                    let n_added = result.added.len();
                    if n_added > 0 {
                        let names: Vec<_> = result.added.iter().map(|s| s.name.as_str()).collect();
                        let _ = tx.send(TaskMsg::Info(format!(
                            "synced {owner}/{repo}: {n_updated} updated, {n_added} new skill(s) added: {}",
                            names.join(", ")
                        )));
                    } else {
                        let _ = tx.send(TaskMsg::Info(format!(
                            "synced {owner}/{repo}: {n_updated} updated, no new skills"
                        )));
                    }
                    let _ = tx.send(TaskMsg::SkillsChanged);
                }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("sync {owner}/{repo}: {e}"))); }
            }
        });
    }

    pub fn search_registry(&self, query: String) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match aiem_core::registry::search_all(&query).await {
                Ok(results) => { let _ = tx.send(TaskMsg::RegistryResults(results)); }
                Err(e) => { let _ = tx.send(TaskMsg::RegistryError(format!("{e}"))); }
            }
        });
    }

    pub fn search_popular(&self) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match aiem_core::registry::popular().await {
                Ok(results) => { let _ = tx.send(TaskMsg::PopularResults(results)); }
                Err(_) => {} // silently ignore — popular is best-effort
            }
        });
    }
}

// --- Synchronous helpers used directly by the UI (no tokio needed). ---------

pub fn remove_skill(id: &str) -> anyhow::Result<()> {
    let mut reg = SkillRegistry::load()?;
    install::remove_skill(&mut reg, id)?;
    reg.save()?;
    Ok(())
}

pub fn deploy_skill(id: &str, ide: &str, project: Option<&std::path::Path>) -> anyhow::Result<PathBuf> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg.get(id).cloned().ok_or_else(|| anyhow::anyhow!("skill {id} not found"))?;
    let (link, _kind) = install::deploy(&mut skill, ide, project)?;
    reg.upsert(skill);
    reg.save()?;
    Ok(link)
}

pub fn undeploy_skill(id: &str, ide: &str, project: Option<&std::path::Path>) -> anyhow::Result<()> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg.get(id).cloned().ok_or_else(|| anyhow::anyhow!("skill {id} not found"))?;
    install::undeploy(&mut skill, ide, project)?;
    reg.upsert(skill);
    reg.save()?;
    Ok(())
}

pub fn clear_all_global_skills() -> anyhow::Result<usize> {
    let mut reg = SkillRegistry::load()?;
    let count = install::undeploy_all_global(&mut reg)?;
    reg.save()?;
    Ok(count)
}

pub fn mcp_sync_all(project: Option<&std::path::Path>) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let reg = McpRegistry::load()?;
    let plan = mcp::sync::plan(&reg, &[]);
    Ok(mcp::sync::execute(&reg, &plan, project)?)
}

pub fn mcp_remove(name: &str) -> anyhow::Result<()> {
    let mut reg = McpRegistry::load()?;
    reg.remove(name)?;
    reg.save()?;
    Ok(())
}

pub fn mcp_toggle(name: &str, disabled: bool) -> anyhow::Result<()> {
    let mut reg = McpRegistry::load()?;
    let s = reg.get_mut(name).ok_or_else(|| anyhow::anyhow!("mcp {name} not found"))?;
    s.disabled = disabled;
    reg.save()?;
    Ok(())
}

pub fn mcp_deploy_to_project(name: &str, project: &std::path::Path)
    -> anyhow::Result<Vec<(String, PathBuf)>>
{
    Ok(mcp::deploy::deploy_to_project(name, project)?)
}

pub fn mcp_undeploy_from_project(name: &str, project: &std::path::Path)
    -> anyhow::Result<Vec<(String, PathBuf)>>
{
    Ok(mcp::deploy::undeploy_from_project(name, project)?)
}

pub fn mcp_projects_with(name: &str) -> anyhow::Result<Vec<String>> {
    Ok(mcp::deploy::projects_with(name)?)
}

pub fn skill_projects_with(id: &str) -> anyhow::Result<Vec<String>> {
    Ok(aiem_core::skills::registry::projects_with(id)?)
}

pub fn skill_deploy_to_project(id: &str, ide: &str, project: &std::path::Path)
    -> anyhow::Result<PathBuf>
{
    Ok(aiem_core::skills::deploy::deploy_to_project(id, ide, project)?)
}

pub fn skill_undeploy_from_project(id: &str, ide: &str, project: &std::path::Path)
    -> anyhow::Result<()>
{
    Ok(aiem_core::skills::deploy::undeploy_from_project(id, ide, project)?)
}

/// Smart merge using 3-way comparison:
/// - If `hash(current_file) == original_hash` → not user-modified → overwrite with new version
/// - If `hash(current_file) != original_hash` → user modified → skip
/// - If file is new (not in dst) → always copy
/// - `original_hashes`: the hashes stored at last install/update time, keyed by relative path (forward slashes)
/// Returns list of skipped file relative paths.
fn smart_merge(
    src: &std::path::Path,
    dst: &std::path::Path,
    original_hashes: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    use sha2::{Digest, Sha256};
    let mut skipped = Vec::new();
    let walker = walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok());
    for entry in walker {
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        if target.exists() {
            let current = std::fs::read(&target).unwrap_or_default();
            let current_hash = hex::encode(Sha256::digest(&current));
            if let Some(orig_hash) = original_hashes.get(&rel_str) {
                // 3-way: current matches original → not user-modified → safe to overwrite
                if &current_hash != orig_hash {
                    skipped.push(rel_str);
                    continue;
                }
            }
            // No stored hash (legacy install) or hash matches → overwrite
        }
        let _ = std::fs::copy(entry.path(), &target);
    }
    skipped
}

/// Compute SHA-256 hashes of all files under `dir`, keyed by relative path (forward slashes).
fn hash_files(dir: &std::path::Path) -> std::collections::BTreeMap<String, String> {
    use sha2::{Digest, Sha256};
    let mut map = std::collections::BTreeMap::new();
    let walker = walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok());
    for entry in walker {
        if !entry.file_type().is_file() { continue; }
        let rel = match entry.path().strip_prefix(dir) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if let Ok(bytes) = std::fs::read(entry.path()) {
            map.insert(rel, hex::encode(Sha256::digest(&bytes)));
        }
    }
    map
}

// ─── Backup tasks ─────────────────────────────────────────────────────────────

impl TaskBus {
    /// Take a local timestamped snapshot in background.
    pub fn backup_snapshot(&self) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            match aiem_core::backup::snapshot_local() {
                Ok(path) => {
                    let _ = tx.send(TaskMsg::Info(
                        format!("Snapshot saved: {}", path.display())
                    ));
                }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("Snapshot failed: {e}"))); }
            }
        });
    }

    /// Push backup to GitHub in background.
    pub fn backup_push_github(&self, repo: String, token: Option<String>) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            match aiem_core::backup::push_github(&repo, token.as_deref()) {
                Ok(()) => { let _ = tx.send(TaskMsg::Info("Backup pushed to GitHub".into())); }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("GitHub push failed: {e}"))); }
            }
        });
    }

    /// Quick connectivity check: runs `git ls-remote` without touching local data.
    pub fn backup_test_connection(&self, repo: String, token: Option<String>) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            let _ = tx.send(TaskMsg::Info("Testing connection…".into()));
            match aiem_core::backup::check_connectivity(&repo, token.as_deref()) {
                Ok(()) => { let _ = tx.send(TaskMsg::Info("✅ Connection OK — repo is reachable".into())); }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("Connection failed: {e}"))); }
            }
        });
    }

    /// Pull (restore) backup from GitHub in background.
    pub fn backup_pull_github(&self, repo: String, token: Option<String>) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            match aiem_core::backup::pull_github(&repo, token.as_deref()) {
                Ok(()) => {
                    let _ = tx.send(TaskMsg::Info("Restored from GitHub backup".into()));
                    let _ = tx.send(TaskMsg::SkillsChanged);
                    let _ = tx.send(TaskMsg::McpChanged);
                }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("GitHub pull failed: {e}"))); }
            }
        });
    }

    /// Export config files to an explicit destination directory in background.
    pub fn backup_export_dir(&self, dest: std::path::PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            match aiem_core::backup::export_to_dir(&dest) {
                Ok(files) => {
                    let _ = tx.send(TaskMsg::Info(
                        format!("Exported {} file(s) to {}", files.len(), dest.display())
                    ));
                }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("Export failed: {e}"))); }
            }
        });
    }

    /// Import (restore) config files from a source directory in background.
    pub fn backup_import_dir(&self, src: std::path::PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            match aiem_core::backup::import_from_dir(&src) {
                Ok(files) => {
                    let _ = tx.send(TaskMsg::Info(
                        format!("Restored {} file(s) from {}", files.len(), src.display())
                    ));
                    let _ = tx.send(TaskMsg::SkillsChanged);
                    let _ = tx.send(TaskMsg::McpChanged);
                }
                Err(e) => { let _ = tx.send(TaskMsg::Error(format!("Import failed: {e}"))); }
            }
        });
    }
}
