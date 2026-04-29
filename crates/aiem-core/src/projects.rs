//! Projects — project-level skill & MCP deployment management.
//!
//! Each project associates a directory with a set of skills and MCP servers
//! that should be deployed there. This enables per-project configuration for
//! IDEs that use project-scope skills (Cursor, VSCode, Windsurf, etc.).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs_util::{atomic_write, strip_utf8_bom};
use crate::ide;
use crate::{paths, Error, Result};

/// Normalize a project path for use as a registry key.
///
/// Canonicalizes the path when it exists on disk, then converts to a
/// forward-slash string. This ensures the same directory is never registered
/// twice with different representations (e.g. `C:\Users\foo\proj` vs
/// `C:/Users/foo/proj` vs `c:\users\foo\proj`).
pub fn normalize_project_path(raw: &str) -> String {
    let p = PathBuf::from(raw);
    let resolved = p.canonicalize().unwrap_or(p);
    let s = resolved.to_string_lossy();
    // Strip Windows \\?\ prefix
    let stripped = s.strip_prefix(r"\\?\").unwrap_or(&s);
    stripped.replace('\\', "/")
}

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
    pub fn file() -> Result<PathBuf> {
        paths::projects_file()
    }

    pub fn load() -> Result<Self> {
        let p = Self::file()?;
        if !p.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&p)?;
        let inner: ProjectsFile = serde_json::from_slice(strip_utf8_bom(&bytes))?;
        Ok(Self { inner })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.inner)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &Project> {
        self.inner.projects.values()
    }

    pub fn get(&self, path: &str) -> Option<&Project> {
        self.inner.projects.get(path).or_else(|| {
            let n = normalize_project_path(path);
            self.inner.projects.get(&n)
        })
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut Project> {
        if self.inner.projects.contains_key(path) {
            return self.inner.projects.get_mut(path);
        }
        let n = normalize_project_path(path);
        self.inner.projects.get_mut(&n)
    }

    pub fn upsert(&mut self, mut p: Project) {
        p.path = normalize_project_path(&p.path);
        self.inner.projects.insert(p.path.clone(), p);
    }

    pub fn remove(&mut self, path: &str) -> Result<()> {
        if self.inner.projects.remove(path).is_some() {
            return Ok(());
        }
        let n = normalize_project_path(path);
        if self.inner.projects.remove(&n).is_some() {
            return Ok(());
        }
        Err(Error::NotFound(format!("project `{path}` not found")))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    struct Isolated {
        _dir: tempfile::TempDir,
        _guard: MutexGuard<'static, ()>,
    }

    fn isolate() -> Isolated {
        let guard = crate::test_support::lock();
        let dir = tempfile::tempdir().expect("tempdir");
        std::env::set_var("AIEM_HOME", dir.path());
        Isolated {
            _dir: dir,
            _guard: guard,
        }
    }

    #[test]
    fn roundtrip() {
        let _h = isolate();
        let mut store = ProjectStore::load().unwrap();
        store.upsert(Project {
            name: "demo".into(),
            path: "/tmp/demo".into(),
            ides: vec![],
            skills: vec![],
            mcp_servers: vec!["s1".into()],
        });
        store.save().unwrap();
        let store2 = ProjectStore::load().unwrap();
        let p = store2.get("/tmp/demo").unwrap();
        assert_eq!(p.mcp_servers, vec!["s1".to_string()]);
    }

    #[test]
    fn load_with_bom() {
        let _h = isolate();
        let p = ProjectStore::file().unwrap();
        crate::paths::ensure_layout().unwrap();
        let mut data = vec![0xEF, 0xBB, 0xBF];
        data.extend_from_slice(b"{\"projects\":{}}");
        std::fs::write(&p, &data).unwrap();
        let store = ProjectStore::load().unwrap();
        assert_eq!(store.list().count(), 0);
    }

    #[test]
    fn remove_missing_errors() {
        let _h = isolate();
        let mut store = ProjectStore::load().unwrap();
        assert!(store.remove("/no/such").is_err());
    }

    #[test]
    fn normalize_forward_and_back_slashes() {
        let a = normalize_project_path("C:/Users/test/project");
        let b = normalize_project_path("C:\\Users\\test\\project");
        assert_eq!(
            a, b,
            "forward-slash and back-slash should normalize to the same key"
        );
    }

    #[test]
    fn get_finds_by_normalized_key() {
        let _h = isolate();
        let mut store = ProjectStore::load().unwrap();
        store.upsert(Project {
            name: "x".into(),
            path: "C:/Users/test/proj".into(),
            ides: vec![],
            skills: vec![],
            mcp_servers: vec![],
        });
        store.save().unwrap();
        let store2 = ProjectStore::load().unwrap();
        // Look up with backslash — should still find it.
        assert!(store2.get("C:\\Users\\test\\proj").is_some());
    }
}
