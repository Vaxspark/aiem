//! Discover existing skills and MCP servers on this machine that are NOT yet
//! managed by aiem -- then optionally import them into unified management.
//!
//! Skills discovery: scan IDE skills directories under `$HOME`, `~/.agents/skills`,
//! and optional extra paths for sub-directories not in the registry.
//!
//! MCP discovery: read each supported IDE's native config and collect servers
//! not already present in aiem's MCP registry.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::ide;
use crate::mcp::adapters;
use crate::mcp::model::McpServer;
use crate::mcp::registry::McpRegistry;
use crate::skills::model::{Skill, SkillSource};
use crate::skills::registry::SkillRegistry;
use crate::{paths, Result};

// ─── Skills ─────────────────────────────────────────────────────────────────

/// A skill folder found on disk that isn't currently tracked by aiem.
#[derive(Debug, Clone)]
pub struct FoundSkill {
    /// Full path on disk.
    pub path: PathBuf,
    /// Source label (IDE id or directory description).
    pub ide_id: String,
    /// The directory name (will become the skill id if imported).
    pub dir_name: String,
    /// Whether this looks like a symlink/junction pointing somewhere else.
    pub is_link: bool,
}

/// Normalize a path for comparison: canonicalize if possible, otherwise
/// convert all forward slashes to backslashes on Windows.
fn normalize_path(p: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(p) {
        return c;
    }
    #[cfg(windows)]
    {
        PathBuf::from(p.to_string_lossy().replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        p.to_path_buf()
    }
}

/// Check if `candidate` matches any path in `known` after normalization.
fn path_matches_any(candidate: &Path, known: &[PathBuf]) -> bool {
    let nc = normalize_path(candidate);
    known.iter().any(|k| normalize_path(k) == nc)
}

/// Scan the machine for unmanaged skill folders.
///
/// Searches:
/// 1. ALL IDE skills directories under `$HOME` (not just user-scope)
/// 2. `~/.agents/skills/` (common shared skills location)
/// 3. Any extra directories passed in `extra_dirs`
///
/// Each sub-directory found that isn't already in the registry is returned.
pub fn discover_skills() -> Result<Vec<FoundSkill>> {
    discover_skills_with_extras(&[])
}

pub fn discover_skills_with_extras(extra_dirs: &[PathBuf]) -> Result<Vec<FoundSkill>> {
    let reg = SkillRegistry::load().unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();

    // Collect managed paths and IDs for deduplication.
    let managed_paths: Vec<PathBuf> = reg.list().map(|s| s.path.clone()).collect();
    let managed_ids: Vec<String> = reg.list().map(|s| s.id.clone()).collect();

    // Collect deployment targets already tracked.
    let mut deployed_links: Vec<PathBuf> = Vec::new();
    for skill in reg.list() {
        for (ide_id, _roots) in &skill.deployments {
            if let Some(ide) = ide::find(ide_id) {
                deployed_links.push(home.join(ide.skills_dir).join(&skill.id));
            }
        }
    }

    let mut found = Vec::new();
    let mut seen_paths: Vec<PathBuf> = Vec::new(); // deduplicate across sources

    // --- 1. Scan ALL IDE skills dirs under $HOME (both user-scope and project-scope) ---
    for ide in ide::IDES {
        let dir = home.join(ide.skills_dir);
        scan_dir_for_skills(
            &dir,
            ide.id,
            &managed_paths,
            &managed_ids,
            &deployed_links,
            &mut seen_paths,
            &mut found,
        );
    }

    // --- 2. Scan ~/.agents/skills/ (common shared skills directory) ---
    let agents_dir = home.join(".agents").join("skills");
    scan_dir_for_skills(
        &agents_dir,
        "agents",
        &managed_paths,
        &managed_ids,
        &deployed_links,
        &mut seen_paths,
        &mut found,
    );

    // --- 3. Scan extra directories ---
    // For each extra path, first check if it IS a skills dir itself (direct scan),
    // then also look for <subdir>/.<ide>/skills/ patterns within it (project scan).
    for dir in extra_dirs {
        // If the path itself looks like a skills directory (ends with "skills"),
        // scan it directly as a flat skills folder.
        if dir.file_name().map(|n| n.to_string_lossy().contains("skills")).unwrap_or(false) {
            scan_dir_for_skills(
                dir,
                "custom",
                &managed_paths,
                &managed_ids,
                &deployed_links,
                &mut seen_paths,
                &mut found,
            );
        }
        // Treat the extra dir itself as a project root: scan <dir>/.claude/skills/ etc.
        for ide in ide::IDES {
            let skills_dir = dir.join(ide.skills_dir);
            scan_dir_for_skills(
                &skills_dir,
                ide.id,
                &managed_paths,
                &managed_ids,
                &deployed_links,
                &mut seen_paths,
                &mut found,
            );
        }
        // Sub-project scan: look for .<ide>/skills/ inside each sub-directory (1 level deep)
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let project_dir = entry.path();
                if !project_dir.is_dir() { continue; }
                for ide in ide::IDES {
                    let skills_dir = project_dir.join(ide.skills_dir);
                    scan_dir_for_skills(
                        &skills_dir,
                        ide.id,
                        &managed_paths,
                        &managed_ids,
                        &deployed_links,
                        &mut seen_paths,
                        &mut found,
                    );
                }
            }
        }
    }

    Ok(found)
}

/// Scan a single directory for skill sub-folders, appending results to `found`.
fn scan_dir_for_skills(
    dir: &Path,
    source_label: &str,
    managed_paths: &[PathBuf],
    managed_ids: &[String],
    deployed_links: &[PathBuf],
    seen_paths: &mut Vec<PathBuf>,
    found: &mut Vec<FoundSkill>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return, // directory doesn't exist or unreadable
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip non-directories; also skip broken junctions / symlinks
        let is_dir = match std::fs::metadata(&path) {
            Ok(m) => m.is_dir(),
            Err(_) => continue,
        };
        if !is_dir { continue; }
        let dir_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

        // Skip hidden system dirs like .git
        if dir_name == ".git" { continue; }

        // A skill directory should contain at least one .md file (the instruction).
        // Skip directories that don't look like skills (e.g. .vscode, outputs).
        if !looks_like_skill(&path) { continue; }

        // Skip if already seen (from another IDE with overlapping dirs)
        if seen_paths.iter().any(|s| normalize_path(s) == normalize_path(&path)) {
            continue;
        }

        // Skip if this is an already deployed link
        if path_matches_any(&path, deployed_links) { continue; }

        // Skip if managed by aiem (match by ID or path, using normalized comparison)
        let candidate_id = format!("local__{}", sanitize_id(&dir_name));
        if managed_ids.iter().any(|id| id == &candidate_id || id == &dir_name) {
            continue;
        }
        if path_matches_any(&path, managed_paths) { continue; }

        let is_link = crate::fs_util::is_link(&path);
        seen_paths.push(path.clone());
        found.push(FoundSkill {
            path,
            ide_id: source_label.to_string(),
            dir_name,
            is_link,
        });
    }
}

/// Check if a directory looks like a skill (contains at least one .md file).
fn looks_like_skill(path: &Path) -> bool {
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        if let Some(ext) = entry.path().extension() {
            if ext.eq_ignore_ascii_case("md") {
                return true;
            }
        }
    }
    false
}

/// Import a discovered skill folder into the aiem registry.
///
/// When `copy_to_aiem == true` aiem takes **full ownership** of the content:
///
/// 1. The folder is copied into `~/.aiem/skills/<id>/`.
/// 2. The original location is moved to the recycle bin at
///    `~/.aiem/trash/` so it can no longer pollute the IDE's skills
///    directory (but is still recoverable).
/// 3. If the source was found inside a real IDE's skills directory we
///    redeploy it by creating a symlink/junction back at the original
///    path, so the IDE keeps seeing the skill transparently.
///
/// When `copy_to_aiem == false` aiem simply registers a reference to the
/// existing on-disk folder and leaves the filesystem untouched.
pub fn import_skill(found: &FoundSkill, copy_to_aiem: bool) -> Result<Skill> {
    let mut reg = SkillRegistry::load().unwrap_or_default();
    let id = format!("local__{}", sanitize_id(&found.dir_name));

    // Track whether we successfully performed the copy + trash step so we
    // know whether a post-copy re-link back to the original location is
    // safe to attempt.
    let mut did_migrate = false;

    let dest_path = if copy_to_aiem {
        let dest = paths::skills_dir()?.join(&id);
        if !dest.exists() {
            // Resolve the real path first (follow junctions/symlinks).
            let real_src = std::fs::canonicalize(&found.path).unwrap_or_else(|_| found.path.clone());
            match crate::fs_util::copy_dir_safe(&real_src, &dest) {
                Ok(()) => {
                    // Copy succeeded — move the original out of the IDE's
                    // skills tree into the recycle bin so the IDE no longer
                    // sees two copies of the same content.  Best-effort:
                    // if trashing fails (e.g. permission denied) we keep
                    // going with just the aiem-side copy.
                    let label = format!("skill-{}", sanitize_id(&found.dir_name));
                    let _ = crate::fs_util::move_to_trash(&found.path, &label);
                    did_migrate = true;
                    dest
                }
                Err(_) => {
                    // If copy fails, reference in-place instead.
                    found.path.clone()
                }
            }
        } else {
            dest
        }
    } else {
        found.path.clone()
    };

    let now = chrono::Utc::now().to_rfc3339();
    let mut deployments = BTreeMap::new();
    deployments.insert(found.ide_id.clone(), vec!["~".to_string()]);

    let mut skill = Skill {
        id: id.clone(),
        name: found.dir_name.clone(),
        source: SkillSource::Local { path: dest_path.clone() },
        version: "imported".to_string(),
        path: dest_path,
        description: Some(format!("Imported from {}", found.ide_id)),
        installed_at: Some(now),
        deployments,
        file_hashes: Default::default(),
    };

    // If we successfully extracted the skill out of a real IDE's skills
    // directory, recreate a symlink/junction at the original path so the
    // IDE continues to see the skill.  Failures here are non-fatal — the
    // user can always trigger a manual deploy later.
    if did_migrate {
        if crate::ide::find(&found.ide_id).is_some() {
            let _ = crate::skills::install::deploy(&mut skill, &found.ide_id, None);
        }
    }

    reg.upsert(skill.clone());
    reg.save()?;
    Ok(skill)
}

// ─── MCP ────────────────────────────────────────────────────────────────────

/// An MCP server found in an IDE config that isn't yet in aiem's registry.
#[derive(Debug, Clone)]
pub struct FoundMcpServer {
    pub server: McpServer,
    /// Which IDE config it was found in.
    pub source_ide: String,
}

/// Scan all supported IDE configs for MCP servers not in aiem's registry.
pub fn discover_mcp() -> Result<Vec<FoundMcpServer>> {
    let reg = McpRegistry::load().unwrap_or_default();
    let managed: Vec<String> = reg.list().map(|s| s.name.clone()).collect();
    let mut found = Vec::new();
    let mut seen_names: Vec<String> = Vec::new();

    for &ide_id in adapters::SUPPORTED {
        let servers = match adapters::read(ide_id, None) {
            Ok(s) => s,
            Err(_) => continue, // config missing or unreadable
        };
        for s in servers {
            if managed.contains(&s.name) { continue; }
            if seen_names.contains(&s.name) {
                // Already found in another IDE — merge target
                if let Some(existing) = found.iter_mut().find(|f: &&mut FoundMcpServer| f.server.name == s.name) {
                    if !existing.server.targets.contains(&ide_id.to_string()) {
                        existing.server.targets.push(ide_id.to_string());
                    }
                }
                continue;
            }
            seen_names.push(s.name.clone());
            found.push(FoundMcpServer {
                server: McpServer {
                    targets: vec![ide_id.to_string()],
                    ..s
                },
                source_ide: ide_id.to_string(),
            });
        }
    }
    Ok(found)
}

/// Import a discovered MCP server into aiem's unified registry.
pub fn import_mcp(found: &FoundMcpServer) -> Result<()> {
    let mut reg = McpRegistry::load()?;
    // If it already exists (race), just merge targets.
    if let Some(existing) = reg.get_mut(&found.server.name) {
        for t in &found.server.targets {
            if !existing.targets.contains(t) {
                existing.targets.push(t.clone());
            }
        }
    } else {
        reg.upsert(found.server.clone());
    }
    reg.save()?;
    Ok(())
}

/// Import ALL discovered MCP servers at once. Returns the count imported.
pub fn import_all_mcp(found: &[FoundMcpServer]) -> Result<usize> {
    if found.is_empty() { return Ok(0); }
    let mut reg = McpRegistry::load()?;
    let mut count = 0;
    for f in found {
        if let Some(existing) = reg.get_mut(&f.server.name) {
            for t in &f.server.targets {
                if !existing.targets.contains(t) { existing.targets.push(t.clone()); }
            }
        } else {
            reg.upsert(f.server.clone());
            count += 1;
        }
    }
    reg.save()?;
    Ok(count)
}

/// Import ALL discovered skills at once. Returns the count imported.
pub fn import_all_skills(found: &[FoundSkill], copy_to_aiem: bool) -> Result<usize> {
    let mut count = 0;
    for f in found {
        import_skill(f, copy_to_aiem)?;
        count += 1;
    }
    Ok(count)
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}
