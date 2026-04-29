//! Background task plumbing: run async jobs (GitHub fetches, sync, etc.) on a
//! tokio runtime and deliver their results back to the UI thread via a channel.

use std::path::PathBuf;
use std::sync::mpsc;

use aiem_core::mcp::{self, McpRegistry};
use aiem_core::skills::{install, service as skill_svc, SkillRegistry};

/// A message produced by a background task.
#[derive(Debug)]
pub enum TaskMsg {
    Info(String),
    Error(String),
    SkillsChanged,
    McpChanged,
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

    #[allow(dead_code)]
    pub fn info(&self, s: impl Into<String>) {
        let _ = self.tx.send(TaskMsg::Info(s.into()));
    }
    #[allow(dead_code)]
    pub fn error(&self, s: impl Into<String>) {
        let _ = self.tx.send(TaskMsg::Error(s.into()));
    }

    pub fn add_skill_from_github(&self, source: String, name: Option<String>) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(TaskMsg::Info(format!("downloading {source}...")));
            match skill_svc::add_from_github(&source, name.as_deref()).await {
                Ok(r) => {
                    let _ = tx.send(TaskMsg::Info(r.summary));
                    let _ = tx.send(TaskMsg::SkillsChanged);
                    if !r.mcp_registered.is_empty() {
                        let _ = tx.send(TaskMsg::McpChanged);
                    }
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("add: {e}")));
                }
            }
        });
    }

    pub fn update_skill(&self, id: String) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(TaskMsg::Info(format!("updating {id}...")));
            match skill_svc::update_skill(&id).await {
                Ok(r) => {
                    let _ = tx.send(TaskMsg::Info(r.summary));
                    let _ = tx.send(TaskMsg::SkillsChanged);
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("update: {e}")));
                }
            }
        });
    }

    pub fn sync_group(&self, owner: String, repo: String) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(TaskMsg::Info(format!("syncing {owner}/{repo}...")));
            match skill_svc::sync_github_group(&owner, &repo).await {
                Ok(r) => {
                    let _ = tx.send(TaskMsg::Info(r.summary));
                    let _ = tx.send(TaskMsg::SkillsChanged);
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("sync: {e}")));
                }
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

pub fn deploy_skill(
    id: &str,
    ide: &str,
    project: Option<&std::path::Path>,
) -> anyhow::Result<PathBuf> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg
        .get(id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("skill {id} not found"))?;
    let (link, _kind) = install::deploy(&mut skill, ide, project)?;
    reg.upsert(skill);
    reg.save()?;
    Ok(link)
}

pub fn undeploy_skill(
    id: &str,
    ide: &str,
    project: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg
        .get(id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("skill {id} not found"))?;
    install::undeploy(&mut skill, ide, project)?;
    reg.upsert(skill);
    reg.save()?;
    Ok(())
}

pub fn deploy_skill_group(
    ids: &[String],
    ide: &str,
    project: Option<&std::path::Path>,
) -> anyhow::Result<usize> {
    let mut ok = 0usize;
    let mut last_err = None;
    for id in ids {
        match deploy_skill(id, ide, project) {
            Ok(_) => ok += 1,
            Err(e) => last_err = Some(format!("{id}: {e}")),
        }
    }
    if ok == 0 {
        if let Some(err) = last_err {
            anyhow::bail!(err);
        }
    }
    Ok(ok)
}

pub fn undeploy_skill_group(
    ids: &[String],
    ide: &str,
    project: Option<&std::path::Path>,
) -> anyhow::Result<usize> {
    let mut ok = 0usize;
    let mut last_err = None;
    for id in ids {
        match undeploy_skill(id, ide, project) {
            Ok(_) => ok += 1,
            Err(e) => last_err = Some(format!("{id}: {e}")),
        }
    }
    if ok == 0 {
        if let Some(err) = last_err {
            anyhow::bail!(err);
        }
    }
    Ok(ok)
}

pub fn clear_all_global_skills() -> anyhow::Result<usize> {
    let mut reg = SkillRegistry::load()?;
    let count = install::undeploy_all_global(&mut reg)?;
    reg.save()?;
    Ok(count)
}

pub fn mcp_sync_all(project: Option<&std::path::Path>) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let reg = McpRegistry::load()?;
    let plan = mcp::sync::plan(&reg, &[], None);
    Ok(mcp::sync::execute(&reg, &plan, project, None)?)
}

pub fn mcp_remove(name: &str) -> anyhow::Result<()> {
    let mut reg = McpRegistry::load()?;
    reg.remove(name)?;
    reg.save()?;
    Ok(())
}

pub fn mcp_sync_one_global(
    name: &str,
    target_ides: &[String],
) -> anyhow::Result<Vec<(String, PathBuf)>> {
    Ok(mcp::sync::sync_one_global(name, target_ides)?)
}

pub fn mcp_retract_one_global_from_ide(
    name: &str,
    ide: &str,
) -> anyhow::Result<Vec<(String, PathBuf)>> {
    Ok(mcp::sync::retract_one_global_from_ides(
        name,
        &[ide.to_string()],
    )?)
}

pub fn mcp_toggle(name: &str, disabled: bool) -> anyhow::Result<()> {
    let mut reg = McpRegistry::load()?;
    let s = reg
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("mcp {name} not found"))?;
    s.disabled = disabled;
    reg.save()?;
    Ok(())
}

pub fn mcp_deploy_to_project_for_ide(
    name: &str,
    ide: &str,
    project: &std::path::Path,
) -> anyhow::Result<Vec<(String, PathBuf)>> {
    Ok(mcp::deploy::deploy_to_project_for_ides(
        name,
        project,
        &[ide.to_string()],
    )?)
}

pub fn mcp_undeploy_from_project_for_ide(
    name: &str,
    ide: &str,
    project: &std::path::Path,
) -> anyhow::Result<Vec<(String, PathBuf)>> {
    Ok(mcp::deploy::undeploy_from_project_for_ides(
        name,
        project,
        &[ide.to_string()],
    )?)
}

#[allow(dead_code)]
pub fn mcp_projects_with(name: &str) -> anyhow::Result<Vec<String>> {
    Ok(mcp::deploy::projects_with(name)?)
}

pub fn skill_projects_with(id: &str) -> anyhow::Result<Vec<String>> {
    Ok(aiem_core::skills::registry::projects_with(id)?)
}

pub fn skill_deploy_to_project(
    id: &str,
    ide: &str,
    project: &std::path::Path,
) -> anyhow::Result<PathBuf> {
    Ok(aiem_core::skills::deploy::deploy_to_project(
        id, ide, project,
    )?)
}

pub fn skill_undeploy_from_project(
    id: &str,
    ide: &str,
    project: &std::path::Path,
) -> anyhow::Result<()> {
    Ok(aiem_core::skills::deploy::undeploy_from_project(
        id, ide, project,
    )?)
}

// ─── Backup tasks ─────────────────────────────────────────────────────────────

impl TaskBus {
    pub fn backup_snapshot(&self) {
        let tx = self.tx.clone();
        self.runtime
            .spawn_blocking(move || match aiem_core::backup::snapshot_local() {
                Ok(path) => {
                    let _ = tx.send(TaskMsg::Info(format!("Snapshot saved: {}", path.display())));
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("Snapshot failed: {e}")));
                }
            });
    }

    pub fn backup_push_github(&self, repo: String, token: Option<String>) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            match aiem_core::backup::push_github(&repo, token.as_deref()) {
                Ok(()) => {
                    let _ = tx.send(TaskMsg::Info("Backup pushed to GitHub".into()));
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("GitHub push failed: {e}")));
                }
            }
        });
    }

    pub fn backup_test_connection(&self, repo: String, token: Option<String>) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            let _ = tx.send(TaskMsg::Info("Testing connection...".into()));
            match aiem_core::backup::check_connectivity(&repo, token.as_deref()) {
                Ok(()) => {
                    let _ = tx.send(TaskMsg::Info("Connection OK".into()));
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("Connection failed: {e}")));
                }
            }
        });
    }

    pub fn backup_pull_github(&self, repo: String, token: Option<String>) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            match aiem_core::backup::pull_github(&repo, token.as_deref()) {
                Ok(()) => {
                    let _ = tx.send(TaskMsg::Info("Restored from GitHub backup".into()));
                    let _ = tx.send(TaskMsg::SkillsChanged);
                    let _ = tx.send(TaskMsg::McpChanged);
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("GitHub pull failed: {e}")));
                }
            }
        });
    }

    pub fn backup_export_dir(&self, dest: std::path::PathBuf) {
        let tx = self.tx.clone();
        self.runtime
            .spawn_blocking(move || match aiem_core::backup::export_to_dir(&dest) {
                Ok(files) => {
                    let _ = tx.send(TaskMsg::Info(format!(
                        "Exported {} file(s) to {}",
                        files.len(),
                        dest.display()
                    )));
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("Export failed: {e}")));
                }
            });
    }

    pub fn backup_import_dir(&self, src: std::path::PathBuf) {
        let tx = self.tx.clone();
        self.runtime
            .spawn_blocking(move || match aiem_core::backup::import_from_dir(&src) {
                Ok(files) => {
                    let _ = tx.send(TaskMsg::Info(format!(
                        "Restored {} file(s) from {}",
                        files.len(),
                        src.display()
                    )));
                    let _ = tx.send(TaskMsg::SkillsChanged);
                    let _ = tx.send(TaskMsg::McpChanged);
                }
                Err(e) => {
                    let _ = tx.send(TaskMsg::Error(format!("Import failed: {e}")));
                }
            });
    }
}
