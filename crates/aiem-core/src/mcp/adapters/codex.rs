//! Codex adapter: reads/writes `~/.codex/config.toml`.
//!
//! Codex's MCP config lives under the `[mcp_servers.<name>]` table with fields
//! `command`, `args`, `env`. Codex currently only supports the stdio transport,
//! so HTTP/SSE servers are skipped with a warning.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::fs_util::{atomic_write, backup_file};
use crate::mcp::model::{McpServer, McpTransport};
use crate::{Error, Result};

pub fn config_path(project_root: Option<&Path>) -> Result<PathBuf> {
    let root = match project_root {
        Some(p) => p.to_path_buf(),
        None => dirs::home_dir().ok_or_else(|| Error::Invalid("cannot locate home dir".into()))?,
    };
    Ok(root.join(".codex").join("config.toml"))
}

fn load(path: &Path) -> Result<toml::value::Table> {
    if !path.exists() {
        return Ok(toml::value::Table::new());
    }
    let s = std::fs::read_to_string(path)?;
    let v: Value = toml::from_str(&s)?;
    match v {
        Value::Table(t) => Ok(t),
        _ => Err(Error::Invalid(format!("expected TOML table at {path:?}"))),
    }
}

fn server_to_toml(s: &McpServer) -> Option<Value> {
    match &s.transport {
        McpTransport::Stdio {
            command,
            args,
            env,
            cwd,
            bundle: _,
        } => {
            let mut t = toml::value::Table::new();
            t.insert("command".into(), Value::String(command.clone()));
            if !args.is_empty() {
                t.insert(
                    "args".into(),
                    Value::Array(args.iter().cloned().map(Value::String).collect()),
                );
            }
            if !env.is_empty() {
                let mut e = toml::value::Table::new();
                for (k, v) in env {
                    e.insert(k.clone(), Value::String(v.clone()));
                }
                t.insert("env".into(), Value::Table(e));
            }
            if let Some(cwd) = cwd {
                t.insert("cwd".into(), Value::String(cwd.clone()));
            }
            Some(Value::Table(t))
        }
        McpTransport::Http { .. } | McpTransport::Sse { .. } => {
            tracing::warn!(name = %s.name, "Codex currently supports stdio only; skipping");
            None
        }
    }
}

pub fn apply(project_root: Option<&Path>, servers: &[McpServer]) -> Result<PathBuf> {
    let path = config_path(project_root)?;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    backup_file(&path, "codex")?;

    let mut root = load(&path)?;
    let section = root
        .entry("mcp_servers")
        .or_insert(Value::Table(Default::default()));
    let table = match section {
        Value::Table(t) => t,
        _ => return Err(Error::Invalid("`mcp_servers` must be a TOML table".into())),
    };

    for s in servers {
        if s.disabled {
            continue;
        }
        if let Some(v) = server_to_toml(s) {
            table.insert(s.name.clone(), v);
        }
    }

    let out = toml::to_string_pretty(&Value::Table(root))?;
    atomic_write(&path, out.as_bytes())?;
    Ok(path)
}

pub fn retract(project_root: Option<&Path>, names: &[String]) -> Result<PathBuf> {
    let path = config_path(project_root)?;
    if !path.exists() {
        return Ok(path);
    }
    backup_file(&path, "codex")?;

    let mut root = load(&path)?;
    if let Some(Value::Table(t)) = root.get_mut("mcp_servers") {
        for n in names {
            t.remove(n);
        }
    }
    let out = toml::to_string_pretty(&Value::Table(root))?;
    atomic_write(&path, out.as_bytes())?;
    Ok(path)
}

/// Read servers currently present in Codex's config (useful for `diff` / `import`).
pub fn read(project_root: Option<&Path>) -> Result<Vec<McpServer>> {
    let path = config_path(project_root)?;
    let root = load(&path)?;
    let mut out = Vec::new();
    if let Some(Value::Table(t)) = root.get("mcp_servers") {
        for (name, val) in t {
            let Value::Table(st) = val else { continue };
            let command = st
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = st
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let env = st
                .get("env")
                .and_then(|v| v.as_table())
                .map(|e| {
                    e.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect::<BTreeMap<_, _>>()
                })
                .unwrap_or_default();
            let cwd = st.get("cwd").and_then(|v| v.as_str()).map(String::from);
            out.push(McpServer {
                name: name.clone(),
                transport: McpTransport::Stdio {
                    command,
                    args,
                    env,
                    cwd,
                    bundle: None,
                },
                targets: vec!["codex".into()],
                description: None,
                tags: vec![],
                source: None,
                runtime: None,
                auth_mode: Default::default(),
                disabled: false,
            });
        }
    }
    Ok(out)
}
