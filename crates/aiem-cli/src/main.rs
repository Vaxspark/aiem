use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser, Debug)]
#[command(
    name = "aiem",
    version,
    about = "AI Extension Manager — unified skills & MCP manager",
    propagate_version = true
)]
pub struct Cli {
    /// Increase verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Initialize the aiem home directory (~/.aiem).
    Init,
    /// List supported IDE targets.
    Ide {
        #[command(subcommand)]
        cmd: commands::ide::IdeCmd,
    },
    /// Manage AI skills and deploy them into IDEs.
    Skill {
        #[command(subcommand)]
        cmd: commands::skill::SkillCmd,
    },
    /// Manage MCP servers and sync them across IDEs.
    Mcp {
        #[command(subcommand)]
        cmd: commands::mcp::McpCmd,
    },
    /// Manage secrets stored in the OS keyring (usable in MCP env/headers as
    /// `${secret:NAME}`).
    Secret {
        #[command(subcommand)]
        cmd: commands::secret::SecretCmd,
    },
    /// Discover existing skills & MCP servers on this machine and import them.
    Discover {
        #[command(subcommand)]
        cmd: commands::discover::DiscoverCmd,
    },
    /// Backup and restore aiem config files (local snapshots + GitHub).
    Backup {
        #[command(subcommand)]
        cmd: commands::backup::BackupCmd,
    },
    /// Start the Web UI server (browser-based management, great for headless Linux via SSH port-forward).
    #[cfg(feature = "web")]
    Serve {
        /// Host/IP to bind to. Default 127.0.0.1 (loopback only — use SSH port forwarding for remote access).
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to listen on.
        #[arg(long, default_value_t = 8787)]
        port: u16,
        /// Open the browser automatically on startup (local use only).
        #[arg(long)]
        open: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    match cli.cmd {
        Cmd::Init => commands::init()?,
        Cmd::Ide { cmd } => commands::ide::run(cmd)?,
        Cmd::Skill { cmd } => commands::skill::run(cmd).await?,
        Cmd::Mcp { cmd } => commands::mcp::run(cmd).await?,
        Cmd::Secret { cmd } => commands::secret::run(cmd)?,
        Cmd::Discover { cmd } => commands::discover::run(cmd)?,
        Cmd::Backup { cmd } => commands::backup::run(cmd)?,
        #[cfg(feature = "web")]
        Cmd::Serve { host, port, open } => {
            use std::net::{IpAddr, SocketAddr};
            let ip: IpAddr = host
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid --host `{host}`: {e}"))?;
            let cfg = aiem_web::ServeConfig {
                addr: SocketAddr::new(ip, port),
                open_browser: open,
            };
            aiem_web::serve(cfg).await?;
        }
    }
    Ok(())
}

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(format!(
            "aiem={level},aiem_cli={level},aiem_core={level}"
        ))
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .try_init();
}
