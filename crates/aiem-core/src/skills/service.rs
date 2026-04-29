//! Unified skill operations — single source of truth for CLI, Web, and GUI.
//!
//! Every mutation goes through this module so parse → fetch → registry save →
//! MCP auto-register → smart merge → hash happen in exactly one place.

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::github;
use super::install;
use super::model::{Skill, SkillSource};
use super::registry::SkillRegistry;
use crate::mcp::McpRegistry;
use crate::Result;

/// Outcome of any skill service operation.
#[derive(Debug, Default)]
pub struct ServiceResult {
    pub skills_added: Vec<String>,
    pub skills_updated: Vec<String>,
    pub mcp_registered: Vec<String>,
    pub removed_noise: Vec<String>,
    pub skipped_modified: Vec<String>,
    pub summary: String,
}

/// Download skill(s) from a GitHub source string and persist to the registries.
pub async fn add_from_github(source: &str, name: Option<&str>) -> Result<ServiceResult> {
    let normalized = super::model::apply_github_proxy_env(source);
    let parsed = SkillSource::parse_github(normalized)
        .ok_or_else(|| crate::Error::Invalid(format!("invalid GitHub source: {source}")))?;
    let SkillSource::GitHub {
        owner,
        repo,
        r#ref,
        subdir,
    } = parsed
    else {
        return Err(crate::Error::Invalid(
            "only GitHub sources supported".into(),
        ));
    };

    let result =
        github::fetch_github_auto(&owner, &repo, r#ref.as_deref(), subdir.as_deref(), name).await?;

    let mut reg = SkillRegistry::load()?;
    let mut out = ServiceResult::default();

    for skill in result.skills {
        out.skills_added.push(skill.id.clone());
        reg.upsert(skill);
    }
    reg.save()?;

    if !result.mcp_servers.is_empty() {
        let mut mcp_reg = McpRegistry::load()?;
        for s in result.mcp_servers {
            out.mcp_registered.push(s.name.clone());
            mcp_reg.upsert(s);
        }
        mcp_reg.save()?;
    }

    out.summary = format!(
        "added {} skill(s), {} MCP server(s) from {owner}/{repo}",
        out.skills_added.len(),
        out.mcp_registered.len()
    );
    Ok(out)
}

/// Re-fetch the latest version of an installed skill with smart merge.
pub async fn update_skill(id: &str) -> Result<ServiceResult> {
    let reg = SkillRegistry::load()?;
    let existing = reg
        .get(id)
        .cloned()
        .ok_or_else(|| crate::Error::NotFound(format!("skill `{id}` not found")))?;

    let SkillSource::GitHub {
        owner,
        repo,
        subdir,
        ..
    } = existing.source.clone()
    else {
        return Err(crate::Error::Invalid("skill not from GitHub".into()));
    };

    let (temp_dir, new_version, actual_subdir) =
        github::fetch_github_to_temp(&owner, &repo, None, subdir.as_deref()).await?;

    let mut out = ServiceResult::default();

    let target = existing.path.clone();
    let new_hashes = hash_dir(temp_dir.path());

    if new_version == existing.version {
        out.removed_noise = github::cleanup_noise(&target, &existing.file_hashes, &new_hashes);
        if !out.removed_noise.is_empty() {
            let mut skill = existing;
            skill.file_hashes = hash_dir(&target);
            let mut reg = SkillRegistry::load()?;
            reg.upsert(skill);
            reg.save()?;
            out.summary = format!(
                "{id} already up to date; cleaned {} old file(s)",
                out.removed_noise.len()
            );
        } else {
            out.summary = format!("{id} already up to date");
        }
        return Ok(out);
    }

    out.skipped_modified = smart_merge(temp_dir.path(), &target, &existing.file_hashes);

    out.removed_noise = github::cleanup_noise(&target, &existing.file_hashes, &new_hashes);

    let mut skill = existing;
    skill.version = new_version;
    skill.file_hashes = hash_dir(&target);
    if let SkillSource::GitHub {
        r#ref: ref mut stored_ref,
        subdir: ref mut stored_subdir,
        ..
    } = skill.source
    {
        *stored_ref = None;
        if let Some(new_sub) = actual_subdir {
            *stored_subdir = Some(new_sub);
        }
    }

    let mut reg = SkillRegistry::load()?;
    reg.upsert(skill);
    reg.save()?;

    out.skills_updated.push(id.to_string());
    let mut parts = vec![format!("updated {id}")];
    if !out.removed_noise.is_empty() {
        parts.push(format!("cleaned {} old file(s)", out.removed_noise.len()));
    }
    if !out.skipped_modified.is_empty() {
        parts.push(format!(
            "skipped {} modified: {}",
            out.skipped_modified.len(),
            out.skipped_modified.join(", ")
        ));
    }
    out.summary = parts.join("; ");
    Ok(out)
}

/// Sync an entire owner/repo group: update existing + install new skills.
pub async fn sync_github_group(owner: &str, repo: &str) -> Result<ServiceResult> {
    let reg = SkillRegistry::load()?;
    let skills: Vec<Skill> = reg
        .list()
        .filter(|s| {
            matches!(
                &s.source,
                SkillSource::GitHub { owner: o, repo: r, .. } if *o == owner && *r == repo
            )
        })
        .cloned()
        .collect();

    let result = github::sync_github_group(owner, repo, &skills).await?;
    let mut out = ServiceResult::default();
    out.skills_updated = result.updated.iter().map(|s| s.id.clone()).collect();
    out.skills_added = result.added.iter().map(|s| s.id.clone()).collect();

    out.summary = if out.skills_added.is_empty() {
        format!(
            "synced {owner}/{repo}: {} updated, no new skills",
            out.skills_updated.len()
        )
    } else {
        let names: Vec<_> = result.added.iter().map(|s| s.name.as_str()).collect();
        format!(
            "synced {owner}/{repo}: {} updated, {} new: {}",
            out.skills_updated.len(),
            out.skills_added.len(),
            names.join(", ")
        )
    };
    Ok(out)
}

/// Create a brand-new local skill.
pub fn create_local(name: &str, content: &str) -> Result<ServiceResult> {
    let skill = super::registry::create_local_skill(name, content)?;
    Ok(ServiceResult {
        skills_added: vec![skill.id],
        summary: format!("created skill: {name}"),
        ..Default::default()
    })
}

/// Remove a skill (undeploys from all IDEs, moves to trash).
pub fn remove_skill(id: &str) -> Result<ServiceResult> {
    let mut reg = SkillRegistry::load()?;
    install::remove_skill(&mut reg, id)?;
    reg.save()?;
    Ok(ServiceResult {
        summary: format!("removed {id}"),
        ..Default::default()
    })
}

/// Deploy a skill to an IDE.
pub fn deploy_skill(id: &str, ide: &str, project: Option<&Path>) -> Result<ServiceResult> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg
        .get(id)
        .cloned()
        .ok_or_else(|| crate::Error::NotFound(format!("skill `{id}` not found")))?;
    let (link, _kind) = install::deploy(&mut skill, ide, project)?;
    reg.upsert(skill);
    reg.save()?;
    Ok(ServiceResult {
        summary: format!("deployed {} -> {}", id, link.display()),
        ..Default::default()
    })
}

/// Undeploy a skill from an IDE.
pub fn undeploy_skill(id: &str, ide: &str, project: Option<&Path>) -> Result<ServiceResult> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg
        .get(id)
        .cloned()
        .ok_or_else(|| crate::Error::NotFound(format!("skill `{id}` not found")))?;
    install::undeploy(&mut skill, ide, project)?;
    reg.upsert(skill);
    reg.save()?;
    Ok(ServiceResult {
        summary: format!("undeployed {id} from {ide}"),
        ..Default::default()
    })
}

// ─── Shared helpers (formerly duplicated in web/fs_merge.rs & gui/tasks.rs) ──

/// Smart merge using 3-way comparison:
/// - current == original hash → overwrite with new version
/// - current != original hash → user modified → skip
/// Returns list of skipped file relative paths (forward slashes).
pub fn smart_merge(
    src: &Path,
    dst: &Path,
    original_hashes: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut skipped = Vec::new();
    let walker = walkdir::WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in walker {
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        if target.exists() {
            let current = std::fs::read(&target).unwrap_or_default();
            let current_hash = hex::encode(Sha256::digest(&current));
            if let Some(orig_hash) = original_hashes.get(&rel_str) {
                if &current_hash != orig_hash {
                    skipped.push(rel_str);
                    continue;
                }
            }
        }
        let _ = std::fs::copy(entry.path(), &target);
    }
    skipped
}

/// Compute SHA-256 hashes of all files under `dir`, keyed by relative path (forward slashes).
pub fn hash_dir(dir: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let walker = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in walker {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = match entry.path().strip_prefix(dir) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if let Ok(bytes) = std::fs::read(entry.path()) {
            map.insert(rel, hex::encode(Sha256::digest(&bytes)));
        }
    }
    map
}
