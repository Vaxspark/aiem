//! Standard paths used by aiem.
//!
//! Layout:
//! ```text
//! ~/.aiem/
//! ├── skills/<owner>__<repo>[__<subdir>]/   unified local skills repo
//! ├── mcp/servers.json                      unified MCP server registry
//! ├── backups/<ide>/<ts>/                   IDE config backups before write
//! └── cache/                                http / registry cache
//! ```

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Root directory: `$AIEM_HOME` if set, otherwise `~/.aiem`.
pub fn home() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("AIEM_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().ok_or_else(|| Error::Invalid("cannot locate home dir".into()))?;
    Ok(home.join(".aiem"))
}

pub fn skills_dir() -> Result<PathBuf> { Ok(home()?.join("skills")) }
pub fn mcp_dir() -> Result<PathBuf> { Ok(home()?.join("mcp")) }
pub fn backups_dir() -> Result<PathBuf> { Ok(home()?.join("backups")) }
pub fn cache_dir() -> Result<PathBuf> { Ok(home()?.join("cache")) }

pub fn mcp_servers_file() -> Result<PathBuf> { Ok(mcp_dir()?.join("servers.json")) }
pub fn secrets_index_file() -> Result<PathBuf> { Ok(home()?.join("secrets.json")) }
pub fn profiles_file() -> Result<PathBuf> { Ok(home()?.join("profiles.json")) }
pub fn projects_file() -> Result<PathBuf> { Ok(home()?.join("projects.json")) }

/// Timestamped local snapshots: `~/.aiem/snapshots/`
pub fn snapshots_dir() -> Result<PathBuf> { Ok(home()?.join("snapshots")) }

/// Git working tree used for GitHub push/pull backups: `~/.aiem/backup-git/`
pub fn backup_git_dir() -> Result<PathBuf> { Ok(home()?.join("backup-git")) }

/// Backup preferences file: `~/.aiem/backup.json`
pub fn backup_config_file() -> Result<PathBuf> { Ok(home()?.join("backup.json")) }

/// Plain-text fallback slot for the GitHub backup token, used when the OS
/// keyring cannot persist the secret (e.g. systemd user services on Linux,
/// which run in their own non-interactive session keyring).  Written with
/// `0600` perms on Unix.  Primary storage remains the OS keyring.
pub fn backup_token_file() -> Result<PathBuf> { Ok(home()?.join(".backup-token")) }

/// Recycle bin: `~/.aiem/trash/`.  When a skill or MCP bundle is removed,
/// its on-disk content is moved here (under a timestamped subdirectory)
/// instead of being deleted outright, so the user can recover from an
/// accidental deletion.  Never synced to git backup.
pub fn trash_dir() -> Result<PathBuf> { Ok(home()?.join("trash")) }

/// Source storage for MCP "bundles" — local script folders that an MCP
/// server depends on.  Layout: `~/.aiem/mcp/bundles/<bundle_name>/`.
/// Synced to the git backup.  At deploy time, a bundle is copied into the
/// target project as `<project>/.aiem-mcp/<bundle_name>/`.
pub fn mcp_bundles_dir() -> Result<PathBuf> { Ok(mcp_dir()?.join("bundles")) }

/// Create all standard dirs if missing.
pub fn ensure_layout() -> Result<()> {
    for p in [home()?, skills_dir()?, mcp_dir()?, backups_dir()?, cache_dir()?, snapshots_dir()?, mcp_bundles_dir()?] {
        std::fs::create_dir_all(&p)?;
    }
    Ok(())
}

/// Expand `~` prefix in a path string to the user's home dir.
pub fn expand_user<P: AsRef<Path>>(p: P) -> PathBuf {
    let s = p.as_ref().to_string_lossy().to_string();
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}
