use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where a skill comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SkillSource {
    /// Fetched from a GitHub repo (or a subdir of it).
    GitHub {
        owner: String,
        repo: String,
        /// Branch, tag or commit. Defaults to the repo's default branch when `None`.
        #[serde(default)]
        r#ref: Option<String>,
        /// Optional sub-directory inside the repo that contains the skill.
        #[serde(default)]
        subdir: Option<String>,
    },
    /// A local directory, imported as-is.
    Local { path: PathBuf },
}

impl SkillSource {
    /// Parse shorthand strings like:
    ///   "owner/repo"
    ///   "owner/repo@ref"
    ///   "owner/repo//subdir"
    ///   "owner/repo//subdir@ref"
    ///   "github:owner/repo"
    ///   "https://github.com/owner/repo"
    pub fn parse_github(s: &str) -> Option<Self> {
        let s = s.trim();

        // Strip common GitHub proxy prefixes (e.g. https://gh-proxy.org/https://github.com/...)
        let s = if let Some(pos) = s.find("https://github.com/") {
            // If there's a proxy prefix before the real GitHub URL, extract the proxy
            // and set it as GITHUB_MIRROR env var for downloads
            if pos > 0 {
                let proxy_base = &s[..pos];
                let proxy_base = proxy_base.trim_end_matches('/');
                // Set env vars so fetch_github uses this proxy
                std::env::set_var("GITHUB_MIRROR", format!("{proxy_base}/https://codeload.github.com"));
                std::env::set_var("GITHUB_API_MIRROR", format!("{proxy_base}/https://api.github.com"));
            }
            &s[pos..]
        } else if let Some(pos) = s.find("http://github.com/") {
            if pos > 0 {
                let proxy_base = &s[..pos].trim_end_matches('/');
                std::env::set_var("GITHUB_MIRROR", format!("{proxy_base}/https://codeload.github.com"));
                std::env::set_var("GITHUB_API_MIRROR", format!("{proxy_base}/https://api.github.com"));
            }
            &s[pos..]
        } else {
            s
        };

        let s = s
            .strip_prefix("github:")
            .or_else(|| s.strip_prefix("https://github.com/"))
            .or_else(|| s.strip_prefix("http://github.com/"))
            .unwrap_or(s)
            .trim_end_matches('/')
            .trim_end_matches(".git");

        // Handle /tree/<ref>[/subdir...] pattern from GitHub web URLs
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() >= 4 && parts[2] == "tree" {
            let owner = parts[0].to_string();
            let repo = parts[1].to_string();
            let r#ref = Some(parts[3].to_string());
            let subdir = if parts.len() > 4 {
                Some(parts[4..].join("/"))
            } else {
                None
            };
            if owner.is_empty() || repo.is_empty() { return None; }
            return Some(SkillSource::GitHub { owner, repo, r#ref, subdir });
        }
        // Also handle /blob/<ref>/... pattern
        if parts.len() >= 4 && parts[2] == "blob" {
            let owner = parts[0].to_string();
            let repo = parts[1].to_string();
            let r#ref = Some(parts[3].to_string());
            let subdir = if parts.len() > 4 {
                Some(parts[4..].join("/"))
            } else {
                None
            };
            if owner.is_empty() || repo.is_empty() { return None; }
            return Some(SkillSource::GitHub { owner, repo, r#ref, subdir });
        }

        let (body, r#ref) = match s.rsplit_once('@') {
            Some((a, b)) if !b.is_empty() => (a, Some(b.to_string())),
            _ => (s, None),
        };
        let (repo_part, subdir) = match body.split_once("//") {
            Some((a, b)) => (a, Some(b.trim_matches('/').to_string())),
            None => (body, None),
        };
        let (owner, repo) = repo_part.split_once('/')?;
        if owner.is_empty() || repo.is_empty() {
            return None;
        }
        Some(SkillSource::GitHub {
            owner: owner.to_string(),
            repo: repo.to_string(),
            r#ref,
            subdir,
        })
    }

    pub fn canonical_id(&self) -> String {
        match self {
            SkillSource::GitHub { owner, repo, subdir, .. } => {
                let mut id = format!("{owner}__{repo}");
                if let Some(sd) = subdir {
                    // Use only the last path component for the id
                    // e.g. ".claude/skills/banner-design" -> "banner-design"
                    let tail = sd.rsplit('/').next().unwrap_or(sd);
                    id.push_str("__");
                    id.push_str(&tail.replace(['/', '\\'], "_"));
                }
                id
            }
            SkillSource::Local { path } => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                format!("local__{name}")
            }
        }
    }
}

/// A single skill tracked in the local registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub source: SkillSource,
    /// Installed version identifier (commit sha / tag / timestamp).
    pub version: String,
    /// Absolute path on disk where the skill content lives.
    pub path: PathBuf,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
    /// Deployments: map of `ide_id` -> workspace root (or `"~"` for user scope).
    #[serde(default)]
    pub deployments: BTreeMap<String, Vec<String>>,
    /// SHA-256 hashes (hex) of each file at install/update time, keyed by relative path.
    /// Used by smart-merge to detect whether a file has been locally modified since install.
    #[serde(default)]
    pub file_hashes: BTreeMap<String, String>,
}

/// On-disk index file under `~/.aiem/skills/index.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SkillIndex {
    #[serde(default)]
    pub skills: BTreeMap<String, Skill>,
}
