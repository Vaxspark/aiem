//! High-level MCP sync orchestration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::mcp::adapters;
use crate::mcp::model::{McpServer, McpTransport};
use crate::mcp::registry::McpRegistry;
use crate::profiles::ProfileStore;
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
/// If a profile is active and has a non-empty `mcp_servers` list, servers not
/// in that list are skipped.
pub fn plan(reg: &McpRegistry, only_ides: &[String]) -> SyncPlan {
    let active_filter: Option<Vec<String>> = ProfileStore::load()
        .ok()
        .and_then(|s| s.active().cloned())
        .map(|p| p.mcp_servers)
        .filter(|v| !v.is_empty());

    let mut plan = SyncPlan::default();
    for s in reg.list() {
        if s.disabled { continue; }
        if let Some(ref allow) = active_filter {
            if !allow.iter().any(|n| n == &s.name) { continue; }
        }
        for ide in &s.targets {
            if !only_ides.is_empty() && !only_ides.iter().any(|x| x == ide) { continue; }
            plan.writes.entry(ide.clone()).or_default().push(s.name.clone());
        }
    }
    plan
}

/// Execute the plan: for each target IDE, call its adapter with the set of
/// managed servers. Returns the list of config files touched.
pub fn execute(
    reg: &McpRegistry,
    plan: &SyncPlan,
    project_root: Option<&Path>,
) -> Result<Vec<(String, PathBuf)>> {
    let mut touched = Vec::new();
    for (ide, names) in &plan.writes {
        let servers: Vec<McpServer> = names
            .iter()
            .filter_map(|n| reg.get(n).cloned())
            .map(expand_server_secrets)
            .collect();
        let path = adapters::apply(ide, project_root, &servers)?;
        touched.push((ide.clone(), path));
    }
    Ok(touched)
}

/// Return a clone of `s` with `${secret:NAME}` placeholders resolved via the
/// OS keyring. Placeholders that can't be resolved are left intact.
fn expand_server_secrets(mut s: McpServer) -> McpServer {
    match &mut s.transport {
        McpTransport::Stdio { args, env, .. } => {
            for a in args.iter_mut() { *a = secrets::expand(a); }
            for (_, v) in env.iter_mut() { *v = secrets::expand(v); }
        }
        McpTransport::Http { url, headers } | McpTransport::Sse { url, headers } => {
            *url = secrets::expand(url);
            for (_, v) in headers.iter_mut() { *v = secrets::expand(v); }
        }
    }
    s
}
