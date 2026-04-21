use std::path::PathBuf;

use crate::fs_util::atomic_write;
use crate::{paths, Error, Result};

use super::model::{McpRegistryFile, McpServer};

#[derive(Debug, Default)]
pub struct McpRegistry {
    inner: McpRegistryFile,
}

impl McpRegistry {
    pub fn file() -> Result<PathBuf> { paths::mcp_servers_file() }

    pub fn load() -> Result<Self> {
        let path = Self::file()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&path)?;
        // Strip UTF-8 BOM if present
        let data = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { &bytes[3..] } else { &bytes };
        let inner: McpRegistryFile = serde_json::from_slice(data)?;
        Ok(Self { inner })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.inner)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &McpServer> { self.inner.servers.values() }
    pub fn get(&self, name: &str) -> Option<&McpServer> { self.inner.servers.get(name) }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut McpServer> { self.inner.servers.get_mut(name) }

    pub fn upsert(&mut self, s: McpServer) {
        self.inner.servers.insert(s.name.clone(), s);
    }

    pub fn remove(&mut self, name: &str) -> Result<McpServer> {
        self.inner
            .servers
            .remove(name)
            .ok_or_else(|| Error::NotFound(format!("mcp server `{name}` not found")))
    }
}
