//! Per-IDE adapters that know how to read & write their native MCP config files.
//!
//! Supported targets:
//! - `codex`        → `~/.codex/config.toml`           (TOML, `[mcp_servers.<name>]`)
//! - `claude-code`  → `~/.claude.json`                 (JSON, `mcpServers`)
//! - `copilot`      → `~/.config/Code/User/mcp.json`   (JSON, `servers`)
//!                   or project `.vscode/mcp.json` when a project root is given.
//! - `cursor`       → `~/.cursor/mcp.json`             (JSON, `mcpServers`)
//!                   or project `.cursor/mcp.json` when a project root is given.

pub mod claude_code;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod json_mcp;

use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::model::McpServer;

/// Where should the config file live for a given IDE / scope?
pub fn config_path(ide_id: &str, project_root: Option<&Path>) -> Result<PathBuf> {
    match canonical_id(ide_id) {
        "codex" => codex::config_path(project_root),
        "claude-code" => claude_code::config_path(project_root),
        "vscode" => copilot::config_path(project_root),
        "cursor" => cursor::config_path(project_root),
        "windsurf" | "trae" | "qoder" | "kiro" => {
            json_mcp::config_path(canonical_id(ide_id), project_root)
        }
        other => Err(Error::Unsupported(format!(
            "MCP sync not supported for IDE `{other}` yet"
        ))),
    }
}

/// Write the full set of managed servers to the IDE's native config. Preserves
/// any unmanaged keys that already exist in the file.
pub fn apply(ide_id: &str, project_root: Option<&Path>, servers: &[McpServer]) -> Result<PathBuf> {
    match canonical_id(ide_id) {
        "codex" => codex::apply(project_root, servers),
        "claude-code" => claude_code::apply(project_root, servers),
        "vscode" => copilot::apply(project_root, servers),
        "cursor" => cursor::apply(project_root, servers),
        "windsurf" | "trae" | "qoder" | "kiro" => {
            json_mcp::apply(canonical_id(ide_id), project_root, servers)
        }
        other => Err(Error::Unsupported(format!(
            "MCP sync not supported for IDE `{other}` yet"
        ))),
    }
}

/// Remove the named servers from the IDE's native config (leaves unmanaged keys alone).
pub fn retract(ide_id: &str, project_root: Option<&Path>, names: &[String]) -> Result<PathBuf> {
    match canonical_id(ide_id) {
        "codex" => codex::retract(project_root, names),
        "claude-code" => claude_code::retract(project_root, names),
        "vscode" => copilot::retract(project_root, names),
        "cursor" => cursor::retract(project_root, names),
        "windsurf" | "trae" | "qoder" | "kiro" => {
            json_mcp::retract(canonical_id(ide_id), project_root, names)
        }
        other => Err(Error::Unsupported(format!(
            "MCP sync not supported for IDE `{other}` yet"
        ))),
    }
}

/// List of canonical IDE ids that currently support MCP sync.
pub const SUPPORTED: &[&str] = &[
    "claude-code",
    "codex",
    "cursor",
    "vscode",
    "windsurf",
    "trae",
    "qoder",
    "kiro",
];

/// Normalize legacy aliases to the canonical ids used by `crate::ide::IDES`.
pub fn canonical_id(ide_id: &str) -> &str {
    match ide_id {
        "copilot" => "vscode",
        other => other,
    }
}

/// Read MCP servers currently configured in a given IDE's native config file.
pub fn read(ide_id: &str, project_root: Option<&Path>) -> Result<Vec<McpServer>> {
    match canonical_id(ide_id) {
        "codex" => codex::read(project_root),
        "claude-code" => claude_code::read(project_root),
        "vscode" => copilot::read(project_root),
        "cursor" => cursor::read(project_root),
        "windsurf" | "trae" | "qoder" | "kiro" => {
            json_mcp::read(canonical_id(ide_id), project_root)
        }
        other => Err(Error::Unsupported(format!(
            "MCP read not supported for IDE `{other}` yet"
        ))),
    }
}
