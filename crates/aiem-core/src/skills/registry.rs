use std::path::PathBuf;

use crate::fs_util::atomic_write;
use crate::projects::ProjectStore;
use crate::{paths, Result};

use super::model::{Skill, SkillIndex};

/// Thin wrapper over the on-disk skill index.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    index: SkillIndex,
}

impl SkillRegistry {
    pub fn file() -> Result<PathBuf> { Ok(paths::skills_dir()?.join("index.json")) }

    pub fn load() -> Result<Self> {
        let path = Self::file()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(path)?;
        // Strip UTF-8 BOM if present (can be added by external editors/PowerShell)
        let data = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { &bytes[3..] } else { &bytes };
        let index: SkillIndex = serde_json::from_slice(data)?;
        Ok(Self { index })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.index)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &Skill> { self.index.skills.values() }
    pub fn get(&self, id: &str) -> Option<&Skill> { self.index.skills.get(id) }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Skill> { self.index.skills.get_mut(id) }
    pub fn upsert(&mut self, s: Skill) { self.index.skills.insert(s.id.clone(), s); }
    pub fn remove(&mut self, id: &str) -> Option<Skill> { self.index.skills.remove(id) }
}

/// Reverse lookup: names of projects whose `skills` contains `skill_id`.
/// Sorted by project name.
pub fn projects_with(skill_id: &str) -> Result<Vec<String>> {
    let store = ProjectStore::load()?;
    let mut out: Vec<String> = store
        .list()
        .filter(|p| p.skills.iter().any(|s| s == skill_id))
        .map(|p| p.name.clone())
        .collect();
    out.sort();
    Ok(out)
}
