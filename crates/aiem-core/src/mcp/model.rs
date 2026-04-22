use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Transport type for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransport {
    /// Classic stdio: spawn a local process and talk JSON-RPC over stdin/stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        cwd: Option<String>,
        /// Optional name of a local script directory that this server
        /// depends on.  When set, the directory lives at
        /// `~/.aiem/mcp/bundles/<bundle>/` and is synced with the git
        /// backup.  At deploy time the bundle is copied into the target
        /// project as `<project>/.aiem-mcp/<bundle>/`, and any occurrence
        /// of the `{BUNDLE}` placeholder in `command`, `args`, `env` or
        /// `cwd` is replaced with that absolute path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bundle: Option<String>,
    },
    /// Streamable HTTP (modern MCP transport).
    Http {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    /// Legacy SSE transport.
    Sse {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

/// A single MCP server managed by aiem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    /// Canonical name (also used as the key in every IDE config).
    pub name: String,
    #[serde(flatten)]
    pub transport: McpTransport,
    /// IDE ids this server should be synced to.
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// When `true`, `sync` skips this server (useful for temporarily disabling it
    /// without losing its definition).
    #[serde(default)]
    pub disabled: bool,
}

/// On-disk structure for `~/.aiem/mcp/servers.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct McpRegistryFile {
    #[serde(default)]
    pub servers: BTreeMap<String, McpServer>,
}
