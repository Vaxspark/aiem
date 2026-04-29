use std::collections::BTreeMap;
use std::path::PathBuf;

use aiem_core::mcp::adapters;
use aiem_core::mcp::deploy as mcp_deploy;
use aiem_core::mcp::model::{McpServer, McpTransport};
use aiem_core::mcp::sync;
use aiem_core::mcp::McpRegistry;
use clap::{Args, Subcommand, ValueEnum};

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum TransportKind {
    Stdio,
    Http,
    Sse,
}

#[derive(Subcommand, Debug)]
pub enum McpCmd {
    /// Add or update an MCP server definition in aiem's registry.
    Add(AddArgs),
    /// Add MCP server(s) from a JSON block (same format as Claude/Codex config).
    /// Reads from stdin or --input. For AI agent integration.
    AddJson {
        /// JSON string. If omitted, reads from stdin.
        #[arg(long)]
        input: Option<String>,
    },
    /// List all MCP servers in aiem's registry.
    List,
    /// Show one server's full definition as JSON.
    Show { name: String },
    /// Remove an MCP server from the registry and retract from all IDE configs.
    Remove { name: String },
    /// Mark an MCP server as disabled (keeps the definition).
    Disable { name: String },
    /// Re-enable a disabled MCP server.
    Enable { name: String },
    /// Add an IDE target for a server.
    TargetAdd { name: String, ide: String },
    /// Remove an IDE target for a server.
    TargetRemove { name: String, ide: String },
    /// Write managed servers to every targeted IDE's native config.
    Sync {
        /// Only sync to these IDEs (comma-separated). Default: all targets.
        #[arg(long, value_delimiter = ',')]
        ide: Vec<String>,
        /// Project root (required for project-scoped configs).
        #[arg(long)]
        project: Option<PathBuf>,
        /// Print the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Import servers from an IDE's existing config into aiem's registry.
    Import {
        /// Which IDE to import from (currently: codex).
        #[arg(long)]
        from: String,
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Print the config file path for an IDE.
    Path {
        #[arg(long)]
        ide: String,
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// List IDEs that support MCP sync.
    Supported,
    /// Attach an MCP server to a registered project and sync to its IDE
    /// config files immediately.
    Deploy {
        /// MCP server name (as registered in aiem's registry).
        name: String,
        /// Path to a project registered with `aiem project add`.
        #[arg(long)]
        project: PathBuf,
    },
    /// Detach an MCP server from a registered project and remove it from the
    /// project's IDE config files.
    Undeploy {
        /// MCP server name.
        name: String,
        /// Path to a project registered with `aiem project add`.
        #[arg(long)]
        project: PathBuf,
    },
    /// Import MCP server(s) from a GitHub repository.
    ///
    /// Two-phase flow: analyze the repo first (--preview), then confirm import.
    ImportGithub {
        /// owner/repo (e.g. `modelcontextprotocol/servers`).
        repo: String,
        /// Git ref (branch/tag/commit). Defaults to `main`.
        #[arg(long, default_value = "main")]
        r#ref: String,
        /// Only preview the detected servers without saving anything.
        #[arg(long)]
        preview: bool,
        /// Skip the confirmation prompt and import all detected servers.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Canonical server name (used as the key in IDE configs).
    pub name: String,
    /// Transport type.
    #[arg(long, value_enum, default_value_t = TransportKind::Stdio)]
    pub r#type: TransportKind,
    /// Command to launch (stdio transport). Required when --type stdio.
    #[arg(long)]
    pub command: Option<String>,
    /// Arguments for the command. Use `--arg foo --arg bar` (leading dashes OK).
    #[arg(long = "arg", allow_hyphen_values = true)]
    pub arg: Vec<String>,
    /// Environment variables, `KEY=VALUE` (can be repeated).
    #[arg(long = "env", value_parser = parse_kv, allow_hyphen_values = true)]
    pub env: Vec<(String, String)>,
    /// Working directory.
    #[arg(long)]
    pub cwd: Option<String>,
    /// Existing bundle name to attach to this stdio server.
    #[arg(long)]
    pub bundle: Option<String>,
    /// Local bundle directory to import before saving this stdio server.
    #[arg(long)]
    pub bundle_src: Option<PathBuf>,
    /// URL for http / sse transports.
    #[arg(long)]
    pub url: Option<String>,
    /// HTTP headers, `KEY=VALUE` (can be repeated).
    #[arg(long = "header", value_parser = parse_kv, allow_hyphen_values = true)]
    pub header: Vec<(String, String)>,
    /// IDE targets (comma-separated). Defaults to every supported IDE.
    #[arg(long, value_delimiter = ',')]
    pub target: Vec<String>,
    #[arg(long)]
    pub description: Option<String>,
    /// Tags (comma-separated).
    #[arg(long, value_delimiter = ',')]
    pub tag: Vec<String>,
}

fn parse_kv(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("expected KEY=VALUE, got `{s}`"))?;
    Ok((k.to_string(), v.to_string()))
}

pub async fn run(cmd: McpCmd) -> anyhow::Result<()> {
    match cmd {
        McpCmd::Add(a) => add(a),
        McpCmd::AddJson { input } => add_json(input),
        McpCmd::List => list(),
        McpCmd::Show { name } => show(&name),
        McpCmd::Remove { name } => remove(&name),
        McpCmd::Disable { name } => toggle(&name, true),
        McpCmd::Enable { name } => toggle(&name, false),
        McpCmd::TargetAdd { name, ide } => target(&name, &ide, true),
        McpCmd::TargetRemove { name, ide } => target(&name, &ide, false),
        McpCmd::Sync {
            ide,
            project,
            dry_run,
        } => sync_cmd(ide, project, dry_run),
        McpCmd::Import { from, project } => import(&from, project),
        McpCmd::Path { ide, project } => path_cmd(&ide, project),
        McpCmd::Supported => {
            for ide in adapters::SUPPORTED {
                println!("{ide}");
            }
            Ok(())
        }
        McpCmd::Deploy { name, project } => deploy_cmd(&name, &project),
        McpCmd::Undeploy { name, project } => undeploy_cmd(&name, &project),
        McpCmd::ImportGithub {
            repo,
            r#ref,
            preview,
            yes,
        } => import_github_cmd(&repo, &r#ref, preview, yes).await,
    }
}

fn deploy_cmd(name: &str, project: &std::path::Path) -> anyhow::Result<()> {
    let touched = mcp_deploy::deploy_to_project(name, project)?;
    if touched.is_empty() {
        println!("(server `{name}` attached to project but no IDE configs were written — check server.targets)");
    } else {
        for (ide, path) in touched {
            println!("✓ {ide:<12} → {}", path.display());
        }
    }
    Ok(())
}

fn undeploy_cmd(name: &str, project: &std::path::Path) -> anyhow::Result<()> {
    let touched = mcp_deploy::undeploy_from_project(name, project)?;
    println!("✓ removed `{name}` from project");
    for (ide, path) in touched {
        println!("  re-synced {ide:<12} → {}", path.display());
    }
    Ok(())
}

async fn import_github_cmd(
    repo: &str,
    r#ref: &str,
    preview_only: bool,
    yes: bool,
) -> anyhow::Result<()> {
    let (owner, name) = repo
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("expected owner/repo format"))?;

    let preview =
        aiem_core::mcp::github::preview_github_mcp(owner, name, Some(r#ref), None).await?;

    println!(
        "Repository: {}/{}  ref: {}  commit: {}",
        preview.owner,
        preview.repo,
        preview.r#ref,
        &preview.commit[..preview.commit.len().min(12)]
    );
    for w in &preview.warnings {
        println!("⚠ {w}");
    }

    if preview.servers.is_empty() {
        println!("No MCP servers detected.");
        return Ok(());
    }

    println!("\nDetected {} server(s):\n", preview.servers.len());
    for (i, ps) in preview.servers.iter().enumerate() {
        let rt_str = ps
            .server
            .runtime
            .map(|r| format!("{r:?}"))
            .unwrap_or_else(|| "unknown".into());
        println!("  [{}] {} (runtime: {})", i + 1, ps.server.name, rt_str);
        if let Some(ep) = &ps.entrypoint {
            println!("      entry: {ep}");
        }
        println!(
            "      files: {} kept, {} dropped",
            ps.kept_files.len(),
            ps.dropped_files.len()
        );
        if !ps.dropped_files.is_empty() {
            let sample: Vec<&str> = ps
                .dropped_files
                .iter()
                .take(5)
                .map(|s| s.as_str())
                .collect();
            println!("      dropped sample: {}", sample.join(", "));
        }
        if !ps.detected_secrets.is_empty() {
            println!(
                "      secrets: {} token(s) will be saved to Vault",
                ps.detected_secrets.len()
            );
        }
        for w in &ps.warnings {
            println!("      ⚠ {w}");
        }
    }

    if preview_only {
        println!("\n(preview only — use --yes to import)");
        return Ok(());
    }

    if !yes {
        println!("\nUse --yes to confirm import, or --preview to only preview.");
        return Ok(());
    }

    let imported = aiem_core::mcp::github::import_github_mcp(&preview, None).await?;
    println!("\n✓ Imported {} server(s):", imported.len());
    for name in &imported {
        println!("  - {name}");
    }
    Ok(())
}

fn add(a: AddArgs) -> anyhow::Result<()> {
    let bundle_name = a
        .bundle
        .clone()
        .or_else(|| a.bundle_src.as_ref().map(|_| a.name.clone()));
    if let (Some(bundle), Some(src)) = (&bundle_name, &a.bundle_src) {
        aiem_core::mcp::bundles::import_bundle(bundle, src)?;
    }

    let transport = match a.r#type {
        TransportKind::Stdio => {
            let command = a
                .command
                .ok_or_else(|| anyhow::anyhow!("--command is required for stdio"))?;
            McpTransport::Stdio {
                command,
                args: a.arg,
                env: a.env.into_iter().collect(),
                cwd: a.cwd,
                bundle: bundle_name,
            }
        }
        TransportKind::Http => {
            let url = a
                .url
                .ok_or_else(|| anyhow::anyhow!("--url is required for http"))?;
            McpTransport::Http {
                url,
                headers: a.header.into_iter().collect(),
            }
        }
        TransportKind::Sse => {
            let url = a
                .url
                .ok_or_else(|| anyhow::anyhow!("--url is required for sse"))?;
            McpTransport::Sse {
                url,
                headers: a.header.into_iter().collect(),
            }
        }
    };
    let server = McpServer {
        name: a.name.clone(),
        transport,
        targets: default_mcp_targets(a.target),
        description: a.description,
        tags: a.tag,
        disabled: false,
        source: None,
        runtime: None,
        auth_mode: Default::default(),
    };
    let mut reg = McpRegistry::load()?;
    reg.upsert(server);
    reg.save()?;
    println!("✓ saved mcp server `{}`", a.name);
    Ok(())
}

fn add_json(input: Option<String>) -> anyhow::Result<()> {
    let json_str = match input {
        Some(s) => s,
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };

    let val: serde_json::Value = serde_json::from_str(json_str.trim())?;
    let obj = val
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("expected JSON object"))?;

    let mut servers = Vec::new();

    // Single server with "command"/"url" at top level
    if obj.contains_key("command") || obj.contains_key("url") {
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        servers.push(json_to_mcp_server(&name, &val)?);
    } else {
        // Map of name -> config
        for (name, config) in obj {
            servers.push(json_to_mcp_server(name, config)?);
        }
    }

    let mut reg = McpRegistry::load()?;
    let count = servers.len();
    for s in servers {
        reg.upsert(s);
    }
    reg.save()?;
    println!("{}", serde_json::json!({ "added": count }));
    Ok(())
}

fn json_to_mcp_server(name: &str, val: &serde_json::Value) -> anyhow::Result<McpServer> {
    let obj = val
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("{name}: expected object"))?;

    let transport = if let Some(cmd) = obj.get("command").and_then(|v| v.as_str()) {
        let args: Vec<String> = obj
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let env: BTreeMap<String, String> = obj
            .get("env")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let cwd = obj.get("cwd").and_then(|v| v.as_str()).map(String::from);
        let bundle = obj.get("bundle").and_then(|v| v.as_str()).map(String::from);
        McpTransport::Stdio {
            command: cmd.to_string(),
            args,
            env,
            cwd,
            bundle,
        }
    } else if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
        let headers: BTreeMap<String, String> = obj
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("sse");
        if kind == "http" {
            McpTransport::Http {
                url: url.to_string(),
                headers,
            }
        } else {
            McpTransport::Sse {
                url: url.to_string(),
                headers,
            }
        }
    } else {
        anyhow::bail!("{name}: need 'command' (stdio) or 'url' (http/sse)");
    };

    let targets: Vec<String> = obj
        .get("targets")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| default_mcp_targets(Vec::new()));

    Ok(McpServer {
        name: name.to_string(),
        transport,
        targets,
        description: obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from),
        tags: vec![],
        disabled: false,
        source: None,
        runtime: None,
        auth_mode: Default::default(),
    })
}

fn list() -> anyhow::Result<()> {
    let reg = McpRegistry::load()?;
    let mut any = false;
    println!("{:<24} {:<8} {:<8} {}", "NAME", "TYPE", "STATE", "TARGETS");
    for s in reg.list() {
        any = true;
        let ty = match s.transport {
            McpTransport::Stdio { .. } => "stdio",
            McpTransport::Http { .. } => "http",
            McpTransport::Sse { .. } => "sse",
        };
        let state = if s.disabled { "disabled" } else { "enabled" };
        println!(
            "{:<24} {:<8} {:<8} {}",
            s.name,
            ty,
            state,
            s.targets.join(",")
        );
    }
    if !any {
        println!(
            "(no mcp servers yet — try `aiem mcp add <name> --command npx --arg -y --arg pkg`; targets default to every supported IDE)"
        );
    }
    Ok(())
}

fn default_mcp_targets(targets: Vec<String>) -> Vec<String> {
    if targets.is_empty() {
        adapters::SUPPORTED
            .iter()
            .map(|ide| ide.to_string())
            .collect()
    } else {
        targets
    }
}

fn show(name: &str) -> anyhow::Result<()> {
    let reg = McpRegistry::load()?;
    let s = reg
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("mcp server `{name}` not found"))?;
    println!("{}", serde_json::to_string_pretty(s)?);
    Ok(())
}

fn remove(name: &str) -> anyhow::Result<()> {
    let mut reg = McpRegistry::load()?;
    reg.remove(name)?;
    reg.save()?;
    println!("✓ removed {name} (also retracted from IDE configs)");
    Ok(())
}

fn toggle(name: &str, disabled: bool) -> anyhow::Result<()> {
    let mut reg = McpRegistry::load()?;
    let s = reg
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("mcp server `{name}` not found"))?;
    s.disabled = disabled;
    reg.save()?;
    println!(
        "✓ {name} is now {}",
        if disabled { "disabled" } else { "enabled" }
    );
    Ok(())
}

fn target(name: &str, ide: &str, add: bool) -> anyhow::Result<()> {
    let mut reg = McpRegistry::load()?;
    let s = reg
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("mcp server `{name}` not found"))?;
    if add {
        if !s.targets.iter().any(|x| x == ide) {
            s.targets.push(ide.to_string());
        }
    } else {
        s.targets.retain(|x| x != ide);
    }
    let targets_display = s.targets.join(",");
    // end mutable borrow before calling save
    let _ = s;
    reg.save()?;
    println!("✓ {name} targets: {targets_display}");
    Ok(())
}

fn sync_cmd(ides: Vec<String>, project: Option<PathBuf>, dry_run: bool) -> anyhow::Result<()> {
    let reg = McpRegistry::load()?;
    let plan = sync::plan(&reg, &ides, None);
    if plan.writes.is_empty() {
        println!("(nothing to sync)");
        return Ok(());
    }
    for (ide, names) in &plan.writes {
        println!("{ide}:");
        for n in names {
            println!("  • {n}");
        }
    }
    if dry_run {
        println!("(dry-run: no files were written)");
        return Ok(());
    }
    let touched = sync::execute(&reg, &plan, project.as_deref(), None)?;
    for (ide, path) in touched {
        println!("✓ wrote {ide:<12} → {}", path.display());
    }
    Ok(())
}

fn import(from: &str, project: Option<PathBuf>) -> anyhow::Result<()> {
    let imported: Vec<McpServer> = match from {
        "codex" => adapters::codex::read(project.as_deref())?,
        other => anyhow::bail!("import from `{other}` not yet supported"),
    };
    if imported.is_empty() {
        println!("(no servers found in {from})");
        return Ok(());
    }
    let mut reg = McpRegistry::load()?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for mut s in imported {
        // Keep any existing targets, add this IDE.
        if let Some(existing) = reg.get(&s.name) {
            for t in &existing.targets {
                if !s.targets.contains(t) {
                    s.targets.push(t.clone());
                }
            }
        }
        *counts.entry(s.name.clone()).or_default() += 1;
        reg.upsert(s);
    }
    reg.save()?;
    println!("✓ imported {} server(s) from {from}", counts.len());
    Ok(())
}

fn path_cmd(ide: &str, project: Option<PathBuf>) -> anyhow::Result<()> {
    let p = adapters::config_path(ide, project.as_deref())?;
    println!("{}", p.display());
    Ok(())
}
