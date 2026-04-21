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

/// Create all standard dirs if missing.
pub fn ensure_layout() -> Result<()> {
    for p in [home()?, skills_dir()?, mcp_dir()?, backups_dir()?, cache_dir()?] {
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
