use std::sync::Arc;

use aiem_core::mcp::McpRegistry;
use aiem_core::profiles::ProfileStore;
use aiem_core::projects::ProjectStore;
use aiem_core::secrets::Vault;
use aiem_core::skills::SkillRegistry;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::events::UiEvent;

/// Shared application state passed to every axum handler.
///
/// Registries are lazy-reloaded on access to stay in sync with disk writes
/// performed by the CLI or by other processes.
#[derive(Clone)]
pub struct AppState {
    pub events: broadcast::Sender<UiEvent>,
    /// Serializes write access across all resource kinds — a coarse lock is
    /// plenty for a single-user tool and keeps ordering predictable.
    pub write_lock: Arc<Mutex<()>>,
    /// Monotonically-increasing task id counter.
    pub task_counter: Arc<RwLock<u64>>,
}

impl AppState {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, _rx) = broadcast::channel::<UiEvent>(256);
        Ok(Self {
            events: tx,
            write_lock: Arc::new(Mutex::new(())),
            task_counter: Arc::new(RwLock::new(0)),
        })
    }

    pub async fn next_task_id(&self) -> u64 {
        let mut g = self.task_counter.write().await;
        *g += 1;
        *g
    }

    // --- Registry loaders (always read fresh from disk). ----------------

    pub fn skills(&self) -> anyhow::Result<SkillRegistry> {
        SkillRegistry::load().map_err(Into::into)
    }
    pub fn mcp(&self) -> anyhow::Result<McpRegistry> {
        McpRegistry::load().map_err(Into::into)
    }
    pub fn vault(&self) -> anyhow::Result<Vault> {
        Vault::load().map_err(Into::into)
    }
    pub fn profiles(&self) -> anyhow::Result<ProfileStore> {
        ProfileStore::load().map_err(Into::into)
    }
    pub fn projects(&self) -> anyhow::Result<ProjectStore> {
        ProjectStore::load().map_err(Into::into)
    }
}
