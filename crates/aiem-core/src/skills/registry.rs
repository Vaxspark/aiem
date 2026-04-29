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
    pub fn file() -> Result<PathBuf> {
        Ok(paths::skills_dir()?.join("index.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::file()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(path)?;
        // Strip UTF-8 BOM if present (can be added by external editors/PowerShell)
        let data = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &bytes[3..]
        } else {
            &bytes
        };
        let index: SkillIndex = serde_json::from_slice(data)?;
        Ok(Self { index })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.index)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &Skill> {
        self.index.skills.values()
    }
    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.index.skills.get(id)
    }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Skill> {
        self.index.skills.get_mut(id)
    }
    pub fn upsert(&mut self, s: Skill) {
        self.index.skills.insert(s.id.clone(), s);
    }
    pub fn remove(&mut self, id: &str) -> Option<Skill> {
        self.index.skills.remove(id)
    }
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

/// Read the full SKILL.md content of a skill by id.
pub fn read_skill_content(id: &str) -> Result<String> {
    let reg = SkillRegistry::load()?;
    let skill = reg
        .get(id)
        .ok_or_else(|| crate::Error::NotFound(format!("skill `{id}` not found")))?;
    let skill_md = skill.path.join("SKILL.md");
    if skill_md.is_file() {
        return Ok(std::fs::read_to_string(&skill_md)?);
    }
    let skill_md_lc = skill.path.join("skill.md");
    if skill_md_lc.is_file() {
        return Ok(std::fs::read_to_string(&skill_md_lc)?);
    }
    Err(crate::Error::NotFound(format!(
        "SKILL.md not found for `{id}` at {}",
        skill.path.display()
    )))
}

/// List all files in a skill directory, returning (relative_path, size_bytes).
pub fn list_skill_files(id: &str) -> Result<Vec<(String, u64)>> {
    let reg = SkillRegistry::load()?;
    let skill = reg
        .get(id)
        .ok_or_else(|| crate::Error::NotFound(format!("skill `{id}` not found")))?;
    let mut files = Vec::new();
    let walker = walkdir::WalkDir::new(&skill.path)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in walker {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = match entry.path().strip_prefix(&skill.path) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        files.push((rel, size));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Create a new local skill with the given name and SKILL.md content.
/// Returns the created Skill struct.
pub fn create_local_skill(name: &str, content: &str) -> Result<Skill> {
    let name = name.trim();
    if name.is_empty() {
        return Err(crate::Error::Invalid("skill name cannot be empty".into()));
    }
    let id = format!("local__{}", name.replace(' ', "-").to_lowercase());
    let dir = paths::skills_dir()?.join(&id);
    if dir.exists() {
        return Err(crate::Error::Invalid(format!(
            "skill `{id}` already exists at {}",
            dir.display()
        )));
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("SKILL.md"), content)?;

    let description = content.lines().take(3).collect::<Vec<_>>().join("\n");
    let file_hashes = {
        use sha2::{Digest, Sha256};
        let mut map = std::collections::BTreeMap::new();
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        map.insert("SKILL.md".to_string(), hash);
        map
    };

    let skill = Skill {
        id: id.clone(),
        name: name.to_string(),
        source: super::model::SkillSource::Local { path: dir.clone() },
        version: "local".to_string(),
        path: dir,
        description: Some(description),
        installed_at: Some(chrono::Utc::now().to_rfc3339()),
        deployments: Default::default(),
        file_hashes,
    };

    let mut reg = SkillRegistry::load()?;
    reg.upsert(skill.clone());
    reg.save()?;
    Ok(skill)
}
