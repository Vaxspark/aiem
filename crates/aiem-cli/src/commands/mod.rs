pub mod ide;
pub mod mcp;
pub mod skill;
pub mod secret;
pub mod profile;
pub mod discover;
pub mod backup;

use aiem_core::paths;

pub fn init() -> anyhow::Result<()> {
    paths::ensure_layout()?;
    println!("aiem home: {}", paths::home()?.display());
    println!("skills:    {}", paths::skills_dir()?.display());
    println!("mcp:       {}", paths::mcp_dir()?.display());
    println!("backups:   {}", paths::backups_dir()?.display());
    Ok(())
}
