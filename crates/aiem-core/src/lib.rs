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

pub use error::{Error, Result};
