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

/// Detected runtime for a bundle-backed MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpRuntime {
    Python,
    Node,
    Other,
}

/// How the server authenticates with external services.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpAuthMode {
    /// No auth needed or fully configured via env/headers.
    None,
    /// Tokens stored in aiem Vault, expanded at deploy time.
    SecretRef,
    /// Server requires external login (browser OAuth, etc.).
    External,
    /// Secrets are referenced but missing from the Vault.
    MissingSecret,
}

impl Default for McpAuthMode {
    fn default() -> Self {
        Self::None
    }
}

/// Where this server definition came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSource {
    pub owner: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
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

    /// GitHub origin (set when imported via `import-github`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<McpSource>,
    /// Detected runtime of the bundle entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<McpRuntime>,
    /// Authentication status.
    #[serde(default)]
    pub auth_mode: McpAuthMode,
}

/// On-disk structure for `~/.aiem/mcp/servers.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct McpRegistryFile {
    #[serde(default)]
    pub servers: BTreeMap<String, McpServer>,
}
