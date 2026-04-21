//! GitHub-based skill fetching (tarball/zipball download + extract).

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{mcp::model::{McpServer, McpTransport}, paths, Error, Result};

use super::model::{Skill, SkillSource};

/// Compute SHA-256 hashes of every file under `dir`, keyed by relative path (forward slashes).
fn hash_dir(dir: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let walker = walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok());
    for entry in walker {
        if !entry.file_type().is_file() { continue; }
        let rel = match entry.path().strip_prefix(dir) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if let Ok(bytes) = fs::read(entry.path()) {
            let hash = hex::encode(Sha256::digest(&bytes));
            map.insert(rel, hash);
        }
    }
    map
}

/// Result of auto-fetching a GitHub repo — may contain skills and MCP servers.
pub struct FetchResult {
    pub skills: Vec<Skill>,
    pub mcp_servers: Vec<McpServer>,
}

#[derive(Debug, Deserialize)]
struct RepoInfo {
    default_branch: String,
}

#[derive(Debug, Deserialize)]
struct CommitInfo {
    sha: String,
}

fn client() -> Result<reqwest::Client> {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static("aiem"),
    );
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
            h.insert(reqwest::header::AUTHORIZATION, v);
        }
    }
    Ok(reqwest::Client::builder().default_headers(h).build()?)
}

/// GitHub mirror/proxy base URL. Set GITHUB_MIRROR env to override.
/// Common mirrors: https://ghproxy.com, https://mirror.ghproxy.com
fn github_api_base() -> String {
    std::env::var("GITHUB_API_MIRROR")
        .unwrap_or_else(|_| "https://api.github.com".to_string())
        .trim_end_matches('/').to_string()
}

/// Build the zip download URL for a GitHub repo ref.
/// Format: https://github.com/{owner}/{repo}/archive/{ref}.zip
/// If GITHUB_MIRROR is set, prefix it: {mirror}/https://github.com/...
/// Default mirror: https://gh-proxy.org (avoids codeload 404 on tags and works in CN).
fn build_zip_url(owner: &str, repo: &str, r#ref: &str) -> String {
    let gh_url = format!("https://github.com/{owner}/{repo}/archive/{ref}.zip");
    let mirror = std::env::var("GITHUB_MIRROR")
        .unwrap_or_else(|_| "https://gh-proxy.org".to_string());
    let mirror = mirror.trim_end_matches('/');
    if mirror.is_empty() {
        gh_url
    } else {
        format!("{mirror}/{gh_url}")
    }
}

async fn resolve_ref(client: &reqwest::Client, owner: &str, repo: &str, r#ref: Option<&str>) -> Result<(String, String)> {
    let api = github_api_base();
    let r = match r#ref {
        Some(r) => r.to_string(),
        None => {
            // Only call GitHub API when a token is available; otherwise we hit the
            // 60-req/hr anonymous rate limit immediately and always get 403.
            let has_token = std::env::var("GITHUB_TOKEN").map(|t| !t.is_empty()).unwrap_or(false);
            if has_token {
                let url = format!("{api}/repos/{owner}/{repo}");
                let resp = client.get(&url).send().await?;
                if resp.status().is_success() {
                    let info: RepoInfo = resp.json().await?;
                    info.default_branch
                } else {
                    tracing::warn!(status = %resp.status(), "GitHub API returned error despite token, defaulting to 'main'");
                    "main".to_string()
                }
            } else {
                tracing::debug!("No GITHUB_TOKEN set, skipping API call and defaulting to 'main'");
                "main".to_string()
            }
        }
    };
    // Resolve commit sha for the ref (so version is deterministic).
    let url = format!("{api}/repos/{owner}/{repo}/commits/{r}");
    let resp = client.get(&url).send().await?;
    if resp.status().is_success() {
        let c: CommitInfo = resp.json().await?;
        Ok((r, c.sha))
    } else {
        // Fallback: just use the ref literally as version (e.g. offline / rate-limited).
        Ok((r.clone(), r))
    }
}

/// Install a skill from GitHub. If no subdir is given and the repo root doesn't
/// look like a skill but has skill-like subdirectories, installs each subdir as
/// a separate skill.
pub async fn fetch_github_auto(
    owner: &str,
    repo: &str,
    r#ref: Option<&str>,
    subdir: Option<&str>,
    name: Option<&str>,
) -> Result<FetchResult> {
    // If user specified a subdir, just fetch that single skill
    if subdir.is_some() {
        return Ok(FetchResult {
            skills: vec![fetch_github(owner, repo, r#ref, subdir, name).await?],
            mcp_servers: vec![],
        });
    }

    // Download and inspect
    paths::ensure_layout()?;
    let client = client()?;
    let (resolved_ref, _sha) = resolve_ref(&client, owner, repo, r#ref).await?;
    let zip_url = build_zip_url(owner, repo, &resolved_ref);
    let bytes = client.get(&zip_url).send().await?.error_for_status()?.bytes().await?;
    let staging = tempfile::tempdir()?;
    extract_zip(bytes.as_ref(), staging.path())?;
    let top = find_single_top_dir(staging.path())?;

    // Detect MCP servers in the repo
    let mcp_servers = detect_mcp_servers(&top);

    // Check if root is a skill itself (has SKILL.md)
    if is_skill_dir(&top) {
        drop(staging);
        return Ok(FetchResult {
            skills: vec![fetch_github(owner, repo, r#ref, None, name).await?],
            mcp_servers,
        });
    }

    // Detect skill subdirs using multiple strategies
    let subdirs = collect_subdirs_in_tree(&top);

    if subdirs.is_empty() {
        // Fall back to treating whole repo as one skill
        drop(staging);
        return Ok(FetchResult {
            skills: vec![fetch_github(owner, repo, r#ref, None, name).await?],
            mcp_servers,
        });
    }

    // Install each subdir as a separate skill
    drop(staging);
    let mut skills = Vec::new();
    for sd in &subdirs {
        match fetch_github(owner, repo, Some(&resolved_ref), Some(sd), None).await {
            Ok(s) => skills.push(s),
            Err(e) => tracing::warn!("skip subdir {sd}: {e}"),
        }
    }
    if skills.is_empty() && mcp_servers.is_empty() {
        return Err(Error::Invalid("no valid skill subdirs or MCP servers found".into()));
    }
    Ok(FetchResult { skills, mcp_servers })
}

/// Extract all skill subdir paths from an already-extracted repo tree.
/// Returns paths relative to `top` (forward slashes).
pub fn collect_subdirs_in_tree(top: &Path) -> Vec<String> {
    let mut subdirs = Vec::new();

    // Strategy 1: agent skill dirs
    let agent_skill_dirs = [
        ".claude/skills", ".cursor/skills", ".windsurf/skills",
        ".copilot/skills", ".kiro/skills", ".trae/skills",
        ".continue/skills", ".factory/skills",
    ];
    for asd in &agent_skill_dirs {
        let container = top.join(asd);
        if container.is_dir() {
            if let Ok(entries) = fs::read_dir(&container) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        let n = entry.file_name().to_string_lossy().to_string();
                        if !n.starts_with('.') { subdirs.push(format!("{asd}/{n}")); }
                    }
                }
            }
            if is_skill_dir(&container) && subdirs.is_empty() {
                subdirs.push(asd.to_string());
            }
        }
    }

    // Strategy 2: top-level skills/ dir (ARIS-style)
    let skills_container = top.join("skills");
    if skills_container.is_dir() {
        for sd in detect_skill_dirs(&skills_container) {
            subdirs.push(format!("skills/{sd}"));
        }
    }

    // Strategy 3: top-level subdirs with SKILL.md
    subdirs.extend(detect_skill_dirs(top));
    subdirs.sort();
    subdirs.dedup();

    // Strategy 4: deep recursive search
    if subdirs.is_empty() {
        find_deep_skill_containers(top, top, 0, 5, &mut subdirs);
        subdirs.sort();
        subdirs.dedup();
    }

    subdirs
}

/// Result of a group sync operation.
pub struct SyncResult {
    pub updated: Vec<Skill>,
    pub added: Vec<Skill>,
}

/// Sync all skills from a GitHub repo against `existing_skills` (the currently
/// installed skills that belong to this owner/repo group).
///
/// - Skills already installed → smart-merge updated in place.
/// - Skills present in the new version but not yet installed → freshly installed.
///
/// Downloads the repo zip **once** regardless of how many skills there are.
pub async fn sync_github_group(
    owner: &str,
    repo: &str,
    existing_skills: &[super::model::Skill],
) -> Result<SyncResult> {
    use std::collections::HashMap;
    use sha2::{Digest, Sha256};

    paths::ensure_layout()?;
    let client = client()?;
    let (resolved_ref, sha) = resolve_ref(&client, owner, repo, None).await?;

    let zip_url = build_zip_url(owner, repo, &resolved_ref);
    tracing::info!(url = %zip_url, "sync: downloading repo zipball");
    let bytes = client.get(&zip_url).send().await?.error_for_status()?.bytes().await?;

    let staging = tempfile::tempdir()?;
    extract_zip(bytes.as_ref(), staging.path())?;
    let top = find_single_top_dir(staging.path())?;

    // If the whole repo is a single skill, fall back to regular update
    if is_skill_dir(&top) {
        // Treat as single skill; update the first (or only) existing one
        drop(staging);
        let skill = fetch_github(owner, repo, Some(&resolved_ref), None, None).await?;
        if existing_skills.iter().any(|e| e.id == skill.id) {
            return Ok(SyncResult { updated: vec![skill], added: vec![] });
        } else {
            return Ok(SyncResult { updated: vec![], added: vec![skill] });
        }
    }

    let upstream_subdirs = collect_subdirs_in_tree(&top);

    // Build a lookup: leaf_name → existing Skill (uses current stored subdir)
    // Also build a lookup by canonical_id for exact matches
    let existing_by_id: HashMap<String, &super::model::Skill> =
        existing_skills.iter().map(|s| (s.id.clone(), s)).collect();

    let mut updated = Vec::new();
    let mut added = Vec::new();

    for upstream_subdir in &upstream_subdirs {
        let source_dir = top.join(upstream_subdir.replace('\\', "/"));
        if !source_dir.is_dir() { continue; }

        // Compute what the canonical id would be for this subdir
        let candidate_source = SkillSource::GitHub {
            owner: owner.to_string(),
            repo: repo.to_string(),
            r#ref: Some(resolved_ref.clone()),
            subdir: Some(upstream_subdir.clone()),
        };
        let candidate_id = candidate_source.canonical_id();

        if let Some(existing) = existing_by_id.get(&candidate_id) {
            // Already installed — smart-merge update
            let target = &existing.path;
            if !target.is_dir() {
                fs::create_dir_all(target)?;
            }
            // Smart merge: overwrite files unchanged since last install, skip user-modified
            let walker = walkdir::WalkDir::new(&source_dir).into_iter().filter_map(|e| e.ok());
            for entry in walker {
                let rel = match entry.path().strip_prefix(&source_dir) {
                    Ok(r) => r.to_string_lossy().replace('\\', "/"),
                    Err(_) => continue,
                };
                let dst = target.join(&rel);
                if entry.file_type().is_dir() {
                    let _ = fs::create_dir_all(&dst);
                    continue;
                }
                let should_overwrite = if dst.exists() {
                    let current = fs::read(&dst).unwrap_or_default();
                    let current_hash = hex::encode(Sha256::digest(&current));
                    match existing.file_hashes.get(&rel) {
                        Some(orig) => &current_hash == orig, // unchanged since install
                        None => false, // no hash record → be conservative, don't overwrite
                    }
                } else {
                    true // new file
                };
                if should_overwrite {
                    let _ = fs::copy(entry.path(), &dst);
                }
            }
            // Build updated skill record
            let mut skill = (*existing).clone();
            skill.version = sha.clone();
            skill.file_hashes = hash_dir(target);
            if let SkillSource::GitHub { r#ref: ref mut r, subdir: ref mut sd, .. } = skill.source {
                *r = None;
                *sd = Some(upstream_subdir.clone());
            }
            let mut reg = super::registry::SkillRegistry::load()?;
            reg.upsert(skill.clone());
            reg.save()?;
            updated.push(skill);
        } else {
            // New skill not yet installed
            match fetch_github(owner, repo, Some(&resolved_ref), Some(upstream_subdir), None).await {
                Ok(skill) => {
                    let mut reg = super::registry::SkillRegistry::load()?;
                    reg.upsert(skill.clone());
                    reg.save()?;
                    added.push(skill);
                }
                Err(e) => tracing::warn!("sync: skip new subdir {upstream_subdir}: {e}"),
            }
        }
    }

    Ok(SyncResult { updated, added })
}

/// Install or update a skill from GitHub into `~/.aiem/skills/<id>/`.
pub async fn fetch_github(
    owner: &str,
    repo: &str,
    r#ref: Option<&str>,
    subdir: Option<&str>,
    name: Option<&str>,
) -> Result<Skill> {
    paths::ensure_layout()?;
    let client = client()?;
    let (resolved_ref, sha) = resolve_ref(&client, owner, repo, r#ref).await?;

    // Download zipball.
    let zip_url = build_zip_url(owner, repo, &resolved_ref);
    tracing::info!(url = %zip_url, "downloading repo zipball");
    let bytes = client.get(&zip_url).send().await?.error_for_status()?.bytes().await?;

    // Extract to a staging dir.
    let staging = tempfile::tempdir()?;
    extract_zip(bytes.as_ref(), staging.path())?;

    // GitHub zipballs contain a single top-level dir `<repo>-<sha_or_ref>`.
    let top = find_single_top_dir(staging.path())?;
    let source_dir = match subdir {
        Some(sd) => {
            let candidate = top.join(sd.replace('\\', "/"));
            if candidate.is_dir() {
                candidate
            } else {
                // Subdir path may have changed in a new version of the repo.
                // Fall back: search for a directory with the same basename.
                let basename = std::path::Path::new(sd)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let found = find_dir_by_name(&top, &basename);
                match found {
                    Some(p) => {
                        tracing::info!(old = %sd, new = %p.strip_prefix(&top).unwrap_or(&p).display(), "subdir moved — using new location");
                        p
                    }
                    None => return Err(Error::Invalid(format!("subdir not found in repo: {candidate:?}"))),
                }
            }
        }
        None => top.clone(),
    };
    if !source_dir.is_dir() {
        return Err(Error::Invalid(format!("source dir missing: {source_dir:?}")));
    }

    let source = SkillSource::GitHub {
        owner: owner.to_string(),
        repo: repo.to_string(),
        r#ref: Some(resolved_ref.clone()),
        // Use the actual (possibly relocated) subdir, relative to repo root.
        subdir: if source_dir == top {
            None
        } else {
            source_dir.strip_prefix(&top).ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        },
    };
    let id = source.canonical_id();
    let target = paths::skills_dir()?.join(&id);

    // Replace existing content atomically-ish.
    if target.exists() {
        crate::fs_util::remove_path(&target)?;
    }
    fs::create_dir_all(&target)?;
    crate::fs_util::copy_dir_recursive(&source_dir, &target)?;

    let file_hashes = hash_dir(&target);
    let skill = Skill {
        id: id.clone(),
        name: name.map(|s| s.to_string()).unwrap_or_else(|| default_name(repo, subdir)),
        source,
        version: sha,
        path: target,
        description: read_description(&source_dir),
        installed_at: Some(Utc::now().to_rfc3339()),
        deployments: Default::default(),
        file_hashes,
    };
    Ok(skill)
}

/// Detect skill-like subdirectories in a directory.
/// A directory is considered a skill if it contains SKILL.md (the convention for
/// Claude Code / agent skills). Falls back to checking for any .md files only
/// if no SKILL.md-based skills are found.
pub fn detect_skill_dirs(root: &Path) -> Vec<String> {
    let skip = [".github", ".git", "node_modules", ".vscode", "target", "dist", "build",
        "__pycache__", "docs", "assets", "templates", "tools", "mcp-servers",
        "shared-references"];
    let mut found = Vec::new();
    let mut fallback = Vec::new();
    let Ok(entries) = fs::read_dir(root) else { return found };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() { continue; }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || skip.contains(&name.as_str()) { continue; }
        let dir = entry.path();
        // Primary: look for SKILL.md
        if dir.join("SKILL.md").is_file() || dir.join("skill.md").is_file() {
            found.push(name.clone());
        } else {
            // Secondary fallback: has any .md files
            let has_md = fs::read_dir(&dir)
                .ok()
                .map(|rd| rd.flatten().any(|e| {
                    e.path().extension().map(|x| x == "md").unwrap_or(false)
                }))
                .unwrap_or(false);
            if has_md {
                fallback.push(name);
            }
        }
    }
    // Only use fallback if we found zero SKILL.md dirs
    if found.is_empty() { found = fallback; }
    found
}

/// Check if the root itself looks like a skill (has SKILL.md).
pub fn is_skill_dir(dir: &Path) -> bool {
    dir.join("SKILL.md").is_file() || dir.join("skill.md").is_file()
}

/// Detect MCP server definitions in a repo.
/// Looks for .mcp.json, mcp.json, mcp-servers/ directory with server code,
/// or any JSON with mcpServers config.
pub fn detect_mcp_servers(root: &Path) -> Vec<McpServer> {
    let mut servers = Vec::new();

    // Check common MCP config files
    let candidates = [".mcp.json", "mcp.json", ".mcp/config.json"];
    for name in &candidates {
        let path = root.join(name);
        if path.is_file() {
            if let Ok(content) = fs::read_to_string(&path) {
                servers.extend(parse_mcp_json(&content));
            }
        }
    }

    // Check for mcp-servers/ directory (common pattern: each subdir is a server)
    let mcp_dir = root.join("mcp-servers");
    if mcp_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&mcp_dir) {
            for entry in entries.flatten() {
                let Ok(ft) = entry.file_type() else { continue };
                if !ft.is_dir() { continue; }
                let name = entry.file_name().to_string_lossy().to_string();
                let dir = entry.path();
                // Try to find a README or package.json to get description
                let desc = read_description(&dir).map(|d| d.lines().next().unwrap_or("").to_string());
                // Detect if it's a Python or Node server
                let (command, args) = if dir.join("server.py").is_file() || dir.join("main.py").is_file() {
                    let entry_file = if dir.join("server.py").is_file() { "server.py" } else { "main.py" };
                    ("python".to_string(), vec![entry_file.to_string()])
                } else if dir.join("index.js").is_file() || dir.join("index.ts").is_file() {
                    let entry_file = if dir.join("index.js").is_file() { "index.js" } else { "index.ts" };
                    ("node".to_string(), vec![entry_file.to_string()])
                } else if dir.join("package.json").is_file() {
                    ("npx".to_string(), vec![".".to_string()])
                } else {
                    continue; // Can't determine how to run this server
                };
                servers.push(McpServer {
                    name: name.clone(),
                    transport: McpTransport::Stdio {
                        command,
                        args,
                        env: Default::default(),
                        cwd: Some(format!("mcp-servers/{name}")),
                    },
                    targets: vec!["codex".into(), "claude-code".into(), "copilot".into()],
                    description: desc,
                    tags: vec!["auto-detected".into()],
                    disabled: false,
                });
            }
        }
    }

    // Also scan for any JSON file in root that might contain MCP server defs
    if servers.is_empty() {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if name.contains("mcp") || name.contains("server") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            servers.extend(parse_mcp_json(&content));
                        }
                    }
                }
            }
        }
    }

    servers
}

/// Parse MCP servers from JSON content. Supports:
/// - `{ "mcpServers": { "name": { "command": ... } } }` (Claude format)
/// - `{ "name": { "command": ... } }` (direct map)
fn parse_mcp_json(content: &str) -> Vec<McpServer> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(content) else { return vec![] };
    let Some(obj) = val.as_object() else { return vec![] };

    // Try mcpServers wrapper
    let server_map = if let Some(inner) = obj.get("mcpServers").and_then(|v| v.as_object()) {
        inner.clone()
    } else {
        // Check if top-level entries look like server configs
        obj.clone()
    };

    let mut servers = Vec::new();
    for (name, config) in &server_map {
        let Some(cfg) = config.as_object() else { continue };
        let transport = if let Some(cmd) = cfg.get("command").and_then(|v| v.as_str()) {
            let args: Vec<String> = cfg.get("args")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let env: std::collections::BTreeMap<String, String> = cfg.get("env")
                .and_then(|v| v.as_object())
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
                .unwrap_or_default();
            let cwd = cfg.get("cwd").and_then(|v| v.as_str()).map(String::from);
            McpTransport::Stdio { command: cmd.to_string(), args, env, cwd }
        } else if let Some(url) = cfg.get("url").and_then(|v| v.as_str()) {
            let headers: std::collections::BTreeMap<String, String> = cfg.get("headers")
                .and_then(|v| v.as_object())
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
                .unwrap_or_default();
            McpTransport::Sse { url: url.to_string(), headers }
        } else {
            continue; // Not a recognizable server config
        };

        servers.push(McpServer {
            name: name.clone(),
            transport,
            targets: vec!["codex".into(), "claude-code".into(), "copilot".into()],
            description: cfg.get("description").and_then(|v| v.as_str()).map(String::from),
            tags: vec![],
            disabled: false,
        });
    }
    servers
}

/// Fetch from GitHub into a temp directory (for smart merge / diff-based update).
/// Returns (temp_dir, version_sha, actual_subdir_used).
/// `actual_subdir_used` may differ from `subdir` when the directory was moved in the repo.
pub async fn fetch_github_to_temp(
    owner: &str,
    repo: &str,
    r#ref: Option<&str>,
    subdir: Option<&str>,
) -> Result<(tempfile::TempDir, String, Option<String>)> {
    let client = client()?;
    let (resolved_ref, sha) = resolve_ref(&client, owner, repo, r#ref).await?;

    let zip_url = build_zip_url(owner, repo, &resolved_ref);
    let bytes = client.get(&zip_url).send().await?.error_for_status()?.bytes().await?;

    let staging = tempfile::tempdir()?;
    extract_zip(bytes.as_ref(), staging.path())?;

    let top = find_single_top_dir(staging.path())?;
    let (source_dir, actual_subdir) = match subdir {
        Some(sd) => {
            let candidate = top.join(sd.replace('\\', "/"));
            if candidate.is_dir() {
                (candidate, Some(sd.to_string()))
            } else {
                // Subdir may have moved — try BFS by the leaf directory name.
                let leaf = sd.replace('\\', "/");
                let leaf = leaf.rsplit('/').next().unwrap_or(sd);
                match find_dir_by_name(&top, &leaf.to_lowercase()) {
                    Some(found) => {
                        let new_sub = found.strip_prefix(&top)
                            .ok()
                            .map(|p| p.to_string_lossy().replace('\\', "/"));
                        tracing::info!(old = sd, new = ?new_sub, "subdir moved — using new location");
                        (found, new_sub)
                    }
                    None => return Err(Error::Invalid(format!("subdir not found: {candidate:?}"))),
                }
            }
        }
        None => (top.clone(), None),
    };

    // Copy to a clean temp dir so caller gets just the relevant files
    let out = tempfile::tempdir()?;
    crate::fs_util::copy_dir_recursive(&source_dir, out.path())?;

    // Drop the staging dir (extracted zip)
    drop(staging);

    Ok((out, sha, actual_subdir))
}

fn default_name(repo: &str, subdir: Option<&str>) -> String {
    match subdir {
        Some(s) if !s.is_empty() => {
            let tail = s.rsplit('/').next().unwrap_or(s);
            tail.to_string()
        }
        _ => repo.to_string(),
    }
}

fn read_description(dir: &Path) -> Option<String> {
    for f in ["SKILL.md", "skill.md", "README.md", "readme.md"] {
        let p = dir.join(f);
        if p.is_file() {
            if let Ok(s) = fs::read_to_string(&p) {
                return Some(s.lines().take(5).collect::<Vec<_>>().join("\n"));
            }
        }
    }
    None
}

fn extract_zip(bytes: &[u8], into: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name().map(PathBuf::from) else { continue };
        let out = into.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() { fs::create_dir_all(p)?; }
            let mut f = fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut f)?;
        }
    }
    Ok(())
}

/// BFS-search for the first directory with the given lowercase name under `root`.
/// Returns the full path if found.
fn find_dir_by_name(root: &Path, name: &str) -> Option<PathBuf> {
    let skip = [".github", ".git", "node_modules", "target", "dist", "build", "__pycache__"];
    // Queue entries: (path, depth)
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> = std::collections::VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));
    while let Some((dir, depth)) = queue.pop_front() {
        if depth > 6 { continue; }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() { continue; }
            let n = entry.file_name().to_string_lossy().to_lowercase();
            if skip.contains(&n.as_str()) { continue; }
            if n == name {
                return Some(entry.path());
            }
            queue.push_back((entry.path(), depth + 1));
        }
    }
    None
}

fn find_single_top_dir(root: &Path) -> Result<PathBuf> {    let mut iter = fs::read_dir(root)?;
    let first = iter
        .next()
        .ok_or_else(|| Error::Invalid("empty zip archive".into()))??;
    if iter.next().is_some() {
        // Multiple entries: treat root itself as top.
        return Ok(root.to_path_buf());
    }
    if first.file_type()?.is_dir() {
        Ok(first.path())
    } else {
        Ok(root.to_path_buf())
    }
}

/// Recursively walk the repo tree searching for directories named "skills" that
/// contain skill-like subdirs (SKILL.md). Found skills are appended to `out` as
/// paths relative to `repo_root` using `/` separators.
fn find_deep_skill_containers(dir: &Path, repo_root: &Path, depth: usize, max_depth: usize, out: &mut Vec<String>) {
    if depth >= max_depth { return; }
    let skip = [".github", ".git", "node_modules", ".vscode", "target", "dist", "build",
                "__pycache__", "docs"];
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() { continue; }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || skip.contains(&name.as_str()) { continue; }
        let path = entry.path();
        if name == "skills" {
            // Found a 'skills' dir — collect immediate skill subdirs
            for sd in detect_skill_dirs(&path) {
                if let Ok(rel) = path.strip_prefix(repo_root) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    out.push(format!("{rel_str}/{sd}"));
                }
            }
        } else {
            // Recurse into non-skills subdirs
            find_deep_skill_containers(&path, repo_root, depth + 1, max_depth, out);
        }
    }
}
