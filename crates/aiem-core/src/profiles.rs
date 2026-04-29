//! Profiles — named overlays that select a subset of skills & MCP servers.
//!
//! Use cases:
//! - "work"  : company MCP servers + internal skills
//! - "oss"   : only public servers, no secrets
//! - "demo"  : minimal set for screenshots
//!
//! When a profile is *active*, `mcp sync` only writes servers listed in the
//! profile, and `skill deploy --all` only deploys skills listed in the profile.
//! The profile is a filter — it never edits the underlying registries.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::fs_util::{atomic_write, strip_utf8_bom};
use crate::mcp::McpRegistry;
use crate::skills::SkillRegistry;
use crate::{paths, Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Skill IDs to include. Empty means "all".
    #[serde(default)]
    pub skills: Vec<String>,
    /// MCP server names to include. Empty means "all".
    #[serde(default)]
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProfilesFile {
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Default)]
pub struct ProfileStore {
    inner: ProfilesFile,
}

impl ProfileStore {
    pub fn file() -> Result<PathBuf> {
        paths::profiles_file()
    }

    pub fn load() -> Result<Self> {
        let p = Self::file()?;
        if !p.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&p)?;
        let inner: ProfilesFile = serde_json::from_slice(strip_utf8_bom(&bytes))?;
        Ok(Self { inner })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.inner)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &Profile> {
        self.inner.profiles.values()
    }
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.inner.profiles.get(name)
    }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.inner.profiles.get_mut(name)
    }

    pub fn upsert(&mut self, p: Profile) {
        self.inner.profiles.insert(p.name.clone(), p);
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        if self.inner.profiles.remove(name).is_none() {
            return Err(Error::NotFound(format!("profile `{name}` not found")));
        }
        if self.inner.active.as_deref() == Some(name) {
            self.inner.active = None;
        }
        Ok(())
    }

    pub fn active_name(&self) -> Option<&str> {
        self.inner.active.as_deref()
    }

    pub fn active(&self) -> Option<&Profile> {
        self.inner
            .active
            .as_deref()
            .and_then(|n| self.inner.profiles.get(n))
    }

    pub fn set_active(&mut self, name: Option<&str>) -> Result<()> {
        match name {
            None => {
                self.inner.active = None;
                Ok(())
            }
            Some(n) => {
                if !self.inner.profiles.contains_key(n) {
                    return Err(Error::NotFound(format!("profile `{n}` not found")));
                }
                self.inner.active = Some(n.to_string());
                Ok(())
            }
        }
    }

    /// Validate all profiles against the current skill and MCP registries.
    /// Returns a list of human-readable warning strings for references that
    /// cannot be resolved (typos, removed items, etc.).
    pub fn validate(&self) -> Vec<String> {
        let skill_reg = SkillRegistry::load().ok();
        let mcp_reg = McpRegistry::load().ok();
        let mut warnings = Vec::new();
        for p in self.inner.profiles.values() {
            for sid in &p.skills {
                let found = skill_reg.as_ref().map_or(false, |r| r.get(sid).is_some());
                if !found {
                    warnings.push(format!(
                        "profile `{}`: skill `{sid}` not found in registry",
                        p.name
                    ));
                }
            }
            for mname in &p.mcp_servers {
                let found = mcp_reg.as_ref().map_or(false, |r| r.get(mname).is_some());
                if !found {
                    warnings.push(format!(
                        "profile `{}`: MCP server `{mname}` not found in registry",
                        p.name
                    ));
                }
            }
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    struct Isolated {
        _dir: tempfile::TempDir,
        _guard: MutexGuard<'static, ()>,
    }

    fn isolate() -> Isolated {
        let guard = crate::test_support::lock();
        let dir = tempfile::tempdir().expect("tempdir");
        std::env::set_var("AIEM_HOME", dir.path());
        Isolated {
            _dir: dir,
            _guard: guard,
        }
    }

    #[test]
    fn roundtrip() {
        let _h = isolate();
        let mut store = ProfileStore::load().unwrap();
        store.upsert(Profile {
            name: "test".into(),
            description: Some("d".into()),
            skills: vec!["s1".into()],
            mcp_servers: vec!["m1".into()],
        });
        store.save().unwrap();
        let store2 = ProfileStore::load().unwrap();
        let p = store2.get("test").unwrap();
        assert_eq!(p.skills, vec!["s1".to_string()]);
    }

    #[test]
    fn set_active_rejects_missing() {
        let _h = isolate();
        let mut store = ProfileStore::load().unwrap();
        assert!(store.set_active(Some("nope")).is_err());
    }

    #[test]
    fn load_with_bom() {
        let _h = isolate();
        let p = ProfileStore::file().unwrap();
        crate::paths::ensure_layout().unwrap();
        let mut data = vec![0xEF, 0xBB, 0xBF];
        data.extend_from_slice(b"{\"profiles\":{}}");
        std::fs::write(&p, &data).unwrap();
        let store = ProfileStore::load().unwrap();
        assert_eq!(store.list().count(), 0);
    }

    #[test]
    fn remove_active_clears_active() {
        let _h = isolate();
        let mut store = ProfileStore::load().unwrap();
        store.upsert(Profile {
            name: "x".into(),
            ..Default::default()
        });
        store.set_active(Some("x")).unwrap();
        store.remove("x").unwrap();
        assert!(store.active_name().is_none());
    }

    #[test]
    fn validate_reports_missing_refs() {
        let _h = isolate();
        let mut store = ProfileStore::load().unwrap();
        store.upsert(Profile {
            name: "bad".into(),
            skills: vec!["nonexistent_skill".into()],
            mcp_servers: vec!["nonexistent_mcp".into()],
            ..Default::default()
        });
        let warnings = store.validate();
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("nonexistent_skill"));
        assert!(warnings[1].contains("nonexistent_mcp"));
    }

    #[test]
    fn validate_empty_profile_no_warnings() {
        let _h = isolate();
        let mut store = ProfileStore::load().unwrap();
        store.upsert(Profile {
            name: "ok".into(),
            ..Default::default()
        });
        assert!(store.validate().is_empty());
    }
}
