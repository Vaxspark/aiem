//! Deploy / undeploy a locally-managed skill into an IDE's skills directory
//! by way of a symlink (or junction / copy fallback).

use std::path::{Path, PathBuf};

use crate::fs_util::{copy_dir_recursive, is_link, link_dir, remove_path, LinkKind};
use crate::ide::{self, IdeTarget, Scope};
use crate::{paths, Error, Result};

use super::model::{Skill, SkillSource};
use super::registry::SkillRegistry;

/// Resolve the directory into which a skill should be linked for a given IDE.
///
/// - For [`Scope::User`] the skill dir is placed under `$HOME/<ide.skills_dir>`.
/// - For [`Scope::Project`] the caller must pass a `project_root`.
pub fn target_dir(ide: &IdeTarget, project_root: Option<&Path>) -> Result<PathBuf> {
    let root = match (ide.default_scope.clone(), project_root) {
        (Scope::User, None) => {
            dirs::home_dir().ok_or_else(|| Error::Invalid("cannot locate home dir".into()))?
        }
        (Scope::User, Some(p)) | (Scope::Project, Some(p)) => p.to_path_buf(),
        (Scope::Project, None) => {
            return Err(Error::Invalid(format!(
                "IDE `{}` requires a project directory; pass --project <path>",
                ide.id
            )))
        }
    };
    Ok(root.join(ide.skills_dir))
}

/// Install a skill into the given IDE (by IDE id). Returns the created link path.
///
/// If a directory already exists at the target path and points to a different
/// skill, this function logs a warning (via `tracing`) before overwriting.
pub fn deploy(
    skill: &mut Skill,
    ide_id: &str,
    project_root: Option<&Path>,
) -> Result<(PathBuf, LinkKind)> {
    let ide =
        ide::find(ide_id).ok_or_else(|| Error::NotFound(format!("unknown IDE `{ide_id}`")))?;
    let dir = target_dir(ide, project_root)?;
    std::fs::create_dir_all(&dir)?;
    ensure_skill_ready_for_deploy(skill)?;
    let deploy_name = &skill.name;
    let link = dir.join(deploy_name);

    if link.exists() || is_link(&link) {
        let existing_target = std::fs::read_link(&link).ok();
        let is_same = existing_target.as_ref().map_or(false, |t| *t == skill.path);
        if !is_same {
            tracing::warn!(
                skill = %skill.id,
                ide = ide_id,
                path = %link.display(),
                existing_target = ?existing_target,
                "overwriting existing directory/link at deploy target"
            );
        }
    }

    let kind = if project_root.is_some() {
        copy_skill_dir_for_project(&skill.path, &link)?;
        LinkKind::Copy
    } else {
        link_dir(&skill.path, &link)?
    };

    let key = ide_id.to_string();
    let root_tag = project_root
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "~".to_string());
    let entry = skill.deployments.entry(key).or_default();
    if !entry.contains(&root_tag) {
        entry.push(root_tag);
    }
    Ok((link, kind))
}

/// Make sure `skill.path` points at real, Linux-local skill content before
/// creating a deployment.  Restored indexes may contain absolute paths from a
/// different machine (for example `C:\Users\...\ .aiem\skills\...`); deploying
/// those verbatim creates a broken symlink that looks like a bare technical
/// name in the project.  We first re-anchor to `~/.aiem/skills/<id>`, then
/// rehydrate GitHub skills on demand when the content directory is missing.
pub fn ensure_skill_ready_for_deploy(skill: &mut Skill) -> Result<()> {
    reanchor_to_local_skill_dir(skill)?;
    if super::github::ensure_canonical_skill_manifest(&skill.path).is_ok() {
        return Ok(());
    }

    let SkillSource::GitHub {
        owner,
        repo,
        r#ref,
        subdir,
    } = skill.source.clone()
    else {
        return super::github::ensure_canonical_skill_manifest(&skill.path);
    };

    let mut refreshed = fetch_github_for_deploy(
        &owner,
        &repo,
        r#ref.as_deref(),
        subdir.as_deref(),
        &skill.name,
    )?;
    let expected_id = skill.id.clone();
    if refreshed.id != expected_id {
        let target = paths::skills_dir()?.join(&expected_id);
        if target.exists() || crate::fs_util::is_link(&target) {
            remove_path(&target)?;
        }
        copy_dir_recursive(&refreshed.path, &target)?;
        if refreshed.path.starts_with(paths::skills_dir()?) {
            let _ = remove_path(&refreshed.path);
        }
        refreshed.path = target;
        refreshed.id = expected_id;
    }

    let deployments = skill.deployments.clone();
    *skill = Skill {
        deployments,
        ..refreshed
    };
    reanchor_to_local_skill_dir(skill)?;
    super::github::ensure_canonical_skill_manifest(&skill.path)
}

fn reanchor_to_local_skill_dir(skill: &mut Skill) -> Result<()> {
    let local = paths::skills_dir()?.join(&skill.id);
    if local.is_dir() && super::github::ensure_canonical_skill_manifest(&local).is_ok() {
        skill.path = local.clone();
        if let SkillSource::Local { path } = &mut skill.source {
            *path = local;
        }
        return Ok(());
    }

    if is_foreign_windows_path(&skill.path)
        && skill.path.is_dir()
        && super::github::ensure_canonical_skill_manifest(&skill.path).is_ok()
    {
        if local.exists() || crate::fs_util::is_link(&local) {
            remove_path(&local)?;
        }
        copy_dir_recursive(&skill.path, &local)?;
        skill.path = local.clone();
        if let SkillSource::Local { path } = &mut skill.source {
            *path = local;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn is_foreign_windows_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(":\\") || s.contains(":/")
}

#[cfg(not(unix))]
fn is_foreign_windows_path(_path: &Path) -> bool {
    false
}

fn copy_skill_dir_for_project(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() || is_link(dst) {
        remove_path(dst)?;
    }
    copy_dir_recursive(src, dst)?;
    super::github::ensure_canonical_skill_manifest(dst)?;
    Ok(())
}

fn fetch_github_for_deploy(
    owner: &str,
    repo: &str,
    r#ref: Option<&str>,
    subdir: Option<&str>,
    name: &str,
) -> Result<Skill> {
    let fut = super::github::fetch_github(owner, repo, r#ref, subdir, Some(name));
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(fut),
    }
}

/// Remove a skill deployment from an IDE.
pub fn undeploy(skill: &mut Skill, ide_id: &str, project_root: Option<&Path>) -> Result<PathBuf> {
    let ide =
        ide::find(ide_id).ok_or_else(|| Error::NotFound(format!("unknown IDE `{ide_id}`")))?;
    let dir = target_dir(ide, project_root)?;
    let link = dir.join(&skill.name);

    // Also try old-style id-based path for backward compat
    let link_old = dir.join(&skill.id);

    // Only delete the symlink, never follow it. If it is a real dir (copy fallback),
    // the user opted in so we still remove it.
    if link.exists() || is_link(&link) {
        remove_path(&link)?;
    }
    // Also clean up old-style id-based link if it exists
    if link_old.exists() || is_link(&link_old) {
        remove_path(&link_old)?;
    }

    let root_tag = project_root
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "~".to_string());
    if let Some(list) = skill.deployments.get_mut(ide_id) {
        list.retain(|x| x != &root_tag);
        if list.is_empty() {
            skill.deployments.remove(ide_id);
        }
    }
    Ok(link)
}

/// Remove a skill from the registry and move its on-disk content (plus any
/// tracked deployments) out of the active config tree.
///
/// Deployments (symlinks / junctions in IDE skills dirs) are unlinked
/// normally, but the skill's **content directory** under
/// `~/.aiem/skills/<id>/` is moved to the recycle bin at
/// `~/.aiem/trash/` rather than being deleted outright, so the user can
/// recover from an accidental removal.
pub fn remove_skill(reg: &mut SkillRegistry, id: &str) -> Result<()> {
    let Some(mut skill) = reg.remove(id) else {
        return Err(Error::NotFound(format!("skill `{id}` not found")));
    };
    // Best-effort undeploy all.
    let deployments: Vec<(String, Vec<String>)> = skill
        .deployments
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (ide_id, roots) in deployments {
        for root in roots {
            let root_opt: Option<PathBuf> = if root == "~" {
                None
            } else {
                Some(PathBuf::from(root))
            };
            let _ = undeploy(&mut skill, &ide_id, root_opt.as_deref());
        }
    }
    if skill.path.starts_with(paths::skills_dir()?) && skill.path.exists() {
        // Recycle instead of hard-delete.  Fall back to outright removal
        // only if the trash move fails, so we never leave an orphan
        // directory behind.
        let label = format!("skill-{}", crate::fs_util::sanitize_for_path(&skill.id));
        if crate::fs_util::move_to_trash(&skill.path, &label).is_err() {
            crate::fs_util::remove_path(&skill.path)?;
        }
    }
    Ok(())
}

/// Undeploy all skills from global (user) scope across all IDEs.
/// Returns the number of deployments removed.
pub fn undeploy_all_global(reg: &mut SkillRegistry) -> Result<usize> {
    let ids: Vec<String> = reg.list().map(|s| s.id.clone()).collect();
    let mut count = 0;
    for id in ids {
        let skill = match reg.get_mut(&id) {
            Some(s) => s,
            None => continue,
        };
        let global_deployments: Vec<String> = skill
            .deployments
            .iter()
            .filter(|(_, roots)| roots.contains(&"~".to_string()))
            .map(|(ide, _)| ide.clone())
            .collect();
        for ide_id in global_deployments {
            let _ = undeploy(skill, &ide_id, None);
            count += 1;
        }
    }
    Ok(count)
}
