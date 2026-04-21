//! Deploy / undeploy a locally-managed skill into an IDE's skills directory
//! by way of a symlink (or junction / copy fallback).

use std::path::{Path, PathBuf};

use crate::fs_util::{is_link, link_dir, remove_path, LinkKind};
use crate::ide::{self, IdeTarget, Scope};
use crate::{paths, Error, Result};

use super::model::Skill;
use super::registry::SkillRegistry;

/// Resolve the directory into which a skill should be linked for a given IDE.
///
/// - For [`Scope::User`] the skill dir is placed under `$HOME/<ide.skills_dir>`.
/// - For [`Scope::Project`] the caller must pass a `project_root`.
pub fn target_dir(ide: &IdeTarget, project_root: Option<&Path>) -> Result<PathBuf> {
    let root = match (ide.default_scope.clone(), project_root) {
        (Scope::User, None) => dirs::home_dir()
            .ok_or_else(|| Error::Invalid("cannot locate home dir".into()))?,
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
pub fn deploy(
    skill: &mut Skill,
    ide_id: &str,
    project_root: Option<&Path>,
) -> Result<(PathBuf, LinkKind)> {
    let ide = ide::find(ide_id)
        .ok_or_else(|| Error::NotFound(format!("unknown IDE `{ide_id}`")))?;
    let dir = target_dir(ide, project_root)?;
    std::fs::create_dir_all(&dir)?;
    // Use skill name as the deployed directory name (not the full canonical id)
    let deploy_name = &skill.name;
    let link = dir.join(deploy_name);
    let kind = link_dir(&skill.path, &link)?;

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

/// Remove a skill deployment from an IDE.
pub fn undeploy(
    skill: &mut Skill,
    ide_id: &str,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    let ide = ide::find(ide_id)
        .ok_or_else(|| Error::NotFound(format!("unknown IDE `{ide_id}`")))?;
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

/// Remove a skill from the registry and delete its on-disk content plus any
/// tracked deployments.
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
            let root_opt: Option<PathBuf> = if root == "~" { None } else { Some(PathBuf::from(root)) };
            let _ = undeploy(&mut skill, &ide_id, root_opt.as_deref());
        }
    }
    if skill.path.starts_with(paths::skills_dir()?) && skill.path.exists() {
        remove_path(&skill.path)?;
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
