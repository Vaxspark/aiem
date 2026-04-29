//! Cursor IDE adapter: reads/writes `~/.cursor/mcp.json` (global) or
//! `<project>/.cursor/mcp.json` (project-scoped).
//!
//! Cursor's MCP config uses a top-level `mcpServers` object (same key as
//! Claude Code).  Each entry is a server definition with `command`/`args`/`env`
//! for stdio, or `url`/`headers` for HTTP/SSE transports.
//!
//! Unlike Claude Code, Cursor infers stdio from the presence of `command`
//! (no explicit `type` field needed), and uses `url` for remote servers.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::fs_util::{atomic_write, backup_file};
use crate::mcp::model::{McpServer, McpTransport};
use crate::{Error, Result};

pub fn config_path(project_root: Option<&Path>) -> Result<PathBuf> {
    match project_root {
        Some(p) => Ok(p.join(".cursor").join("mcp.json")),
        None => {
            let home =
                dirs::home_dir().ok_or_else(|| Error::Invalid("cannot locate home dir".into()))?;
            Ok(home.join(".cursor").join("mcp.json"))
        }
    }
}

fn load(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let data = crate::fs_util::strip_utf8_bom(&bytes);
    let v: Value = serde_json::from_slice(data)?;
    if !v.is_object() {
        return Err(Error::Invalid(format!("expected JSON object at {path:?}")));
    }
    Ok(v)
}

fn server_to_json(s: &McpServer) -> Value {
    match &s.transport {
        McpTransport::Stdio {
            command,
            args,
            env,
            cwd,
            bundle: _,
        } => {
            let mut obj = Map::new();
            obj.insert("command".into(), json!(command));
            if !args.is_empty() {
                obj.insert("args".into(), json!(args));
            }
            if !env.is_empty() {
                obj.insert("env".into(), json!(env));
            }
            if let Some(cwd) = cwd {
                obj.insert("cwd".into(), json!(cwd));
            }
            Value::Object(obj)
        }
        McpTransport::Http { url, headers } => {
            let mut obj = Map::new();
            obj.insert("url".into(), json!(url));
            if !headers.is_empty() {
                obj.insert("headers".into(), json!(headers));
            }
            Value::Object(obj)
        }
        McpTransport::Sse { url, headers } => {
            let mut obj = Map::new();
            obj.insert("url".into(), json!(url));
            if !headers.is_empty() {
                obj.insert("headers".into(), json!(headers));
            }
            Value::Object(obj)
        }
    }
}

pub fn apply(project_root: Option<&Path>, servers: &[McpServer]) -> Result<PathBuf> {
    let path = config_path(project_root)?;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    backup_file(&path, "cursor")?;

    let mut root = load(&path)?;
    let obj = root.as_object_mut().expect("object checked in load");
    let section = obj.entry("mcpServers").or_insert(Value::Object(Map::new()));
    let map = section
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("`mcpServers` must be an object".into()))?;

    for s in servers {
        if s.disabled {
            continue;
        }
        map.insert(s.name.clone(), server_to_json(s));
    }

    let out = serde_json::to_vec_pretty(&root)?;
    atomic_write(&path, &out)?;
    Ok(path)
}

pub fn retract(project_root: Option<&Path>, names: &[String]) -> Result<PathBuf> {
    let path = config_path(project_root)?;
    if !path.exists() {
        return Ok(path);
    }
    backup_file(&path, "cursor")?;
    let mut root = load(&path)?;
    if let Some(map) = root.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        for n in names {
            map.remove(n);
        }
    }
    let out = serde_json::to_vec_pretty(&root)?;
    atomic_write(&path, &out)?;
    Ok(path)
}

pub fn read(project_root: Option<&Path>) -> Result<Vec<McpServer>> {
    let path = config_path(project_root)?;
    let root = load(&path)?;
    let mut out = Vec::new();
    let Some(servers) = root.get("mcpServers").and_then(|v| v.as_object()) else {
        return Ok(out);
    };
    for (name, val) in servers {
        let Some(obj) = val.as_object() else { continue };
        let transport = parse_transport(obj);
        out.push(McpServer {
            name: name.clone(),
            transport,
            targets: vec!["cursor".into()],
            description: None,
            tags: vec![],
            disabled: false,
            source: None,
            runtime: None,
            auth_mode: Default::default(),
        });
    }
    Ok(out)
}

fn parse_transport(obj: &Map<String, Value>) -> McpTransport {
    if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
        let headers = parse_str_map(obj.get("headers"));
        return McpTransport::Sse {
            url: url.to_string(),
            headers,
        };
    }
    McpTransport::Stdio {
        command: obj
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        args: obj
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        env: parse_str_map(obj.get("env")),
        cwd: obj.get("cwd").and_then(|v| v.as_str()).map(String::from),
        bundle: None,
    }
}

fn parse_str_map(v: Option<&Value>) -> std::collections::BTreeMap<String, String> {
    v.and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}
