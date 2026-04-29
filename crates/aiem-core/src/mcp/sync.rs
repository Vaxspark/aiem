//! High-level MCP sync orchestration.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::mcp::adapters;
use crate::mcp::model::{McpServer, McpTransport};
use crate::mcp::registry::McpRegistry;
use crate::secrets;
use crate::Result;

/// Plan describing which servers will be written to which IDEs.
#[derive(Debug, Default)]
pub struct SyncPlan {
    /// ide_id -> list of server names to write
    pub writes: BTreeMap<String, Vec<String>>,
}

/// Build a sync plan from the registry. If `only_ides` is non-empty, limit to
/// those IDEs; otherwise every IDE referenced by at least one server is included.
///
/// If `only_servers` is `Some(names)`, only those server **names** are included
/// (useful for project-scoped deploy so the project config is not filled with
/// the entire registry).
pub fn plan(reg: &McpRegistry, only_ides: &[String], only_servers: Option<&[String]>) -> SyncPlan {
    let name_filter: Option<HashSet<&str>> =
        only_servers.map(|names| names.iter().map(String::as_str).collect());

    let mut plan = SyncPlan::default();
    let only_ides: HashSet<String> = only_ides
        .iter()
        .map(|ide| adapters::canonical_id(ide).to_string())
        .collect();

    for s in reg.list() {
        if s.disabled {
            continue;
        }
        if let Some(ref names) = name_filter {
            if !names.contains(s.name.as_str()) {
                continue;
            }
        }
        for ide in &s.targets {
            let ide = adapters::canonical_id(ide);
            if !only_ides.is_empty() && !only_ides.contains(ide) {
                continue;
            }
            plan.writes
                .entry(ide.to_string())
                .or_default()
                .push(s.name.clone());
        }
    }
    plan
}

/// Execute the plan: for each target IDE, call its adapter with the set of
/// managed servers. Returns the list of config files touched.
///
/// When syncing to a **project** (`project_root` is `Some`) and
/// `project_mcp_allowlist` is `Some(allowed_names)`, first removes from each
/// IDE’s project file every **registry** server that targets that IDE but is
/// *not* in `allowed_names` (so stale entries from older buggy syncs are
/// dropped). `allowed_names` should match
/// [`crate::projects::Project::mcp_servers`]. When the slice is **empty**,
/// all registry server entries are removed from the project’s IDE config files
/// (unmanaged / manual keys are preserved by [`adapters::retract`]).
pub fn execute(
    reg: &McpRegistry,
    plan: &SyncPlan,
    project_root: Option<&Path>,
    project_mcp_allowlist: Option<&[String]>,
) -> Result<Vec<(String, PathBuf)>> {
    let mut touched = Vec::new();

    if let (Some(root), Some(allow)) = (project_root, project_mcp_allowlist) {
        if allow.is_empty() {
            for &ide in adapters::SUPPORTED {
                let to_retract: Vec<String> = reg
                    .list()
                    .filter(|srv| srv.targets.iter().any(|t| adapters::canonical_id(t) == ide))
                    .map(|srv| srv.name.clone())
                    .collect();
                if !to_retract.is_empty() {
                    let p = adapters::retract(ide, Some(root), &to_retract)?;
                    touched.push((ide.to_string(), p));
                }
            }
            return Ok(touched);
        }
    }

    let allow_set: Option<HashSet<&str>> =
        project_mcp_allowlist.map(|v| v.iter().map(String::as_str).collect());

    // Collect all planned names for efficient lookup.
    let planned_names: HashSet<&str> = plan
        .writes
        .values()
        .flat_map(|v| v.iter().map(String::as_str))
        .collect();

    for (ide, names) in &plan.writes {
        // For project syncs: retract servers that are NOT in the allowlist.
        if let (Some(root), Some(allow)) = (project_root, &allow_set) {
            let to_retract: Vec<String> = reg
                .list()
                .filter(|srv| {
                    srv.targets.iter().any(|t| adapters::canonical_id(t) == ide)
                        && !allow.contains(srv.name.as_str())
                })
                .map(|srv| srv.name.clone())
                .collect();
            if !to_retract.is_empty() {
                adapters::retract(ide, Some(root), &to_retract)?;
            }
        }

        // For global syncs: retract servers that exist in the IDE config
        // but are NOT in the current plan (removed/disabled/not targeted).
        if project_root.is_none() {
            if let Ok(current) = adapters::read(ide, None) {
                let stale: Vec<String> = current
                    .iter()
                    .filter(|s| !planned_names.contains(s.name.as_str()))
                    .map(|s| s.name.clone())
                    .collect();
                if !stale.is_empty() {
                    let _ = adapters::retract(ide, None, &stale);
                }
            }
        }

        let mut servers: Vec<McpServer> = Vec::with_capacity(names.len());
        for n in names {
            let Some(s) = reg.get(n).cloned() else {
                continue;
            };
            let s = expand_server_secrets(s);
            let s = materialize_bundle(s, project_root)?;
            servers.push(s);
        }
        let path = adapters::apply(ide, project_root, &servers)?;
        touched.push((ide.clone(), path));
    }

    // For global syncs: handle IDEs that have no planned servers at all
    // but may still have stale entries from a prior sync.
    if project_root.is_none() {
        for &ide in adapters::SUPPORTED {
            if plan.writes.contains_key(ide) {
                continue;
            }
            if let Ok(current) = adapters::read(ide, None) {
                let stale: Vec<String> = current.iter().map(|s| s.name.clone()).collect();
                if !stale.is_empty() {
                    if let Ok(p) = adapters::retract(ide, None, &stale) {
                        touched.push((ide.to_string(), p));
                    }
                }
            }
        }
    }

    Ok(touched)
}

/// Write a single server to the global configs of the given IDEs.
/// Does NOT retract other servers — only adds/updates this one entry.
pub fn sync_one_global(
    server_name: &str,
    target_ides: &[String],
) -> Result<Vec<(String, PathBuf)>> {
    let reg = McpRegistry::load()?;
    let srv = reg
        .get(server_name)
        .ok_or_else(|| crate::Error::NotFound(format!("mcp server `{server_name}` not found")))?
        .clone();

    let ides = if target_ides.is_empty() {
        srv.targets.clone()
    } else {
        target_ides.to_vec()
    };

    let mut touched = Vec::new();
    let mut seen = BTreeSet::new();
    for ide in &ides {
        let ide = adapters::canonical_id(ide).to_string();
        if !seen.insert(ide.clone()) {
            continue;
        }
        let s = expand_server_secrets(srv.clone());
        let s = materialize_bundle(s, None)?;
        let path = adapters::apply(&ide, None, &[s])?;
        touched.push((ide, path));
    }
    Ok(touched)
}

/// Remove a single server from one or more global IDE configs.
pub fn retract_one_global_from_ides(
    server_name: &str,
    target_ides: &[String],
) -> Result<Vec<(String, PathBuf)>> {
    let names = vec![server_name.to_string()];
    let mut touched = Vec::new();
    let mut seen = BTreeSet::new();
    for ide in target_ides {
        let ide = adapters::canonical_id(ide).to_string();
        if !seen.insert(ide.clone()) {
            continue;
        }
        match adapters::read(&ide, None) {
            Ok(current) => {
                if current.iter().any(|s| s.name == server_name) {
                    if let Ok(p) = adapters::retract(&ide, None, &names) {
                        touched.push((ide, p));
                    }
                }
            }
            Err(_) => {}
        }
    }
    Ok(touched)
}

/// Remove a single server from all global IDE configs it may appear in.
pub fn retract_one_global(server_name: &str) -> Result<Vec<(String, PathBuf)>> {
    let names = vec![server_name.to_string()];
    let mut touched = Vec::new();
    for &ide in adapters::SUPPORTED {
        match adapters::read(ide, None) {
            Ok(current) => {
                if current.iter().any(|s| s.name == server_name) {
                    if let Ok(p) = adapters::retract(ide, None, &names) {
                        touched.push((ide.to_string(), p));
                    }
                }
            }
            Err(_) => {}
        }
    }
    Ok(touched)
}

/// If `s` declares a bundle, copy it into the appropriate on-disk location
/// (`<project>/.aiem-mcp/<name>/` when deploying to a project, otherwise the
/// user's global bundles directory) and expand `{BUNDLE}` placeholders in
/// `command`/`args`/`env`/`cwd`.
fn materialize_bundle(mut s: McpServer, project_root: Option<&Path>) -> Result<McpServer> {
    use crate::mcp::bundles;
    let McpTransport::Stdio {
        command,
        args,
        env,
        cwd,
        bundle: Some(bundle_name),
    } = &mut s.transport
    else {
        return Ok(s);
    };

    let src = bundles::bundle_path(bundle_name)?;
    if !src.exists() {
        tracing::warn!(bundle = %bundle_name, "mcp bundle missing on disk; leaving placeholders untouched");
        return Ok(s);
    }

    let bundle_dir = match project_root {
        Some(root) => {
            let dest = root.join(".aiem-mcp").join(bundle_name.as_str());
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if dest.exists() {
                crate::fs_util::remove_path(&dest)?;
            }
            crate::fs_util::copy_dir_safe(&src, &dest)?;
            dest
        }
        None => src,
    };

    *command = bundles::expand_placeholder(command, &bundle_dir);
    for a in args.iter_mut() {
        *a = bundles::expand_placeholder(a, &bundle_dir);
    }
    for (_, v) in env.iter_mut() {
        *v = bundles::expand_placeholder(v, &bundle_dir);
    }
    if let Some(c) = cwd {
        *c = bundles::expand_placeholder(c, &bundle_dir);
    }
    Ok(s)
}

/// Return a clone of `s` with `${secret:NAME}` placeholders resolved via the
/// OS keyring. Placeholders that can't be resolved are left intact.
fn expand_server_secrets(mut s: McpServer) -> McpServer {
    match &mut s.transport {
        McpTransport::Stdio { args, env, .. } => {
            for a in args.iter_mut() {
                *a = secrets::expand(a);
            }
            for (_, v) in env.iter_mut() {
                *v = secrets::expand(v);
            }
        }
        McpTransport::Http { url, headers } | McpTransport::Sse { url, headers } => {
            *url = secrets::expand(url);
            for (_, v) in headers.iter_mut() {
                *v = secrets::expand(v);
            }
        }
    }
    s
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

    fn stdio_server(name: &str, targets: &[&str]) -> McpServer {
        McpServer {
            name: name.into(),
            transport: McpTransport::Stdio {
                command: "echo".into(),
                args: vec![],
                env: Default::default(),
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

    #[test]
    fn plan_includes_all_when_no_filter() {
        let _h = isolate();
        let mut reg = McpRegistry::load().unwrap();
        reg.upsert(stdio_server("a", &["claude-code"]));
        reg.upsert(stdio_server("b", &["claude-code"]));
        reg.save().unwrap();
        let reg = McpRegistry::load().unwrap();
        let p = plan(&reg, &[], None);
        let names = p.writes.get("claude-code").unwrap();
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
    }

    #[test]
    fn plan_filters_by_only_servers() {
        let _h = isolate();
        let mut reg = McpRegistry::load().unwrap();
        reg.upsert(stdio_server("keep", &["claude-code"]));
        reg.upsert(stdio_server("drop", &["claude-code"]));
        reg.save().unwrap();
        let reg = McpRegistry::load().unwrap();
        let filter = vec!["keep".to_string()];
        let p = plan(&reg, &[], Some(&filter));
        let names = p.writes.get("claude-code").unwrap();
        assert_eq!(names, &vec!["keep".to_string()]);
    }

    #[test]
    fn plan_skips_disabled() {
        let _h = isolate();
        let mut reg = McpRegistry::load().unwrap();
        let mut s = stdio_server("off", &["claude-code"]);
        s.disabled = true;
        reg.upsert(s);
        reg.save().unwrap();
        let reg = McpRegistry::load().unwrap();
        let p = plan(&reg, &[], None);
        assert!(p.writes.is_empty());
    }

    #[test]
    fn plan_filters_by_only_ides() {
        let _h = isolate();
        let mut reg = McpRegistry::load().unwrap();
        reg.upsert(stdio_server("x", &["claude-code", "copilot"]));
        reg.save().unwrap();
        let reg = McpRegistry::load().unwrap();
        let p = plan(&reg, &["copilot".into()], None);
        assert!(p.writes.get("claude-code").is_none());
        assert!(p.writes.get("vscode").is_some());
    }
}
