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

use crate::fs_util::atomic_write;
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
    pub fn file() -> Result<PathBuf> { paths::profiles_file() }

    pub fn load() -> Result<Self> {
        let p = Self::file()?;
        if !p.exists() { return Ok(Self::default()); }
        let bytes = std::fs::read(&p)?;
        let inner: ProfilesFile = serde_json::from_slice(&bytes)?;
        Ok(Self { inner })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.inner)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &Profile> { self.inner.profiles.values() }
    pub fn get(&self, name: &str) -> Option<&Profile> { self.inner.profiles.get(name) }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Profile> { self.inner.profiles.get_mut(name) }

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

    pub fn active_name(&self) -> Option<&str> { self.inner.active.as_deref() }

    pub fn active(&self) -> Option<&Profile> {
        self.inner.active.as_deref().and_then(|n| self.inner.profiles.get(n))
    }

    pub fn set_active(&mut self, name: Option<&str>) -> Result<()> {
        match name {
            None => { self.inner.active = None; Ok(()) }
            Some(n) => {
                if !self.inner.profiles.contains_key(n) {
                    return Err(Error::NotFound(format!("profile `{n}` not found")));
                }
                self.inner.active = Some(n.to_string());
                Ok(())
            }
        }
    }
}
