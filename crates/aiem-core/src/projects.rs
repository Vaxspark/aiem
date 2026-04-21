//! Projects — project-level skill & MCP deployment management.
//!
//! Each project associates a directory with a set of skills and MCP servers
//! that should be deployed there. This enables per-project configuration for
//! IDEs that use project-scope skills (Cursor, VSCode, Windsurf, etc.).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs_util::atomic_write;
use crate::ide;
use crate::{paths, Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Project {
    /// Display name for the project.
    pub name: String,
    /// Absolute path to the project root directory.
    pub path: String,
    /// IDE ids to deploy skills into for this project.
    #[serde(default)]
    pub ides: Vec<String>,
    /// Skill IDs deployed to this project.
    #[serde(default)]
    pub skills: Vec<String>,
    /// MCP server names configured for this project.
    #[serde(default)]
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProjectsFile {
    #[serde(default)]
    pub projects: BTreeMap<String, Project>,
}

#[derive(Debug, Default)]
pub struct ProjectStore {
    inner: ProjectsFile,
}

impl ProjectStore {
    pub fn file() -> Result<PathBuf> { paths::projects_file() }

    pub fn load() -> Result<Self> {
        let p = Self::file()?;
        if !p.exists() { return Ok(Self::default()); }
        let bytes = std::fs::read(&p)?;
        let data = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { &bytes[3..] } else { &bytes };
        let inner: ProjectsFile = serde_json::from_slice(data)?;
        Ok(Self { inner })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.inner)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &Project> { self.inner.projects.values() }
    pub fn get(&self, path: &str) -> Option<&Project> { self.inner.projects.get(path) }
    pub fn get_mut(&mut self, path: &str) -> Option<&mut Project> { self.inner.projects.get_mut(path) }

    pub fn upsert(&mut self, p: Project) {
        self.inner.projects.insert(p.path.clone(), p);
    }

    pub fn remove(&mut self, path: &str) -> Result<()> {
        if self.inner.projects.remove(path).is_none() {
            return Err(Error::NotFound(format!("project `{path}` not found")));
        }
        Ok(())
    }
}

/// Detect which IDE skill directories already exist in a project directory.
/// Returns a list of IDE ids that have an existing skills dir.
pub fn detect_project_ides(project_path: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for ide in ide::IDES {
        let dir = project_path.join(ide.skills_dir);
        if dir.exists() && dir.is_dir() {
            found.push(ide.id.to_string());
        }
    }
    found
}

/// Scan skills already deployed (symlinked) in a project's IDE directories.
/// Returns a list of skill directory names found.
pub fn scan_project_skills(project_path: &Path) -> Vec<(String, String)> {
    let mut results = Vec::new(); // (ide_id, skill_dir_name)
    for ide in ide::IDES {
        let dir = project_path.join(ide.skills_dir);
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        results.push((ide.id.to_string(), name.to_string()));
                    }
                }
            }
        }
    }
    results
}
