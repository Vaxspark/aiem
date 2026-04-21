//! aiem-core: unified AI skills & MCP manager core library.

pub mod error;
pub mod paths;
pub mod fs_util;
pub mod ide;
pub mod skills;
pub mod mcp;
pub mod secrets;
pub mod profiles;
pub mod projects;
pub mod discover;
pub mod registry;
pub mod backup;

pub use error::{Error, Result};

#[cfg(test)]
pub(crate) mod test_support {
    //! Shared `AIEM_HOME` serialization lock. Tests across modules share
    //! process-wide env vars, so we must serialize any test that calls
    //! `std::env::set_var("AIEM_HOME", ...)`.
    use std::sync::{Mutex, MutexGuard};
    pub static ENV_LOCK: Mutex<()> = Mutex::new(());
    pub fn lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }
}
