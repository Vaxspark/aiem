//! Per-project MCP deployment helpers.
//!
//! Bridges the MCP registry and the project store:
//!
//! - [`deploy_to_project`] upserts a server name into `Project.mcp_servers`
//!   and immediately writes the project-scoped IDE config files.
//! - [`undeploy_from_project`] removes the name and re-syncs so the server is
//!   dropped from the project config files.
//! - [`projects_with`] reverse lookup: which registered projects list a given
//!   server name.
//!
//! These helpers are the single entry point used by CLI / GUI / Web when the
//! user triggers "deploy this MCP server to that project" from the MCP list
//! page.

use std::path::{Path, PathBuf};

use crate::mcp::registry::McpRegistry;
use crate::mcp::sync;
use crate::projects::ProjectStore;
use crate::{Error, Result};

/// Upsert `server_name` into the project's `mcp_servers` list and sync every
/// targeted IDE's project-scoped config file. Returns the (ide, path) list of
/// files that were written.
///
/// Errors:
/// - `NotFound` if the server is not in the registry
/// - `NotFound` if `project_path` is not a registered project
pub fn deploy_to_project(
    server_name: &str,
    project_path: &Path,
) -> Result<Vec<(String, PathBuf)>> {
    let reg = McpRegistry::load()?;
    let server = reg
        .get(server_name)
        .ok_or_else(|| Error::NotFound(format!("mcp server `{server_name}` not found")))?
        .clone();

    let path_key = project_path.to_string_lossy().to_string();
    let mut store = ProjectStore::load()?;
    let proj = store
        .get_mut(&path_key)
        .ok_or_else(|| Error::NotFound(format!("project `{path_key}` not registered")))?;

    if !proj.mcp_servers.iter().any(|n| n == server_name) {
        proj.mcp_servers.push(server_name.to_string());
    }
    store.save()?;

    // Sync just the server's targets for this project root.
    let only_ides = server.targets.clone();
    let plan = sync::plan(&reg, &only_ides);
    sync::execute(&reg, &plan, Some(project_path))
}

/// Remove `server_name` from the project's `mcp_servers` list and re-sync so
/// the project config files no longer reference it.
///
/// Errors:
/// - `NotFound` if `project_path` is not a registered project
pub fn undeploy_from_project(
    server_name: &str,
    project_path: &Path,
) -> Result<Vec<(String, PathBuf)>> {
    let path_key = project_path.to_string_lossy().to_string();
    let mut store = ProjectStore::load()?;
    let proj = store
        .get_mut(&path_key)
        .ok_or_else(|| Error::NotFound(format!("project `{path_key}` not registered")))?;

    // The server targets were recorded when it was deployed; we still want
    // to rewrite every IDE that might have been touched. Fall back to the
    // registry entry's targets, or if the server was removed from the
    // registry, use the union of IDEs that currently support MCP.
    let reg = McpRegistry::load()?;
    let ides: Vec<String> = reg
        .get(server_name)
        .map(|s| s.targets.clone())
        .unwrap_or_else(|| {
            crate::mcp::adapters::SUPPORTED
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    proj.mcp_servers.retain(|n| n != server_name);
    store.save()?;

    let plan = sync::plan(&reg, &ides);
    sync::execute(&reg, &plan, Some(project_path))
}

/// Reverse lookup: names of projects whose `mcp_servers` contains
/// `server_name`. Sorted by project name.
pub fn projects_with(server_name: &str) -> Result<Vec<String>> {
    let store = ProjectStore::load()?;
    let mut out: Vec<String> = store
        .list()
        .filter(|p| p.mcp_servers.iter().any(|n| n == server_name))
        .map(|p| p.name.clone())
        .collect();
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::model::{McpServer, McpTransport};
    use crate::projects::Project;
    use std::collections::BTreeMap;
    use std::sync::MutexGuard;

    struct Isolated {
        _dir: tempfile::TempDir,
        _guard: MutexGuard<'static, ()>,
    }

    /// Point aiem-core's config layout at a fresh temp dir for this test.
    fn isolate() -> Isolated {
        let guard = crate::test_support::lock();
        let dir = tempfile::tempdir().expect("tempdir");
        // `paths::` looks at AIEM_HOME for its root. See paths.rs.
        std::env::set_var("AIEM_HOME", dir.path());
        Isolated { _dir: dir, _guard: guard }
    }

    fn stdio_server(name: &str, targets: &[&str]) -> McpServer {
        McpServer {
            name: name.to_string(),
            transport: McpTransport::Stdio {
                command: "echo".into(),
                args: vec!["hi".into()],
                env: BTreeMap::new(),
                cwd: None,
            },
            targets: targets.iter().map(|s| s.to_string()).collect(),
            description: None,
            tags: vec![],
            disabled: false,
        }
    }

    fn register_server(s: McpServer) {
        let mut reg = McpRegistry::load().unwrap();
        reg.upsert(s);
        reg.save().unwrap();
    }

    fn register_project(path: &Path, name: &str) {
        let mut store = ProjectStore::load().unwrap();
        store.upsert(Project {
            name: name.to_string(),
            path: path.to_string_lossy().to_string(),
            ides: vec![],
            skills: vec![],
            mcp_servers: vec![],
        });
        store.save().unwrap();
    }

    #[test]
    fn deploy_adds_server_and_is_idempotent() {
        let _home = isolate();
        let proj = tempfile::tempdir().unwrap();
        register_server(stdio_server("alpha", &["claude-code"]));
        register_project(proj.path(), "demo");

        let touched = deploy_to_project("alpha", proj.path()).unwrap();
        assert!(!touched.is_empty(), "should touch at least one config");

        // Second call must not duplicate the entry.
        deploy_to_project("alpha", proj.path()).unwrap();
        let store = ProjectStore::load().unwrap();
        let p = store.get(&proj.path().to_string_lossy()).unwrap();
        assert_eq!(
            p.mcp_servers.iter().filter(|n| n.as_str() == "alpha").count(),
            1
        );
    }

    #[test]
    fn undeploy_removes_server() {
        let _home = isolate();
        let proj = tempfile::tempdir().unwrap();
        register_server(stdio_server("beta", &["claude-code"]));
        register_project(proj.path(), "demo");

        deploy_to_project("beta", proj.path()).unwrap();
        undeploy_from_project("beta", proj.path()).unwrap();

        let store = ProjectStore::load().unwrap();
        let p = store.get(&proj.path().to_string_lossy()).unwrap();
        assert!(!p.mcp_servers.iter().any(|n| n == "beta"));
    }

    #[test]
    fn deploy_unknown_server_errors() {
        let _home = isolate();
        let proj = tempfile::tempdir().unwrap();
        register_project(proj.path(), "demo");
        let err = deploy_to_project("missing", proj.path()).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[test]
    fn deploy_unregistered_project_errors() {
        let _home = isolate();
        let proj = tempfile::tempdir().unwrap();
        register_server(stdio_server("gamma", &["claude-code"]));
        let err = deploy_to_project("gamma", proj.path()).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[test]
    fn projects_with_reverse_lookup() {
        let _home = isolate();
        let p1 = tempfile::tempdir().unwrap();
        let p2 = tempfile::tempdir().unwrap();
        register_server(stdio_server("delta", &["claude-code"]));
        register_project(p1.path(), "one");
        register_project(p2.path(), "two");

        deploy_to_project("delta", p1.path()).unwrap();
        deploy_to_project("delta", p2.path()).unwrap();

        let names = projects_with("delta").unwrap();
        assert_eq!(names, vec!["one".to_string(), "two".to_string()]);
    }
}
