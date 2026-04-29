use aiem_core::discover;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum DiscoverCmd {
    /// Scan this machine for skills & MCP servers not yet managed by aiem.
    Scan,
    /// Import ALL discovered items into aiem's unified registry.
    ImportAll {
        /// Copy discovered skill folders into ~/.aiem/skills/ (default: reference in-place).
        #[arg(long)]
        copy: bool,
    },
    /// Import only discovered MCP servers.
    ImportMcp,
    /// Import only discovered skills.
    ImportSkills {
        /// Copy skill folders into ~/.aiem/skills/.
        #[arg(long)]
        copy: bool,
    },
}

pub fn run(cmd: DiscoverCmd) -> anyhow::Result<()> {
    match cmd {
        DiscoverCmd::Scan => {
            println!("Scanning for unmanaged skills…");
            let skills = discover::discover_skills()?;
            if skills.is_empty() {
                println!("  (none found)");
            } else {
                println!("  Found {} unmanaged skill(s):", skills.len());
                for f in &skills {
                    let link_tag = if f.is_link { " [link]" } else { "" };
                    println!(
                        "    • {:<30} IDE: {:<12} {}{}",
                        f.dir_name,
                        f.ide_id,
                        f.path.display(),
                        link_tag
                    );
                }
            }

            println!();
            println!("Scanning for unmanaged MCP servers…");
            let mcp = discover::discover_mcp()?;
            if mcp.is_empty() {
                println!("  (none found)");
            } else {
                println!("  Found {} unmanaged MCP server(s):", mcp.len());
                for f in &mcp {
                    let transport = match &f.server.transport {
                        aiem_core::mcp::McpTransport::Stdio { command, .. } => {
                            format!("stdio: {command}")
                        }
                        aiem_core::mcp::McpTransport::Http { url, .. } => format!("http: {url}"),
                        aiem_core::mcp::McpTransport::Sse { url, .. } => format!("sse: {url}"),
                    };
                    println!(
                        "    • {:<24} from: {:<12} targets: [{}]  ({})",
                        f.server.name,
                        f.source_ide,
                        f.server.targets.join(", "),
                        transport
                    );
                }
            }

            let total = skills.len() + mcp.len();
            if total > 0 {
                println!();
                println!("Run `aiem discover import-all` to import everything, or use");
                println!("`aiem discover import-mcp` / `aiem discover import-skills` selectively.");
            }
        }
        DiscoverCmd::ImportAll { copy } => {
            let skills = discover::discover_skills()?;
            let mcp = discover::discover_mcp()?;
            let sc = discover::import_all_skills(&skills, copy)?;
            let mc = discover::import_all_mcp(&mcp)?;
            println!("✓ imported {sc} skill(s), {mc} MCP server(s)");
            if mc > 0 {
                println!("  Tip: run `aiem mcp sync` to push them to IDE configs.");
            }
        }
        DiscoverCmd::ImportMcp => {
            let found = discover::discover_mcp()?;
            if found.is_empty() {
                println!("No unmanaged MCP servers found.");
                return Ok(());
            }
            let count = discover::import_all_mcp(&found)?;
            println!("✓ imported {count} MCP server(s)");
        }
        DiscoverCmd::ImportSkills { copy } => {
            let found = discover::discover_skills()?;
            if found.is_empty() {
                println!("No unmanaged skills found.");
                return Ok(());
            }
            let count = discover::import_all_skills(&found, copy)?;
            println!("✓ imported {count} skill(s)");
        }
    }
    Ok(())
}
