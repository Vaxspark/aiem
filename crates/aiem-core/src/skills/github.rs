//! GitHub-based skill fetching (tarball/zipball download + extract).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    mcp::model::{McpServer, McpTransport},
    paths, Error, Result,
};

use super::model::{Skill, SkillSource};

/// Compute SHA-256 hashes of every file under `dir`, keyed by relative path (forward slashes).
fn hash_dir(dir: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let walker = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in walker {
        if !entry.file_type().is_file() {
            continue;
        }
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
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(crate::backup::load_backup_token_file);
    if let Some(token) = token {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
            h.insert(reqwest::header::AUTHORIZATION, v);
        }
    }
    Ok(reqwest::Client::builder()
        .default_headers(h)
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(120))
        .build()?)
}

/// GitHub mirror/proxy base URL. Set GITHUB_MIRROR env to override.
/// Common mirrors: https://ghproxy.com, https://mirror.ghproxy.com
fn github_api_base() -> String {
    std::env::var("GITHUB_API_MIRROR")
        .unwrap_or_else(|_| "https://api.github.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Build archive download URLs for a GitHub repo ref.
///
/// `GITHUB_MIRROR` may contain one or more comma/semicolon-separated mirror
/// bases. We still include direct GitHub and built-in mirrors as fallback so a
/// token-enabled install does not get stuck on a single unreachable endpoint.
fn build_zip_urls(owner: &str, repo: &str, r#ref: &str) -> Vec<String> {
    let codeload_url = format!("https://codeload.github.com/{owner}/{repo}/zip/{ref}");
    let gh_url = format!("https://github.com/{owner}/{repo}/archive/{ref}.zip");
    let mut urls = Vec::new();

    // Prefer GitHub's official zipball endpoint. It avoids an extra redirect
    // and is less likely to hit body decoding issues than /archive/<ref>.zip.
    urls.push(codeload_url.clone());

    if let Ok(configured) = std::env::var("GITHUB_MIRROR") {
        for mirror in configured.split([',', ';']) {
            let mirror = mirror.trim().trim_end_matches('/');
            if !mirror.is_empty() {
                if mirror.contains("codeload.github.com") {
                    urls.push(format!("{mirror}/{owner}/{repo}/zip/{ref}"));
                } else {
                    urls.push(format!("{mirror}/{codeload_url}"));
                    urls.push(format!("{mirror}/{gh_url}"));
                }
            }
        }
    }

    urls.push(gh_url.clone());
    for mirror in [
        "https://gh-proxy.org",
        "https://mirror.ghproxy.com",
        "https://ghproxy.com",
    ] {
        urls.push(format!("{mirror}/{codeload_url}"));
        urls.push(format!("{mirror}/{gh_url}"));
    }

    let mut seen = BTreeSet::new();
    urls.into_iter()
        .filter(|url| seen.insert(url.clone()))
        .collect()
}

async fn download_zip_with_fallback(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    r#ref: &str,
    context: &str,
) -> Result<Vec<u8>> {
    let urls = build_zip_urls(owner, repo, r#ref);
    let mut last_error = None;
    for url in &urls {
        tracing::info!(url = %url, "{context}");
        match client.get(url).send().await {
            Ok(resp) => match resp.error_for_status() {
                Ok(resp) => match resp.bytes().await {
                    Ok(bytes) => {
                        let bytes = bytes.to_vec();
                        match validate_zip_archive(&bytes) {
                            Ok(()) => return Ok(bytes),
                            Err(e) => last_error = Some(format!("{url}: {e}")),
                        }
                    }
                    Err(e) => last_error = Some(format!("{url}: {e}")),
                },
                Err(e) => last_error = Some(format!("{url}: {e}")),
            },
            Err(e) => last_error = Some(format!("{url}: {e}")),
        }
        tracing::warn!(url = %url, error = ?last_error, "GitHub archive download failed; trying fallback");
    }

    Err(Error::Invalid(format!(
        "failed to download GitHub archive after {} attempt(s); last error: {}; tried: {}",
        urls.len(),
        last_error.unwrap_or_else(|| "unknown".to_string()),
        urls.join(", ")
    )))
}

fn validate_zip_archive(bytes: &[u8]) -> std::result::Result<(), String> {
    zip::ZipArchive::new(Cursor::new(bytes))
        .map(|_| ())
        .map_err(|e| format!("invalid zip archive: {e}"))
}

async fn resolve_ref(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    r#ref: Option<&str>,
) -> Result<(String, String)> {
    let api = github_api_base();
    let r = match r#ref {
        Some(r) => r.to_string(),
        None => {
            // Only call GitHub API when a token is available; otherwise we hit the
            // 60-req/hr anonymous rate limit immediately and always get 403.
            let has_token = std::env::var("GITHUB_TOKEN")
                .map(|t| !t.is_empty())
                .unwrap_or(false);
            if has_token {
                let url = format!("{api}/repos/{owner}/{repo}");
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        let info: RepoInfo = resp.json().await?;
                        info.default_branch
                    }
                    Ok(resp) => {
                        tracing::warn!(status = %resp.status(), "GitHub API returned error despite token, defaulting to 'main'");
                        "main".to_string()
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "GitHub API default-branch lookup failed, defaulting to 'main'");
                        "main".to_string()
                    }
                }
            } else {
                tracing::debug!("No GITHUB_TOKEN set, skipping API call and defaulting to 'main'");
                "main".to_string()
            }
        }
    };
    // Resolve commit sha for the ref (so version is deterministic).
    let url = format!("{api}/repos/{owner}/{repo}/commits/{r}");
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let c: CommitInfo = resp.json().await?;
            Ok((r, c.sha))
        }
        Ok(resp) => {
            tracing::warn!(status = %resp.status(), ref_name = %r, "GitHub commit lookup failed; using ref literally");
            Ok((r.clone(), r))
        }
        Err(e) => {
            tracing::warn!(error = %e, ref_name = %r, "GitHub commit lookup failed; using ref literally");
            Ok((r.clone(), r))
        }
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
    let bytes = download_zip_with_fallback(
        &client,
        owner,
        repo,
        &resolved_ref,
        "downloading repo zipball",
    )
    .await?;
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
        if mcp_servers.is_empty() {
            return Err(Error::Invalid(
                "no valid skills found; expected a directory containing SKILL.md".into(),
            ));
        }
        return Ok(FetchResult {
            skills: vec![],
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
        return Err(Error::Invalid(
            "no valid skill subdirs or MCP servers found".into(),
        ));
    }
    Ok(FetchResult {
        skills,
        mcp_servers,
    })
}

/// Extract all skill subdir paths from an already-extracted repo tree.
/// Returns paths relative to `top` (forward slashes).
pub fn collect_subdirs_in_tree(top: &Path) -> Vec<String> {
    let mut subdirs = Vec::new();

    // Strategy 1: agent skill dirs
    let agent_skill_dirs = [
        ".claude/skills",
        ".cursor/skills",
        ".windsurf/skills",
        ".copilot/skills",
        ".kiro/skills",
        ".trae/skills",
        ".continue/skills",
        ".factory/skills",
    ];
    for asd in &agent_skill_dirs {
        let container = top.join(asd);
        if container.is_dir() {
            if let Ok(entries) = fs::read_dir(&container) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        let n = entry.file_name().to_string_lossy().to_string();
                        if !n.starts_with('.') {
                            subdirs.push(format!("{asd}/{n}"));
                        }
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
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;

    paths::ensure_layout()?;
    let client = client()?;
    let (resolved_ref, sha) = resolve_ref(&client, owner, repo, None).await?;

    let bytes = download_zip_with_fallback(
        &client,
        owner,
        repo,
        &resolved_ref,
        "sync: downloading repo zipball",
    )
    .await?;

    let staging = tempfile::tempdir()?;
    extract_zip(bytes.as_ref(), staging.path())?;
    let top = find_single_top_dir(staging.path())?;

    // If the whole repo is a single skill, fall back to regular update
    if is_skill_dir(&top) {
        // Treat as single skill; update the first (or only) existing one
        drop(staging);
        let skill = fetch_github(owner, repo, Some(&resolved_ref), None, None).await?;
        if existing_skills.iter().any(|e| e.id == skill.id) {
            return Ok(SyncResult {
                updated: vec![skill],
                added: vec![],
            });
        } else {
            return Ok(SyncResult {
                updated: vec![],
                added: vec![skill],
            });
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
        if !source_dir.is_dir() {
            continue;
        }

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
            let filtered = tempfile::tempdir()?;
            copy_skill_filtered(&source_dir, filtered.path())?;
            ensure_canonical_skill_manifest(filtered.path())?;
            // Smart merge: overwrite files unchanged since last install, skip user-modified
            let walker = walkdir::WalkDir::new(filtered.path())
                .into_iter()
                .filter_map(|e| e.ok());
            for entry in walker {
                let rel = match entry.path().strip_prefix(filtered.path()) {
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
            let new_hashes = hash_dir(filtered.path());
            cleanup_noise(target, &existing.file_hashes, &new_hashes);
            ensure_canonical_skill_manifest(target)?;
            // Build updated skill record
            let mut skill = (*existing).clone();
            skill.version = sha.clone();
            skill.file_hashes = hash_dir(target);
            if let SkillSource::GitHub {
                r#ref: ref mut r,
                subdir: ref mut sd,
                ..
            } = skill.source
            {
                *r = None;
                *sd = Some(upstream_subdir.clone());
            }
            let mut reg = super::registry::SkillRegistry::load()?;
            reg.upsert(skill.clone());
            reg.save()?;
            updated.push(skill);
        } else {
            // New skill not yet installed
            match fetch_github(
                owner,
                repo,
                Some(&resolved_ref),
                Some(upstream_subdir),
                None,
            )
            .await
            {
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
    let bytes = download_zip_with_fallback(
        &client,
        owner,
        repo,
        &resolved_ref,
        "downloading repo zipball",
    )
    .await?;

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
                    None => {
                        return Err(Error::Invalid(format!(
                            "subdir not found in repo: {candidate:?}"
                        )))
                    }
                }
            }
        }
        None => top.clone(),
    };
    if !source_dir.is_dir() {
        return Err(Error::Invalid(format!(
            "source dir missing: {source_dir:?}"
        )));
    }
    if !is_skill_dir(&source_dir) {
        return Err(Error::Invalid(format!(
            "invalid skill format at {}: missing SKILL.md",
            source_dir.display()
        )));
    }

    let source = SkillSource::GitHub {
        owner: owner.to_string(),
        repo: repo.to_string(),
        r#ref: Some(resolved_ref.clone()),
        // Use the actual (possibly relocated) subdir, relative to repo root.
        subdir: if source_dir == top {
            None
        } else {
            source_dir
                .strip_prefix(&top)
                .ok()
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
    copy_skill_filtered(&source_dir, &target)?;
    ensure_canonical_skill_manifest(&target)?;

    let file_hashes = hash_dir(&target);
    let skill = Skill {
        id: id.clone(),
        name: name
            .map(|s| s.to_string())
            .unwrap_or_else(|| default_name(repo, subdir)),
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
/// A directory is considered a skill only when it contains `SKILL.md`
/// (or legacy lowercase `skill.md`, which is normalized on install).
pub fn detect_skill_dirs(root: &Path) -> Vec<String> {
    let skip = [
        ".github",
        ".git",
        "node_modules",
        ".vscode",
        "target",
        "dist",
        "build",
        "__pycache__",
        "docs",
        "assets",
        "templates",
        "tools",
        "mcp-servers",
        "shared-references",
    ];
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || skip.contains(&name.as_str()) {
            continue;
        }
        let dir = entry.path();
        if dir.join("SKILL.md").is_file() || dir.join("skill.md").is_file() {
            found.push(name.clone());
        }
    }
    found
}

/// Check if the root itself looks like a skill (has SKILL.md).
pub fn is_skill_dir(dir: &Path) -> bool {
    dir.join("SKILL.md").is_file() || dir.join("skill.md").is_file()
}

/// Ensure the canonical uppercase manifest exists.
///
/// Some older repos use `skill.md`; that can work on case-insensitive
/// workstations but fails on Linux hosts where skill loaders look for
/// `SKILL.md` exactly.
pub fn ensure_canonical_skill_manifest(dir: &Path) -> Result<()> {
    let canonical = dir.join("SKILL.md");
    if canonical.is_file() {
        return Ok(());
    }
    let legacy = dir.join("skill.md");
    if legacy.is_file() {
        fs::copy(&legacy, &canonical)?;
        return Ok(());
    }
    Err(Error::Invalid(format!(
        "invalid skill format at {}: missing SKILL.md",
        dir.display()
    )))
}

/// Detect MCP server definitions in a repo.
/// Looks for .mcp.json, mcp.json, mcp-servers/ directory with server code,
/// or any JSON with mcpServers config.
pub fn detect_mcp_servers(root: &Path) -> Vec<McpServer> {
    let mut servers = Vec::new();

    // Check common MCP config files
    let candidates = [
        ".mcp.json",
        "mcp.json",
        "mcp-config.json",
        "mcp_config.json",
        ".mcp/config.json",
    ];
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
                if !ft.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let dir = entry.path();
                // Try to find a README or package.json to get description
                let desc =
                    read_description(&dir).map(|d| d.lines().next().unwrap_or("").to_string());
                // Detect if it's a Python or Node server
                let (command, args) =
                    if dir.join("server.py").is_file() || dir.join("main.py").is_file() {
                        let entry_file = if dir.join("server.py").is_file() {
                            "server.py"
                        } else {
                            "main.py"
                        };
                        ("python".to_string(), vec![entry_file.to_string()])
                    } else if dir.join("index.js").is_file() || dir.join("index.ts").is_file() {
                        let entry_file = if dir.join("index.js").is_file() {
                            "index.js"
                        } else {
                            "index.ts"
                        };
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
                        bundle: None,
                    },
                    targets: crate::mcp::adapters::SUPPORTED
                        .iter()
                        .map(|ide| ide.to_string())
                        .collect(),
                    description: desc,
                    tags: vec!["auto-detected".into()],
                    disabled: false,
                    source: None,
                    runtime: None,
                    auth_mode: Default::default(),
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
                    let lower = name.to_ascii_lowercase();
                    if lower.contains("sample")
                        || lower.contains("example")
                        || lower.contains("template")
                    {
                        continue;
                    }
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
    let Ok(val) = serde_json::from_str::<serde_json::Value>(content) else {
        return vec![];
    };
    let Some(obj) = val.as_object() else {
        return vec![];
    };

    // Try mcpServers wrapper
    let server_map = if let Some(inner) = obj.get("mcpServers").and_then(|v| v.as_object()) {
        inner.clone()
    } else {
        // Check if top-level entries look like server configs
        obj.clone()
    };

    let mut servers = Vec::new();
    for (name, config) in &server_map {
        let Some(cfg) = config.as_object() else {
            continue;
        };
        let transport = if let Some(cmd) = cfg.get("command").and_then(|v| v.as_str()) {
            let args: Vec<String> = cfg
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let env: std::collections::BTreeMap<String, String> = cfg
                .get("env")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let cwd = cfg.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let bundle = cfg.get("bundle").and_then(|v| v.as_str()).map(String::from);
            McpTransport::Stdio {
                command: cmd.to_string(),
                args,
                env,
                cwd,
                bundle,
            }
        } else if let Some(url) = cfg.get("url").and_then(|v| v.as_str()) {
            let headers: std::collections::BTreeMap<String, String> = cfg
                .get("headers")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                        .collect()
                })
                .unwrap_or_default();
            McpTransport::Sse {
                url: url.to_string(),
                headers,
            }
        } else {
            continue; // Not a recognizable server config
        };

        servers.push(McpServer {
            name: name.clone(),
            transport,
            targets: crate::mcp::adapters::SUPPORTED
                .iter()
                .map(|ide| ide.to_string())
                .collect(),
            description: cfg
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from),
            tags: vec![],
            disabled: false,
            source: None,
            runtime: None,
            auth_mode: Default::default(),
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

    let bytes = download_zip_with_fallback(
        &client,
        owner,
        repo,
        &resolved_ref,
        "downloading repo zipball",
    )
    .await?;

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
                        let new_sub = found
                            .strip_prefix(&top)
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
    if !is_skill_dir(&source_dir) {
        return Err(Error::Invalid(format!(
            "invalid skill format at {}: missing SKILL.md",
            source_dir.display()
        )));
    }

    // Copy to a clean temp dir using the skill whitelist filter
    let out = tempfile::tempdir()?;
    copy_skill_filtered(&source_dir, out.path())?;
    ensure_canonical_skill_manifest(out.path())?;

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
        let Some(rel) = entry.enclosed_name().map(PathBuf::from) else {
            continue;
        };
        let out = into.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() {
                fs::create_dir_all(p)?;
            }
            let mut f = fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut f)?;
        }
    }
    Ok(())
}

/// BFS-search for the first directory with the given lowercase name under `root`.
/// Returns the full path if found.
fn find_dir_by_name(root: &Path, name: &str) -> Option<PathBuf> {
    let skip = [
        ".github",
        ".git",
        "node_modules",
        "target",
        "dist",
        "build",
        "__pycache__",
    ];
    // Queue entries: (path, depth)
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> = std::collections::VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));
    while let Some((dir, depth)) = queue.pop_front() {
        if depth > 6 {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let n = entry.file_name().to_string_lossy().to_lowercase();
            if skip.contains(&n.as_str()) {
                continue;
            }
            if n == name {
                return Some(entry.path());
            }
            queue.push_back((entry.path(), depth + 1));
        }
    }
    None
}

/// Copy skill files from `src` to `dst`, applying the standard package whitelist.
///
/// Only copies files that belong in a skill package: SKILL.md, allowed subdirs
/// (references/, scripts/, assets/, templates/, data/, images/, media/, fonts/),
/// and root-level runtime config files.
fn copy_skill_filtered(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    let rules = SkillPackageRules::from_skill_dir(src);
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if entry.file_type().is_dir() {
            if should_include_path(&rel_str, true, &rules) {
                fs::create_dir_all(dst.join(rel))?;
            }
            continue;
        }
        if entry.file_type().is_file() && should_include_path(&rel_str, false, &rules) {
            let target = dst.join(rel);
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct SkillPackageRules {
    referenced_files: BTreeSet<String>,
    referenced_dirs: BTreeSet<String>,
}

impl SkillPackageRules {
    fn from_skill_dir(src: &Path) -> Self {
        let manifest = src.join("SKILL.md");
        let manifest = if manifest.is_file() {
            manifest
        } else {
            src.join("skill.md")
        };
        let Ok(content) = fs::read_to_string(manifest) else {
            return Self::default();
        };
        Self::from_manifest(&content)
    }

    fn from_manifest(content: &str) -> Self {
        let mut rules = Self::default();
        for token in extract_path_tokens(content) {
            let token = normalize_skill_rel(&token);
            if token.is_empty() {
                continue;
            }
            if token.ends_with('/') {
                rules
                    .referenced_dirs
                    .insert(token.trim_end_matches('/').to_string());
            } else if token_has_extension(&token) {
                rules.referenced_files.insert(token);
            } else {
                rules.referenced_dirs.insert(token);
            }
        }
        rules
    }

    fn references_file(&self, rel: &str) -> bool {
        self.referenced_files.contains(rel)
            || self
                .referenced_dirs
                .iter()
                .any(|dir| rel == dir || rel.starts_with(&format!("{dir}/")))
    }

    fn references_dir(&self, rel: &str) -> bool {
        self.referenced_dirs
            .iter()
            .any(|dir| rel == dir || dir.starts_with(&format!("{rel}/")))
            || self
                .referenced_files
                .iter()
                .any(|file| file.starts_with(&format!("{rel}/")))
    }
}

const ALWAYS_ALLOWED_DIRS: &[&str] = &["scripts", "templates", "data", "fonts"];
const REFERENCED_ONLY_DIRS: &[&str] = &["references", "assets", "images", "media"];

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".github",
    ".cursor",
    ".claude",
    ".codex",
    ".vscode",
    "agents",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    "docs",
    "tests",
    "test",
];

const ALLOWED_ROOT_FILES: &[&str] = &[
    ".env.example",
    "requirements.txt",
    "package.json",
    "pyproject.toml",
    "Cargo.toml",
];

const SKIP_ROOT_FILES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    "README.md",
    "readme.md",
    "README.rst",
    "readme.rst",
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
    "LICENCE",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "poetry.lock",
    "Pipfile.lock",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    ".prettierrc",
    ".eslintrc",
    ".eslintrc.json",
];

fn extract_path_tokens(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in content.split(is_path_token_boundary) {
        let token = raw.trim_matches(is_path_token_trim);
        if !token.contains('/') && !token.contains('\\') {
            continue;
        }
        let normalized = normalize_skill_rel(token);
        let Some(top) = normalized.split('/').next() else {
            continue;
        };
        let top_lc = top.to_lowercase();
        if ALWAYS_ALLOWED_DIRS.contains(&top_lc.as_str())
            || REFERENCED_ONLY_DIRS.contains(&top_lc.as_str())
            || token_has_extension(&normalized)
        {
            out.push(normalized);
        }
    }
    out
}

fn is_path_token_boundary(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '<' | '>' | '"' | '\'' | '`')
}

fn is_path_token_trim(c: char) -> bool {
    matches!(
        c,
        ',' | ';' | ':' | '.' | '!' | '?' | ')' | ']' | '}' | '"' | '\'' | '`'
    )
}

fn normalize_skill_rel(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect::<Vec<_>>()
        .join("/")
}

fn token_has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| !ext.is_empty())
        .unwrap_or(false)
}

/// Decide whether a relative path should be included in a skill package.
fn should_include_path(rel: &str, is_dir: bool, rules: &SkillPackageRules) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    if parts.is_empty() {
        return false;
    }
    let top = parts[0].to_lowercase();

    // Always skip well-known noise directories at any level
    for p in &parts {
        let lc = p.to_lowercase();
        if SKIP_DIRS.contains(&lc.as_str()) {
            return false;
        }
    }

    if parts.len() == 1 && !is_dir {
        let name = parts[0];
        let name_lc = top.as_str();
        if name_lc == "skill.md" {
            return true;
        }
        if is_readme_like(name_lc) || SKIP_ROOT_FILES.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            return false;
        }
        if ALLOWED_ROOT_FILES
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name))
        {
            return true;
        }
        return rules.references_file(rel);
    }

    if ALWAYS_ALLOWED_DIRS.contains(&top.as_str()) {
        return true;
    }

    if REFERENCED_ONLY_DIRS.contains(&top.as_str()) {
        return if is_dir {
            rules.references_dir(rel)
        } else {
            rules.references_file(rel)
        };
    }

    if is_dir {
        rules.references_dir(rel)
    } else {
        rules.references_file(rel)
    }
}

fn is_readme_like(name_lc: &str) -> bool {
    name_lc == "readme"
        || name_lc.starts_with("readme.")
        || name_lc.starts_with("readme-")
        || name_lc.starts_with("readme_")
}
/// Remove files from `dir` that were tracked in `old_hashes` but are absent
/// from `new_hashes`, provided the user has not modified them (hash matches).
/// Returns list of removed file relative paths.
pub fn cleanup_noise(
    dir: &Path,
    old_hashes: &BTreeMap<String, String>,
    new_hashes: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut removed = Vec::new();
    for (rel, old_hash) in old_hashes {
        if new_hashes.contains_key(rel) {
            continue;
        }
        let file = dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !file.exists() {
            continue;
        }
        let current = std::fs::read(&file).unwrap_or_default();
        let current_hash = hex::encode(Sha256::digest(&current));
        if current_hash == *old_hash {
            let _ = std::fs::remove_file(&file);
            removed.push(rel.clone());
        }
    }
    removed
}

fn find_single_top_dir(root: &Path) -> Result<PathBuf> {
    let mut iter = fs::read_dir(root)?;
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
fn find_deep_skill_containers(
    dir: &Path,
    repo_root: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) {
    if depth >= max_depth {
        return;
    }
    let skip = [
        ".github",
        ".git",
        "node_modules",
        ".vscode",
        "target",
        "dist",
        "build",
        "__pycache__",
        "docs",
    ];
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || skip.contains(&name.as_str()) {
            continue;
        }
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

// ─── Public wrappers for cross-module reuse (mcp::github) ───────────────────

pub fn make_client() -> Result<reqwest::Client> {
    client()
}

pub async fn resolve_ref_pub(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    r#ref: Option<&str>,
) -> Result<(String, String)> {
    resolve_ref(client, owner, repo, r#ref).await
}

pub async fn download_zip(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    r#ref: &str,
    context: &str,
) -> Result<Vec<u8>> {
    download_zip_with_fallback(client, owner, repo, r#ref, context).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_zip_urls_falls_back_to_mirrors_even_with_token() {
        let _guard = crate::test_support::lock();
        let old_token = std::env::var("GITHUB_TOKEN").ok();
        let old_mirror = std::env::var("GITHUB_MIRROR").ok();
        std::env::set_var("GITHUB_TOKEN", "dummy-token");
        std::env::remove_var("GITHUB_MIRROR");

        let urls = build_zip_urls("nextlevelbuilder", "ui-ux-pro-max-skill", "main");

        assert_eq!(
            urls.first().map(String::as_str),
            Some("https://codeload.github.com/nextlevelbuilder/ui-ux-pro-max-skill/zip/main")
        );
        assert!(urls
            .iter()
            .any(|u| u.starts_with("https://gh-proxy.org/https://codeload.github.com/")));
        assert!(urls
            .iter()
            .any(|u| u.starts_with("https://mirror.ghproxy.com/https://github.com/")));

        restore_env("GITHUB_TOKEN", old_token);
        restore_env("GITHUB_MIRROR", old_mirror);
    }

    #[test]
    fn build_zip_urls_prefers_configured_mirror() {
        let _guard = crate::test_support::lock();
        let old_mirror = std::env::var("GITHUB_MIRROR").ok();
        std::env::set_var(
            "GITHUB_MIRROR",
            "https://mirror-one.example; https://mirror-two.example/",
        );

        let urls = build_zip_urls("owner", "repo", "main");

        assert_eq!(
            urls.first().map(String::as_str),
            Some("https://codeload.github.com/owner/repo/zip/main")
        );
        assert!(urls.contains(
            &"https://mirror-one.example/https://codeload.github.com/owner/repo/zip/main"
                .to_string()
        ));
        assert!(urls.contains(
            &"https://mirror-two.example/https://github.com/owner/repo/archive/main.zip"
                .to_string()
        ));
        assert!(urls.contains(&"https://github.com/owner/repo/archive/main.zip".to_string()));

        restore_env("GITHUB_MIRROR", old_mirror);
    }

    #[test]
    fn validate_zip_archive_rejects_html_error_pages() {
        let err = validate_zip_archive(b"<html>not a zip</html>").unwrap_err();
        assert!(err.contains("invalid zip archive"));
    }

    #[test]
    fn detect_mcp_servers_prefers_real_config_over_sample() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("mcp-config.json"),
            r#"{
              "mcpServers": {
                "ppt": {
                  "command": "python",
                  "args": ["ppt_mcp_server.py"]
                }
              }
            }"#,
        )
        .unwrap();
        fs::write(
            root.path().join("mcp_config_sample.json"),
            r#"{
              "mcpServers": {
                "word-document-server": {
                  "command": "python",
                  "args": ["word_server.py"]
                }
              }
            }"#,
        )
        .unwrap();

        let servers = detect_mcp_servers(root.path());
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "ppt");
    }

    fn restore_env(name: &str, value: Option<String>) {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }

    #[test]
    fn detect_skill_dirs_requires_manifest() {
        let root = tempfile::tempdir().unwrap();
        let valid = root.path().join("valid");
        let readme_only = root.path().join("readme-only");
        fs::create_dir_all(&valid).unwrap();
        fs::create_dir_all(&readme_only).unwrap();
        fs::write(valid.join("SKILL.md"), "# Valid").unwrap();
        fs::write(readme_only.join("README.md"), "# Not a skill").unwrap();

        assert_eq!(detect_skill_dirs(root.path()), vec!["valid".to_string()]);
    }

    #[test]
    fn parse_mcp_json_preserves_bundle() {
        let servers = parse_mcp_json(
            r#"{
              "mcpServers": {
                "local-python": {
                  "command": "python",
                  "args": ["server.py"],
                  "cwd": "{BUNDLE}",
                  "bundle": "local-python-bundle"
                }
              }
            }"#,
        );

        assert_eq!(servers.len(), 1);
        match &servers[0].transport {
            McpTransport::Stdio { bundle, cwd, .. } => {
                assert_eq!(bundle.as_deref(), Some("local-python-bundle"));
                assert_eq!(cwd.as_deref(), Some("{BUNDLE}"));
            }
            other => panic!("expected stdio transport, got {other:?}"),
        }
    }

    #[test]
    fn ensure_canonical_skill_manifest_copies_lowercase() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("skill.md"), "# lower").unwrap();

        ensure_canonical_skill_manifest(dir.path()).unwrap();

        assert!(dir.path().join("SKILL.md").is_file());
        assert_eq!(
            fs::read_to_string(dir.path().join("SKILL.md")).unwrap(),
            "# lower"
        );
    }

    #[test]
    fn copy_skill_filtered_skips_readme_and_docs() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(
            src.path().join("SKILL.md"),
            "# Skill\n\nUse references/api.md and assets/icon.svg.",
        )
        .unwrap();
        fs::write(src.path().join("AGENTS.md"), "agent instructions").unwrap();
        fs::write(src.path().join("CLAUDE.md"), "claude instructions").unwrap();
        fs::write(src.path().join("README.md"), "# Readme").unwrap();
        fs::write(src.path().join("README.zh-CN.md"), "# Readme").unwrap();
        fs::create_dir(src.path().join("docs")).unwrap();
        fs::write(src.path().join("docs").join("guide.md"), "guide").unwrap();
        fs::create_dir(src.path().join("agents")).unwrap();
        fs::write(src.path().join("agents").join("openai.yaml"), "model: x").unwrap();
        fs::create_dir(src.path().join("references")).unwrap();
        fs::write(src.path().join("references").join("api.md"), "api").unwrap();
        fs::write(src.path().join("references").join("unused.md"), "unused").unwrap();
        fs::create_dir(src.path().join("assets")).unwrap();
        fs::write(src.path().join("assets").join("icon.svg"), "<svg/>").unwrap();
        fs::write(src.path().join("assets").join("usage-example.png"), "png").unwrap();

        copy_skill_filtered(src.path(), dst.path()).unwrap();

        assert!(dst.path().join("SKILL.md").is_file());
        assert!(!dst.path().join("AGENTS.md").exists());
        assert!(!dst.path().join("CLAUDE.md").exists());
        assert!(!dst.path().join("README.md").exists());
        assert!(!dst.path().join("README.zh-CN.md").exists());
        assert!(!dst.path().join("docs").exists());
        assert!(!dst.path().join("agents").exists());
        assert!(dst.path().join("references").join("api.md").is_file());
        assert!(!dst.path().join("references").join("unused.md").exists());
        assert!(dst.path().join("assets").join("icon.svg").is_file());
        assert!(!dst.path().join("assets").join("usage-example.png").exists());
    }

    #[test]
    fn cleanup_noise_removes_unmodified_obsolete_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("old.txt"), "old content").unwrap();
        fs::write(dir.path().join("kept.txt"), "modified").unwrap();

        let old_hash_old = hex::encode(Sha256::digest(b"old content"));
        let old_hash_kept = hex::encode(Sha256::digest(b"original"));
        let mut old_hashes = BTreeMap::new();
        old_hashes.insert("old.txt".to_string(), old_hash_old);
        old_hashes.insert("kept.txt".to_string(), old_hash_kept);

        let new_hashes = BTreeMap::new(); // empty = nothing in new package

        let removed = cleanup_noise(dir.path(), &old_hashes, &new_hashes);
        assert_eq!(removed, vec!["old.txt".to_string()]);
        assert!(!dir.path().join("old.txt").exists());
        assert!(dir.path().join("kept.txt").exists()); // user modified, preserved
    }
}
