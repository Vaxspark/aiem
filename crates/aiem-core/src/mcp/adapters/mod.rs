//! Per-IDE adapters that know how to read & write their native MCP config files.
//!
//! Supported targets:
//! - `codex`        → `~/.codex/config.toml`           (TOML, `[mcp_servers.<name>]`)
//! - `claude-code`  → `~/.claude.json`                 (JSON, `mcpServers`)
//! - `copilot`      → `~/.config/Code/User/mcp.json`   (JSON, `servers`)
//!                   or project `.vscode/mcp.json` when a project root is given.

pub mod codex;
pub mod claude_code;
pub mod copilot;

use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::model::McpServer;

/// Where should the config file live for a given IDE / scope?
pub fn config_path(ide_id: &str, project_root: Option<&Path>) -> Result<PathBuf> {
    match ide_id {
        "codex" => codex::config_path(project_root),
        "claude-code" => claude_code::config_path(project_root),
        "copilot" | "vscode" => copilot::config_path(project_root),
        other => Err(Error::Unsupported(format!(
            "MCP sync not supported for IDE `{other}` yet"
        ))),
    }
}

/// Write the full set of managed servers to the IDE's native config. Preserves
/// any unmanaged keys that already exist in the file.
pub fn apply(
    ide_id: &str,
    project_root: Option<&Path>,
    servers: &[McpServer],
) -> Result<PathBuf> {
    match ide_id {
        "codex" => codex::apply(project_root, servers),
        "claude-code" => claude_code::apply(project_root, servers),
        "copilot" | "vscode" => copilot::apply(project_root, servers),
        other => Err(Error::Unsupported(format!(
            "MCP sync not supported for IDE `{other}` yet"
        ))),
    }
}

/// Remove the named servers from the IDE's native config (leaves unmanaged keys alone).
pub fn retract(
    ide_id: &str,
    project_root: Option<&Path>,
    names: &[String],
) -> Result<PathBuf> {
    match ide_id {
        "codex" => codex::retract(project_root, names),
        "claude-code" => claude_code::retract(project_root, names),
        "copilot" | "vscode" => copilot::retract(project_root, names),
        other => Err(Error::Unsupported(format!(
            "MCP sync not supported for IDE `{other}` yet"
        ))),
    }
}

/// List of IDE ids that currently support MCP sync.
pub const SUPPORTED: &[&str] = &["codex", "claude-code", "copilot"];

/// Read MCP servers currently configured in a given IDE's native config file.
pub fn read(ide_id: &str, project_root: Option<&Path>) -> Result<Vec<McpServer>> {
    match ide_id {
        "codex" => codex::read(project_root),
        "claude-code" => claude_code::read(project_root),
        "copilot" | "vscode" => copilot::read(project_root),
        other => Err(Error::Unsupported(format!(
            "MCP read not supported for IDE `{other}` yet"
        ))),
    }
}
