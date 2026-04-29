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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::mcp::model::{McpAuthMode, McpRuntime, McpTransport};
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
pub fn deploy_to_project(server_name: &str, project_path: &Path) -> Result<Vec<(String, PathBuf)>> {
    deploy_to_project_for_ides(server_name, project_path, &[])
}

/// Deploy `server_name` to a project, limiting the write to the provided IDEs.
/// When `only_ides` is empty, the server's saved target IDEs are used.
pub fn deploy_to_project_for_ides(
    server_name: &str,
    project_path: &Path,
    only_ides: &[String],
) -> Result<Vec<(String, PathBuf)>> {
    let reg = McpRegistry::load()?;
    let server = reg
        .get(server_name)
        .ok_or_else(|| Error::NotFound(format!("mcp server `{server_name}` not found")))?
        .clone();

    // Fail early if the server declares a bundle that is missing on disk.
    if let McpTransport::Stdio {
        bundle: Some(ref b),
        ..
    } = &server.transport
    {
        let src = crate::mcp::bundles::bundle_path(b)?;
        if !src.exists() {
            return Err(Error::Invalid(format!(
                "bundle `{b}` is missing from {}; import it first before deploying",
                src.display()
            )));
        }
    }

    let path_key = project_path.to_string_lossy().to_string();
    let mut store = ProjectStore::load()?;
    let proj = store
        .get_mut(&path_key)
        .ok_or_else(|| Error::NotFound(format!("project `{path_key}` not registered")))?;

    if !proj.mcp_servers.iter().any(|n| n == server_name) {
        proj.mcp_servers.push(server_name.to_string());
    }
    store.save()?;

    // Reload to get the canonical `mcp_servers` list, then plan only those names
    // (plus retract stale registry entries in execute).
    let store = ProjectStore::load()?;
    let allowed = store
        .get(&path_key)
        .map(|p| p.mcp_servers.as_slice())
        .ok_or_else(|| Error::NotFound(format!("project `{path_key}` not registered")))?;

    let only_ides = normalize_ides(only_ides, &server.targets);
    let plan = sync::plan(&reg, &only_ides, Some(allowed));
    let touched = sync::execute(&reg, &plan, Some(project_path), Some(allowed))?;

    // After IDE config is written, update the project manifest.
    update_project_manifest(project_path, &reg)?;

    Ok(touched)
}

/// Remove `server_name` from the project's `mcp_servers` list and re-sync so
/// the project config files no longer reference it.
///
/// Also cleans up `.aiem-mcp/<bundle>/` directories that are no longer referenced.
///
/// Errors:
/// - `NotFound` if `project_path` is not a registered project
pub fn undeploy_from_project(
    server_name: &str,
    project_path: &Path,
) -> Result<Vec<(String, PathBuf)>> {
    undeploy_from_project_for_ides(server_name, project_path, &[])
}

/// Retract `server_name` from a project for selected IDEs. If no selected IDE
/// still contains the server after the retract, the project association is
/// removed and bundle cleanup runs as usual.
pub fn undeploy_from_project_for_ides(
    server_name: &str,
    project_path: &Path,
    only_ides: &[String],
) -> Result<Vec<(String, PathBuf)>> {
    let path_key = project_path.to_string_lossy().to_string();
    let mut store = ProjectStore::load()?;
    let proj = store
        .get_mut(&path_key)
        .ok_or_else(|| Error::NotFound(format!("project `{path_key}` not registered")))?;

    let reg = McpRegistry::load()?;
    let ides: Vec<String> = reg
        .get(server_name)
        .map(|s| normalize_ides(only_ides, &s.targets))
        .unwrap_or_else(|| {
            crate::mcp::adapters::SUPPORTED
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    // Collect bundle name of the server being undeployed so we can clean up.
    let removed_bundle: Option<String> = reg.get(server_name).and_then(|s| {
        if let McpTransport::Stdio {
            bundle: Some(b), ..
        } = &s.transport
        {
            Some(b.clone())
        } else {
            None
        }
    });

    let names = vec![server_name.to_string()];
    let mut touched = Vec::new();
    for ide in &ides {
        let path = crate::mcp::adapters::retract(ide, Some(project_path), &names)?;
        touched.push((ide.clone(), path));
    }

    let still_configured = crate::mcp::adapters::SUPPORTED.iter().any(|ide| {
        crate::mcp::adapters::read(ide, Some(project_path))
            .map(|servers| servers.iter().any(|s| s.name == server_name))
            .unwrap_or(false)
    });

    if !still_configured {
        proj.mcp_servers.retain(|n| n != server_name);
    }
    let allowed: Vec<String> = proj.mcp_servers.clone();
    store.save()?;

    // Clean up bundle directory if no remaining server references it.
    if !still_configured {
        if let Some(ref bundle_name) = removed_bundle {
            let still_used = allowed.iter().any(|name| {
                reg.get(name)
                    .map(|s| {
                        matches!(&s.transport, McpTransport::Stdio { bundle: Some(b), .. } if b == bundle_name)
                    })
                    .unwrap_or(false)
            });
            if !still_used {
                let bundle_dir = project_path.join(".aiem-mcp").join(bundle_name);
                if bundle_dir.exists() {
                    let _ = std::fs::remove_dir_all(&bundle_dir);
                }
            }
        }
    }

    // Update manifest (removing the server's entry) and clean empty .aiem-mcp.
    update_project_manifest(project_path, &reg)?;
    cleanup_empty_aiem_mcp(project_path);

    Ok(touched)
}

fn normalize_ides(selected: &[String], fallback: &[String]) -> Vec<String> {
    let source = if selected.is_empty() {
        fallback
    } else {
        selected
    };
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for ide in source {
        let canonical = crate::mcp::adapters::canonical_id(ide).to_string();
        if seen.insert(canonical.clone()) {
            out.push(canonical);
        }
    }
    out
}

// ─── Manifest & AIEM_MCP.md generation ──────────────────────────────────────

/// Per-server entry in the project manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub dep_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
}

/// Full manifest written to `<project>/.aiem-mcp/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub version: u32,
    pub servers: Vec<ManifestEntry>,
}

/// Rebuild `<project>/.aiem-mcp/manifest.json` based on the project's current
/// `mcp_servers` list and the registry state. Also generates `AIEM_MCP.md`
/// inside each deployed bundle directory.
fn update_project_manifest(project_path: &Path, reg: &McpRegistry) -> Result<()> {
    let store = ProjectStore::load()?;
    let path_key = project_path.to_string_lossy().to_string();
    let Some(proj) = store.get(&path_key) else {
        return Ok(());
    };

    let aiem_dir = project_path.join(".aiem-mcp");
    let mut entries = Vec::new();

    for name in &proj.mcp_servers {
        let Some(srv) = reg.get(name) else { continue };
        let (bundle, runtime_str, entrypoint, dep_files) = if let McpTransport::Stdio {
            bundle: Some(b),
            args,
            ..
        } = &srv.transport
        {
            let bundle_dir = aiem_dir.join(b);
            let rt = srv.runtime.map(|r| format!("{:?}", r).to_lowercase());
            // Prefer the entry point recorded at import time (first arg) over guessing.
            let ep = args
                .first()
                .filter(|a| !a.starts_with('-'))
                .cloned()
                .or_else(|| find_entrypoint(&bundle_dir));
            let deps = find_dep_files(&bundle_dir);

            if bundle_dir.is_dir() {
                let _ = write_aiem_mcp_md(&bundle_dir, srv, &ep, &deps);
            }

            (Some(b.clone()), rt, ep, deps)
        } else {
            (None, None, None, vec![])
        };

        let source_repo = srv
            .source
            .as_ref()
            .map(|s| format!("{}/{}", s.owner, s.repo));
        let commit = srv.source.as_ref().and_then(|s| s.commit.clone());
        let auth = match &srv.auth_mode {
            McpAuthMode::None => None,
            other => Some(format!("{:?}", other).to_lowercase()),
        };

        entries.push(ManifestEntry {
            server: name.clone(),
            bundle,
            source_repo,
            commit,
            runtime: runtime_str,
            entrypoint,
            dep_files,
            auth_mode: auth,
            targets: srv.targets.clone(),
        });
    }

    if entries.is_empty() {
        // No servers deployed → remove manifest if it exists.
        let manifest_path = aiem_dir.join("manifest.json");
        if manifest_path.exists() {
            let _ = std::fs::remove_file(&manifest_path);
        }
        return Ok(());
    }

    std::fs::create_dir_all(&aiem_dir)?;
    let manifest = ProjectManifest {
        version: 1,
        servers: entries,
    };
    let data = serde_json::to_vec_pretty(&manifest)?;
    crate::fs_util::atomic_write(&aiem_dir.join("manifest.json"), &data)?;
    Ok(())
}

fn find_entrypoint(bundle_dir: &Path) -> Option<String> {
    for name in &["server.py", "main.py", "app.py", "index.js", "index.ts"] {
        if bundle_dir.join(name).is_file() {
            return Some(name.to_string());
        }
    }
    None
}

fn find_dep_files(bundle_dir: &Path) -> Vec<String> {
    let candidates = [
        "requirements.txt",
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "uv.lock",
        "poetry.lock",
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
    ];
    candidates
        .iter()
        .filter(|f| bundle_dir.join(f).is_file())
        .map(|f| f.to_string())
        .collect()
}

fn write_aiem_mcp_md(
    bundle_dir: &Path,
    srv: &crate::mcp::model::McpServer,
    entrypoint: &Option<String>,
    dep_files: &[String],
) -> Result<()> {
    use std::fmt::Write;
    let mut md = String::new();
    writeln!(md, "# {}", srv.name).unwrap();
    writeln!(md).unwrap();
    if let Some(desc) = &srv.description {
        writeln!(md, "{desc}").unwrap();
        writeln!(md).unwrap();
    }

    writeln!(md, "## Setup").unwrap();
    writeln!(md).unwrap();

    let rt = srv.runtime.unwrap_or(McpRuntime::Other);
    if rt == McpRuntime::Python {
        writeln!(md, "This is a Python MCP server.").unwrap();
        writeln!(md).unwrap();
        writeln!(md, "```bash").unwrap();
        writeln!(md, "cd {}", bundle_dir.display()).unwrap();
        if dep_files.iter().any(|f| f == "pyproject.toml") {
            writeln!(md, "uv sync  # or: pip install -e .").unwrap();
        } else if dep_files.iter().any(|f| f.starts_with("requirements")) {
            writeln!(
                md,
                "uv pip install -r requirements.txt  # or: pip install -r requirements.txt"
            )
            .unwrap();
        }
        if let Some(ep) = entrypoint {
            writeln!(md, "python {ep}").unwrap();
        }
        writeln!(md, "```").unwrap();
    } else if rt == McpRuntime::Node {
        writeln!(md, "This is a Node.js MCP server.").unwrap();
        writeln!(md).unwrap();
        writeln!(md, "```bash").unwrap();
        writeln!(md, "cd {}", bundle_dir.display()).unwrap();
        writeln!(md, "npm install").unwrap();
        if let Some(ep) = entrypoint {
            writeln!(md, "node {ep}").unwrap();
        }
        writeln!(md, "```").unwrap();
    } else {
        writeln!(md, "See the server documentation for setup instructions.").unwrap();
    }

    writeln!(md).unwrap();
    writeln!(md, "## Transport").unwrap();
    writeln!(md).unwrap();
    match &srv.transport {
        McpTransport::Stdio { command, args, .. } => {
            writeln!(md, "- Type: stdio").unwrap();
            writeln!(md, "- Command: `{command} {}`", args.join(" ")).unwrap();
        }
        McpTransport::Http { url, .. } => {
            writeln!(md, "- Type: HTTP").unwrap();
            writeln!(md, "- URL: `{url}`").unwrap();
        }
        McpTransport::Sse { url, .. } => {
            writeln!(md, "- Type: SSE").unwrap();
            writeln!(md, "- URL: `{url}`").unwrap();
        }
    }

    if srv.auth_mode == McpAuthMode::External {
        writeln!(md).unwrap();
        writeln!(md, "## Authentication").unwrap();
        writeln!(md).unwrap();
        writeln!(
            md,
            "This server requires external authentication (e.g. browser OAuth). \
             The first connection may trigger a login flow."
        )
        .unwrap();
    }

    std::fs::write(bundle_dir.join("AIEM_MCP.md"), md.as_bytes())?;
    Ok(())
}

/// Remove `.aiem-mcp/` if it's empty (no bundles, no manifest).
fn cleanup_empty_aiem_mcp(project_path: &Path) {
    let dir = project_path.join(".aiem-mcp");
    if !dir.exists() {
        return;
    }
    let is_empty = std::fs::read_dir(&dir)
        .map(|mut rd| rd.next().is_none())
        .unwrap_or(true);
    if is_empty {
        let _ = std::fs::remove_dir(&dir);
    }
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
        Isolated {
            _dir: dir,
            _guard: guard,
        }
    }

    fn stdio_server(name: &str, targets: &[&str]) -> McpServer {
        McpServer {
            name: name.to_string(),
            transport: McpTransport::Stdio {
                command: "echo".into(),
                args: vec!["hi".into()],
                env: BTreeMap::new(),
                cwd: None,
                bundle: None,
            },
            targets: targets.iter().map(|s| s.to_string()).collect(),
            description: None,
            tags: vec![],
            disabled: false,
            source: None,
            runtime: None,
            auth_mode: Default::default(),
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
            p.mcp_servers
                .iter()
                .filter(|n| n.as_str() == "alpha")
                .count(),
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

    /// Project-scoped sync must not write the whole registry to `.mcp.json` — only
    /// servers listed on the project.
    #[test]
    fn deploy_to_project_config_lists_only_attached_servers() {
        let _home = isolate();
        let proj = tempfile::tempdir().unwrap();
        register_server(stdio_server("only-one", &["claude-code"]));
        register_server(stdio_server("other-reg", &["claude-code"]));
        register_project(proj.path(), "demo");

        deploy_to_project("only-one", proj.path()).unwrap();

        let path = proj.path().join(".mcp.json");
        assert!(path.exists(), "expected project .mcp.json");
        let val: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let keys: Vec<String> = val
            .get("mcpServers")
            .and_then(|v| v.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        assert_eq!(keys, vec!["only-one".to_string()]);
    }
}
