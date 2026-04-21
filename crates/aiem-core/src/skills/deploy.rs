//! Single source of truth for "deploy a skill to a registered project".
//!
//! Wraps `install::deploy` / `install::undeploy` and keeps each
//! `Project.skills` list in sync, so the per-card "In projects" chips row
//! (GUI + Web) has real data to show.

use std::path::{Path, PathBuf};

use crate::projects::ProjectStore;
use crate::skills::{install, registry::SkillRegistry};
use crate::{Error, Result};

/// Deploy a registered skill to a registered project under the given IDE.
///
/// - Resolves the project by absolute path (must exist in `ProjectStore`).
/// - Calls `install::deploy` to create the symlink/junction/copy.
/// - Adds `skill_id` to `project.skills` (dedup) and persists.
pub fn deploy_to_project(
    skill_id: &str,
    ide_id: &str,
    project_path: &Path,
) -> Result<PathBuf> {
    let project_s = project_path.to_string_lossy().to_string();
    let mut store = ProjectStore::load()?;
    let _proj = store.get(&project_s).ok_or_else(|| {
        Error::NotFound(format!(
            "project `{project_s}` is not registered; add it in the Projects page first"
        ))
    })?;

    let mut reg = SkillRegistry::load()?;
    let mut skill = reg
        .get(skill_id)
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("skill `{skill_id}` not found")))?;
    let (link, _kind) = install::deploy(&mut skill, ide_id, Some(project_path))?;
    reg.upsert(skill);
    reg.save()?;

    // Upsert into Project.skills.
    if let Some(proj) = store.get_mut(&project_s) {
        if !proj.skills.iter().any(|s| s == skill_id) {
            proj.skills.push(skill_id.to_string());
        }
    }
    store.save()?;

    Ok(link)
}

/// Undeploy a skill from a project: removes the IDE link and drops the skill
/// id from `project.skills`.
pub fn undeploy_from_project(
    skill_id: &str,
    ide_id: &str,
    project_path: &Path,
) -> Result<()> {
    let project_s = project_path.to_string_lossy().to_string();

    let mut reg = SkillRegistry::load()?;
    let mut skill = reg
        .get(skill_id)
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("skill `{skill_id}` not found")))?;
    // Best-effort unlink; ignore "not a deployment" errors.
    let _ = install::undeploy(&mut skill, ide_id, Some(project_path));
    reg.upsert(skill);
    reg.save()?;

    if let Ok(mut store) = ProjectStore::load() {
        if let Some(proj) = store.get_mut(&project_s) {
            proj.skills.retain(|s| s != skill_id);
            let _ = store.save();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::Project;
    use crate::skills::model::{Skill, SkillSource};
    use std::collections::BTreeMap;
    use std::sync::MutexGuard;

    fn lock() -> MutexGuard<'static, ()> { crate::test_support::lock() }

    fn setup_tmp() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("AIEM_HOME", dir.path());
        dir
    }

    fn make_skill(id: &str, name: &str, path: &Path) -> Skill {
        Skill {
            id: id.to_string(),
            name: name.to_string(),
            source: SkillSource::Local { path: path.to_path_buf() },
            version: "v1".into(),
            path: path.to_path_buf(),
            description: None,
            installed_at: None,
            deployments: BTreeMap::new(),
            file_hashes: BTreeMap::new(),
        }
    }

    #[test]
    fn deploy_adds_skill_id_to_project_and_is_idempotent() {
        let _g = lock();
        let _home = setup_tmp();
        let proj_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(skill_src.path().join("content")).unwrap();

        // Register project.
        let mut store = ProjectStore::load().unwrap();
        store.upsert(Project {
            name: "demo".into(),
            path: proj_dir.path().to_string_lossy().to_string(),
            ides: vec![],
            skills: vec![],
            mcp_servers: vec![],
        });
        store.save().unwrap();

        // Register skill.
        let mut reg = SkillRegistry::load().unwrap();
        reg.upsert(make_skill("skill-a", "skill-a", skill_src.path()));
        reg.save().unwrap();

        // First deploy attaches.
        deploy_to_project("skill-a", "cursor", proj_dir.path()).unwrap();
        let st = ProjectStore::load().unwrap();
        let p = st.get(&proj_dir.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(p.skills, vec!["skill-a".to_string()]);

        // Second deploy is idempotent (no duplicate id).
        deploy_to_project("skill-a", "cursor", proj_dir.path()).unwrap();
        let st = ProjectStore::load().unwrap();
        let p = st.get(&proj_dir.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(p.skills, vec!["skill-a".to_string()]);
    }

    #[test]
    fn undeploy_removes_id() {
        let _g = lock();
        let _home = setup_tmp();
        let proj_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(skill_src.path().join("content")).unwrap();

        let mut store = ProjectStore::load().unwrap();
        store.upsert(Project {
            name: "demo".into(),
            path: proj_dir.path().to_string_lossy().to_string(),
            ides: vec![],
            skills: vec![],
            mcp_servers: vec![],
        });
        store.save().unwrap();

        let mut reg = SkillRegistry::load().unwrap();
        reg.upsert(make_skill("skill-b", "skill-b", skill_src.path()));
        reg.save().unwrap();

        deploy_to_project("skill-b", "cursor", proj_dir.path()).unwrap();
        undeploy_from_project("skill-b", "cursor", proj_dir.path()).unwrap();

        let st = ProjectStore::load().unwrap();
        let p = st.get(&proj_dir.path().to_string_lossy().to_string()).unwrap();
        assert!(p.skills.is_empty());
    }

    #[test]
    fn deploy_unregistered_project_errors() {
        let _g = lock();
        let _home = setup_tmp();
        let proj_dir = tempfile::tempdir().unwrap();

        let mut reg = SkillRegistry::load().unwrap();
        reg.upsert(make_skill("skill-c", "skill-c", proj_dir.path()));
        reg.save().unwrap();

        let err = deploy_to_project("skill-c", "cursor", proj_dir.path()).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }
}
