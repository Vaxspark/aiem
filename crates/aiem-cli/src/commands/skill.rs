use std::path::PathBuf;

use aiem_core::skills::{github, install, model::SkillSource, SkillRegistry};
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum SkillCmd {
    /// Add (download) a skill from GitHub into the local repository.
    /// Auto-detects multiple skill subdirectories in a repo.
    Add {
        /// Source: `owner/repo[//subdir][@ref]`, `github:owner/repo`, or a GitHub URL.
        source: String,
        /// Human-friendly name. Defaults to the repo or subdir name.
        #[arg(long)]
        name: Option<String>,
        /// Branch / tag / commit. Overrides `@ref` in source.
        #[arg(long = "ref")]
        r#ref: Option<String>,
        /// Subdirectory inside the repo that is the actual skill. Overrides `//subdir`.
        #[arg(long)]
        subdir: Option<String>,
        /// Output results as JSON (for AI agent integration).
        #[arg(long)]
        json: bool,
    },
    /// List all locally-managed skills.
    List,
    /// Show detailed info for a skill.
    Info { id: String },
    /// Re-download the latest version of a skill from its source.
    Update { id: String },
    /// Remove a skill from the local repository (also undeploys from IDEs).
    Remove { id: String },
    /// Link a skill into an IDE's skills directory.
    Deploy {
        id: String,
        /// Target IDE id (see `aiem ide list`).
        #[arg(long)]
        ide: String,
        /// Project root (required for project-scoped IDEs).
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Remove a skill link from an IDE's skills directory.
    Undeploy {
        id: String,
        #[arg(long)]
        ide: String,
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Deploy a skill to multiple IDEs at once.
    Sync {
        id: String,
        /// Comma-separated list of IDE ids.
        #[arg(long, value_delimiter = ',')]
        ides: Vec<String>,
        #[arg(long)]
        project: Option<PathBuf>,
    },
}

pub async fn run(cmd: SkillCmd) -> anyhow::Result<()> {
    match cmd {
        SkillCmd::Add { source, name, r#ref, subdir, json } => add(source, name, r#ref, subdir, json).await,
        SkillCmd::List => list(),
        SkillCmd::Info { id } => info(&id),
        SkillCmd::Update { id } => update(&id).await,
        SkillCmd::Remove { id } => remove(&id),
        SkillCmd::Deploy { id, ide, project } => deploy(&id, &ide, project.as_deref()),
        SkillCmd::Undeploy { id, ide, project } => undeploy(&id, &ide, project.as_deref()),
        SkillCmd::Sync { id, ides, project } => sync(&id, &ides, project.as_deref()),
    }
}

async fn add(
    source: String,
    name: Option<String>,
    r#ref: Option<String>,
    subdir: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    let parsed = SkillSource::parse_github(&source)
        .ok_or_else(|| anyhow::anyhow!("invalid GitHub source: {source}"))?;
    let SkillSource::GitHub { owner, repo, r#ref: parsed_ref, subdir: parsed_subdir } = parsed
    else {
        anyhow::bail!("only GitHub sources are supported currently");
    };
    let reff = r#ref.or(parsed_ref);
    let subdir = subdir.or(parsed_subdir);
    if !json {
        println!("→ fetching {}/{}{}{}", owner, repo,
            subdir.as_deref().map(|s| format!("//{s}")).unwrap_or_default(),
            reff.as_deref().map(|s| format!("@{s}")).unwrap_or_default());
    }

    let result = github::fetch_github_auto(
        &owner,
        &repo,
        reff.as_deref(),
        subdir.as_deref(),
        name.as_deref(),
    ).await?;

    let mut reg = SkillRegistry::load()?;
    for skill in &result.skills {
        reg.upsert(skill.clone());
    }
    reg.save()?;

    // Auto-register detected MCP servers
    if !result.mcp_servers.is_empty() {
        if let Ok(mut mcp_reg) = aiem_core::mcp::McpRegistry::load() {
            let mcp_count = result.mcp_servers.len();
            for s in &result.mcp_servers {
                mcp_reg.upsert(s.clone());
            }
            if mcp_reg.save().is_ok() {
                if !json {
                    println!("✓ detected and registered {mcp_count} MCP server(s)");
                }
            }
        }
    }

    if json {
        let ids: Vec<&str> = result.skills.iter().map(|s| s.id.as_str()).collect();
        let mcp_names: Vec<&str> = result.mcp_servers.iter().map(|s| s.name.as_str()).collect();
        println!("{}", serde_json::json!({ "added_skills": ids, "added_mcp": mcp_names }));
    } else {
        for skill in &result.skills {
            println!("✓ added  id={}  version={}  path={}", skill.id, short(&skill.version), skill.path.display());
        }
    }
    Ok(())
}

fn list() -> anyhow::Result<()> {
    let reg = SkillRegistry::load()?;
    let mut any = false;
    println!("{:<40} {:<10} {}", "ID", "VERSION", "DEPLOYED TO");
    for s in reg.list() {
        any = true;
        let deployments: Vec<String> = s.deployments.keys().cloned().collect();
        let dep = if deployments.is_empty() { "-".into() } else { deployments.join(",") };
        println!("{:<40} {:<10} {}", s.id, short(&s.version), dep);
    }
    if !any { println!("(no skills installed yet — try `aiem skill add owner/repo`)"); }
    Ok(())
}

fn info(id: &str) -> anyhow::Result<()> {
    let reg = SkillRegistry::load()?;
    let s = reg.get(id).ok_or_else(|| anyhow::anyhow!("skill `{id}` not found"))?;
    println!("{}", serde_json::to_string_pretty(s)?);
    Ok(())
}

async fn update(id: &str) -> anyhow::Result<()> {
    let mut reg = SkillRegistry::load()?;
    let existing = reg.get(id).cloned()
        .ok_or_else(|| anyhow::anyhow!("skill `{id}` not found"))?;
    let SkillSource::GitHub { owner, repo, r#ref, subdir } = existing.source.clone() else {
        anyhow::bail!("skill was not installed from GitHub");
    };
    let updated = github::fetch_github(
        &owner, &repo,
        None,  // always pull latest default branch
        subdir.as_deref(),
        Some(&existing.name),
    ).await?;
    let mut merged = updated;
    merged.deployments = existing.deployments;
    // Clear pinned ref so future updates also pull latest.
    if let SkillSource::GitHub { r#ref: ref mut stored_ref, .. } = merged.source {
        *stored_ref = None;
    }
    reg.upsert(merged.clone());
    reg.save()?;
    println!("✓ updated {}  {} -> {}", id, short(&existing.version), short(&merged.version));
    Ok(())
}

fn remove(id: &str) -> anyhow::Result<()> {
    let mut reg = SkillRegistry::load()?;
    install::remove_skill(&mut reg, id)?;
    reg.save()?;
    println!("✓ removed {id}");
    Ok(())
}

fn deploy(id: &str, ide: &str, project: Option<&std::path::Path>) -> anyhow::Result<()> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg.get(id).cloned()
        .ok_or_else(|| anyhow::anyhow!("skill `{id}` not found"))?;
    let (link, kind) = install::deploy(&mut skill, ide, project)?;
    reg.upsert(skill);
    reg.save()?;
    println!("✓ deployed {id} -> {} ({:?})", link.display(), kind);
    Ok(())
}

fn undeploy(id: &str, ide: &str, project: Option<&std::path::Path>) -> anyhow::Result<()> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg.get(id).cloned()
        .ok_or_else(|| anyhow::anyhow!("skill `{id}` not found"))?;
    let link = install::undeploy(&mut skill, ide, project)?;
    reg.upsert(skill);
    reg.save()?;
    println!("✓ undeployed {id} from {}", link.display());
    Ok(())
}

fn sync(id: &str, ides: &[String], project: Option<&std::path::Path>) -> anyhow::Result<()> {
    let mut reg = SkillRegistry::load()?;
    let mut skill = reg.get(id).cloned()
        .ok_or_else(|| anyhow::anyhow!("skill `{id}` not found"))?;
    for ide in ides {
        match install::deploy(&mut skill, ide, project) {
            Ok((link, kind)) => println!("  ✓ {ide:<14} {} ({:?})", link.display(), kind),
            Err(e) => eprintln!("  ✗ {ide:<14} {e}"),
        }
    }
    reg.upsert(skill);
    reg.save()?;
    Ok(())
}

fn short(v: &str) -> String {
    if v.len() > 10 { v[..10].to_string() } else { v.to_string() }
}
