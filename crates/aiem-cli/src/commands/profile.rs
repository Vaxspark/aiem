use aiem_core::profiles::{Profile, ProfileStore};
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ProfileCmd {
    /// List all profiles (marks the active one with *).
    List,
    /// Create or update a profile.
    Set {
        name: String,
        #[arg(long)]
        description: Option<String>,
        /// Comma-separated skill IDs to include (empty = all).
        #[arg(long, value_delimiter = ',')]
        skills: Vec<String>,
        /// Comma-separated MCP server names to include (empty = all).
        #[arg(long = "mcp", value_delimiter = ',')]
        mcp_servers: Vec<String>,
    },
    /// Activate a profile (affects subsequent `mcp sync`).
    Use { name: String },
    /// Clear the active profile.
    Clear,
    /// Show one profile as JSON.
    Show { name: String },
    /// Delete a profile.
    #[command(alias = "rm")]
    Remove { name: String },
    /// Show which profile is currently active.
    Active,
}

pub fn run(cmd: ProfileCmd) -> anyhow::Result<()> {
    let mut store = ProfileStore::load()?;
    match cmd {
        ProfileCmd::List => {
            let active = store.active_name().map(str::to_string);
            let items: Vec<_> = store.list().cloned().collect();
            if items.is_empty() {
                println!("no profiles defined");
                return Ok(());
            }
            for p in &items {
                let marker = if active.as_deref() == Some(p.name.as_str()) { "*" } else { " " };
                println!(
                    "{marker} {:<20}  skills={:<3} mcp={:<3}  {}",
                    p.name,
                    p.skills.len(),
                    p.mcp_servers.len(),
                    p.description.as_deref().unwrap_or("")
                );
            }
        }
        ProfileCmd::Set { name, description, skills, mcp_servers } => {
            store.upsert(Profile { name: name.clone(), description, skills, mcp_servers });
            store.save()?;
            println!("✓ saved profile `{name}`");
        }
        ProfileCmd::Use { name } => {
            store.set_active(Some(&name))?;
            store.save()?;
            println!("✓ active profile: {name}");
        }
        ProfileCmd::Clear => {
            store.set_active(None)?;
            store.save()?;
            println!("✓ active profile cleared");
        }
        ProfileCmd::Show { name } => {
            let p = store.get(&name).ok_or_else(|| anyhow::anyhow!("profile `{name}` not found"))?;
            println!("{}", serde_json::to_string_pretty(p)?);
        }
        ProfileCmd::Remove { name } => {
            store.remove(&name)?;
            store.save()?;
            println!("✓ removed profile `{name}`");
        }
        ProfileCmd::Active => {
            match store.active_name() {
                Some(n) => println!("{n}"),
                None => println!("(none)"),
            }
        }
    }
    Ok(())
}
