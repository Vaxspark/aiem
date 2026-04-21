//! Filesystem helpers: atomic write, symlink/junction, backup, recursive copy.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::{paths, Error, Result};

/// Atomically write `contents` to `path` (write to tmp sibling then rename).
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = tempfile::NamedTempFile::new_in(
        path.parent().unwrap_or_else(|| Path::new(".")),
    )?;
    {
        let f = tmp.as_file_mut();
        f.write_all(contents)?;
        f.sync_all()?;
    }
    tmp.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}

/// Copy `path` into `$AIEM_HOME/backups/<tag>/<timestamp>-<filename>` if it exists.
pub fn backup_file(path: &Path, tag: &str) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let ts = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = paths::backups_dir()?.join(tag).join(&ts);
    fs::create_dir_all(&dir)?;
    let name = path.file_name().unwrap_or_default();
    let dst = dir.join(name);
    fs::copy(path, &dst)?;
    Ok(Some(dst))
}

/// Create a symlink from `link` -> `target` (a directory).
/// On Windows: try dir symlink, fall back to junction, fall back to recursive copy.
pub fn link_dir(target: &Path, link: &Path) -> Result<LinkKind> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    if link.exists() || link.symlink_metadata().is_ok() {
        remove_path(link)?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)?;
        return Ok(LinkKind::Symlink);
    }

    #[cfg(windows)]
    {
        // Try dir symlink (needs Developer Mode or admin).
        if std::os::windows::fs::symlink_dir(target, link).is_ok() {
            return Ok(LinkKind::Symlink);
        }
        // Fall back to a junction via `cmd /c mklink /J`.
        // Must pass entire mklink command as a single string to avoid
        // cmd.exe argument quoting issues ("无效开关" / invalid switch).
        let cmd_str = format!(
            "mklink /J \"{}\" \"{}\"",
            link.display(),
            target.display()
        );
        let status = std::process::Command::new("cmd")
            .args(["/C", &cmd_str])
            .status();
        if matches!(status, Ok(s) if s.success()) {
            return Ok(LinkKind::Junction);
        }
        // Final fallback: recursive copy.
        copy_dir_recursive(target, link)?;
        Ok(LinkKind::Copy)
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(Error::Unsupported("link_dir: unsupported OS".into()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Symlink,
    Junction,
    Copy,
}

/// Remove a file, symlink, junction or directory, whichever `path` is.
pub fn remove_path(path: &Path) -> Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() {
        // On Windows, symlink_dir must be removed with remove_dir; symlink_file with remove_file.
        #[cfg(windows)]
        {
            if meta.is_dir() {
                fs::remove_dir(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }
        #[cfg(unix)]
        {
            fs::remove_file(path)?;
        }
    } else if meta.is_dir() {
        // Junctions report as dir on Windows; remove_dir works for empty junctions; otherwise
        // we need remove_dir_all for real directories.
        if is_reparse_point(&meta) {
            // Junction/reparse point: remove_dir unlinks it without touching target.
            fs::remove_dir(path).or_else(|_| fs::remove_dir_all(path))?;
        } else {
            fs::remove_dir_all(path)?;
        }
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(unix)]
fn is_reparse_point(_meta: &fs::Metadata) -> bool { false }

/// Is this path a symlink or (on Windows) a junction/reparse point?
pub fn is_link(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(m) => m.file_type().is_symlink() || is_reparse_point(&m),
        Err(_) => false,
    }
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
        let rel = entry.path().strip_prefix(src).unwrap();
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(p) = target.parent() { fs::create_dir_all(p)?; }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Safe version of copy_dir_recursive: skips symlinks/junctions inside the tree,
/// limits depth to 10 levels, and continues on individual file errors.
pub fn copy_dir_safe(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    let walker = walkdir::WalkDir::new(src)
        .max_depth(10)
        .follow_links(false); // Do NOT follow symlinks/junctions
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // skip unreadable entries
        };
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            let _ = fs::create_dir_all(&target);
        } else if entry.file_type().is_file() {
            if let Some(p) = target.parent() { let _ = fs::create_dir_all(p); }
            let _ = fs::copy(entry.path(), &target); // best-effort
        }
        // symlinks/junctions inside: skip silently
    }
    Ok(())
}
