//! aiem-core: unified AI skills & MCP manager core library.

pub mod backup;
pub mod discover;
pub mod error;
pub mod fs_util;
pub mod ide;
pub mod mcp;
pub mod paths;
pub mod projects;
pub mod secrets;
pub mod skills;

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
