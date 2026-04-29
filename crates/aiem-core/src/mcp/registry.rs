use std::path::PathBuf;

use crate::fs_util::atomic_write;
use crate::{paths, Error, Result};

use super::model::{McpRegistryFile, McpServer};

#[derive(Debug, Default)]
pub struct McpRegistry {
    inner: McpRegistryFile,
}

impl McpRegistry {
    pub fn file() -> Result<PathBuf> {
        paths::mcp_servers_file()
    }

    pub fn load() -> Result<Self> {
        let path = Self::file()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&path)?;
        // Strip UTF-8 BOM if present
        let data = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &bytes[3..]
        } else {
            &bytes
        };
        let inner: McpRegistryFile = serde_json::from_slice(data)?;
        Ok(Self { inner })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.inner)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &McpServer> {
        self.inner.servers.values()
    }
    pub fn get(&self, name: &str) -> Option<&McpServer> {
        self.inner.servers.get(name)
    }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut McpServer> {
        self.inner.servers.get_mut(name)
    }

    pub fn upsert(&mut self, s: McpServer) {
        self.inner.servers.insert(s.name.clone(), s);
    }

    /// Remove a server from the registry.
    ///
    /// Also retracts it from all IDE configs it targets (both global and
    /// project-scoped via [`crate::projects::ProjectStore`]), and cleans up
    /// its on-disk bundle if any.  Retract failures are logged but do not
    /// prevent the removal from succeeding.
    pub fn remove(&mut self, name: &str) -> Result<McpServer> {
        let server = self
            .inner
            .servers
            .remove(name)
            .ok_or_else(|| Error::NotFound(format!("mcp server `{name}` not found")))?;

        let names = vec![name.to_string()];

        // Retract from global IDE configs.
        for ide in &server.targets {
            if let Err(e) = crate::mcp::adapters::retract(ide, None, &names) {
                tracing::debug!(ide, error = %e, "retract from global config (non-fatal)");
            }
        }

        // Retract from project-scoped configs and clean up project.mcp_servers.
        if let Ok(mut store) = crate::projects::ProjectStore::load() {
            let projects: Vec<(String, Vec<String>)> = store
                .list()
                .filter(|p| p.mcp_servers.iter().any(|n| n == name))
                .map(|p| (p.path.clone(), p.ides.clone()))
                .collect();
            for (path, _ides) in &projects {
                if let Some(proj) = store.get_mut(path) {
                    proj.mcp_servers.retain(|n| n != name);
                }
                let root = std::path::Path::new(path);
                for ide in &server.targets {
                    if let Err(e) = crate::mcp::adapters::retract(ide, Some(root), &names) {
                        tracing::debug!(ide, path, error = %e, "retract from project config (non-fatal)");
                    }
                }
            }
            if !projects.is_empty() {
                let _ = store.save();
            }
        }

        // Clean up on-disk bundle only if no remaining server references it.
        if let crate::mcp::model::McpTransport::Stdio {
            bundle: Some(ref b),
            ..
        } = &server.transport
        {
            let still_used = self.inner.servers.values().any(|s| {
                matches!(
                    &s.transport,
                    crate::mcp::model::McpTransport::Stdio { bundle: Some(other), .. }
                    if other == b
                )
            });
            if !still_used {
                let _ = crate::mcp::bundles::remove_bundle(b);
            }
        }
        Ok(server)
    }
}
