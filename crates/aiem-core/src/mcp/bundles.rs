//! MCP bundles: local script directories that back custom stdio MCP servers.
//!
//! A bundle is stored under `~/.aiem/mcp/bundles/<name>/`.  Bundles are
//! synced via the git backup (so a user's custom Python / Node MCP script
//! travels with their aiem data).  At deploy time a bundle is copied into
//! the target project as `<project>/.aiem-mcp/<name>/`, and the server's
//! `command`/`args`/`cwd`/`env` may reference the resulting absolute path
//! via the `{BUNDLE}` placeholder.
//!
//! User-provided bundles may depend on language runtimes (python3, node,
//! uv, …).  aiem does not try to install those – it's the user's
//! responsibility to provision the host interpreter.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::fs_util::{copy_dir_safe, move_to_trash};
use crate::paths;

/// Return the on-disk path for a named bundle, regardless of whether it
/// currently exists.
pub fn bundle_path(name: &str) -> Result<PathBuf> {
    let name = sanitize_name(name)?;
    Ok(paths::mcp_bundles_dir()?.join(name))
}

/// Import a bundle from a source directory, copying it into
/// `~/.aiem/mcp/bundles/<name>/`.  If a bundle of that name already
/// exists it is first moved to the trash so the operation is
/// non-destructive.
pub fn import_bundle(name: &str, src: &Path) -> Result<PathBuf> {
    if !src.is_dir() {
        return Err(Error::Invalid(format!(
            "bundle source is not a directory: {}",
            src.display()
        )));
    }
    let dest = bundle_path(name)?;
    if dest.exists() {
        let _ = move_to_trash(&dest, &format!("mcp-bundle-{}", sanitize_name(name)?));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    copy_dir_safe(src, &dest)?;
    Ok(dest)
}

/// Move a bundle to the trash.  Returns Ok(None) if the bundle did not
/// exist to begin with.
pub fn remove_bundle(name: &str) -> Result<Option<PathBuf>> {
    let path = bundle_path(name)?;
    if !path.exists() {
        return Ok(None);
    }
    move_to_trash(&path, &format!("mcp-bundle-{}", sanitize_name(name)?))
}

/// List the names of all currently imported bundles.
pub fn list_bundles() -> Result<Vec<String>> {
    let root = paths::mcp_bundles_dir()?;
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(n) = entry.file_name().to_str() {
                out.push(n.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Expand `{BUNDLE}` placeholders in a string to the absolute path of the
/// bundle's deployed location.
pub fn expand_placeholder(s: &str, bundle_dir: &Path) -> String {
    s.replace("{BUNDLE}", &bundle_dir.to_string_lossy())
}

fn sanitize_name(name: &str) -> Result<&str> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
    {
        return Err(Error::Invalid(format!("invalid bundle name: {name:?}")));
    }
    Ok(name)
}
