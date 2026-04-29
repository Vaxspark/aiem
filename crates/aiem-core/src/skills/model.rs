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

/// When the user pastes a URL with a third-party prefix before `https://github.com/...`,
/// set `GITHUB_MIRROR` / `GITHUB_API_MIRROR` for [`crate::skills::github`] downloads,
/// and return the slice starting at `https://github.com/` (or `http://github.com/`).
/// Call this **before** [`SkillSource::parse_github`]; parsing itself has no side effects.
pub fn apply_github_proxy_env(input: &str) -> &str {
    let s = input.trim();
    if let Some(pos) = s.find("https://github.com/") {
        if pos > 0 {
            let proxy_base = s[..pos].trim_end_matches('/');
            std::env::set_var(
                "GITHUB_MIRROR",
                format!("{proxy_base}/https://codeload.github.com"),
            );
            std::env::set_var(
                "GITHUB_API_MIRROR",
                format!("{proxy_base}/https://api.github.com"),
            );
        }
        return &s[pos..];
    }
    if let Some(pos) = s.find("http://github.com/") {
        if pos > 0 {
            let proxy_base = s[..pos].trim_end_matches('/');
            std::env::set_var(
                "GITHUB_MIRROR",
                format!("{proxy_base}/https://codeload.github.com"),
            );
            std::env::set_var(
                "GITHUB_API_MIRROR",
                format!("{proxy_base}/https://api.github.com"),
            );
        }
        return &s[pos..];
    }
    s
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
            if owner.is_empty() || repo.is_empty() {
                return None;
            }
            return Some(SkillSource::GitHub {
                owner,
                repo,
                r#ref,
                subdir,
            });
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
            if owner.is_empty() || repo.is_empty() {
                return None;
            }
            return Some(SkillSource::GitHub {
                owner,
                repo,
                r#ref,
                subdir,
            });
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
            SkillSource::GitHub {
                owner,
                repo,
                subdir,
                ..
            } => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_owner_repo() {
        let s = SkillSource::parse_github("alice/my-skill").unwrap();
        assert_eq!(
            s,
            SkillSource::GitHub {
                owner: "alice".into(),
                repo: "my-skill".into(),
                r#ref: None,
                subdir: None,
            }
        );
    }

    #[test]
    fn parse_with_ref_and_subdir() {
        let s = SkillSource::parse_github("alice/repo//sub@v2").unwrap();
        assert_eq!(
            s,
            SkillSource::GitHub {
                owner: "alice".into(),
                repo: "repo".into(),
                r#ref: Some("v2".into()),
                subdir: Some("sub".into()),
            }
        );
    }

    #[test]
    fn parse_full_github_url() {
        let s = SkillSource::parse_github("https://github.com/bob/tools").unwrap();
        assert_eq!(
            s,
            SkillSource::GitHub {
                owner: "bob".into(),
                repo: "tools".into(),
                r#ref: None,
                subdir: None,
            }
        );
    }

    #[test]
    fn parse_github_tree_url() {
        let s = SkillSource::parse_github("https://github.com/bob/tools/tree/main/sub").unwrap();
        assert_eq!(
            s,
            SkillSource::GitHub {
                owner: "bob".into(),
                repo: "tools".into(),
                r#ref: Some("main".into()),
                subdir: Some("sub".into()),
            }
        );
    }

    #[test]
    fn parse_rejects_empty_parts() {
        assert!(SkillSource::parse_github("").is_none());
        assert!(SkillSource::parse_github("/repo").is_none());
    }

    #[test]
    fn parse_does_not_set_env() {
        let before_mirror = std::env::var("GITHUB_MIRROR").ok();
        let before_api = std::env::var("GITHUB_API_MIRROR").ok();
        let _ = SkillSource::parse_github("https://proxy.example.com/https://github.com/o/r");
        assert_eq!(std::env::var("GITHUB_MIRROR").ok(), before_mirror);
        assert_eq!(std::env::var("GITHUB_API_MIRROR").ok(), before_api);
    }

    #[test]
    fn apply_proxy_env_sets_vars() {
        let _ = apply_github_proxy_env("https://proxy.example.com/https://github.com/o/r");
        assert!(std::env::var("GITHUB_MIRROR")
            .unwrap()
            .contains("proxy.example.com"));
    }

    #[test]
    fn canonical_id_no_subdir() {
        let s = SkillSource::GitHub {
            owner: "a".into(),
            repo: "b".into(),
            r#ref: None,
            subdir: None,
        };
        assert_eq!(s.canonical_id(), "a__b");
    }

    #[test]
    fn canonical_id_with_subdir() {
        let s = SkillSource::GitHub {
            owner: "a".into(),
            repo: "b".into(),
            r#ref: None,
            subdir: Some("x/y".into()),
        };
        assert_eq!(s.canonical_id(), "a__b__y");
    }
}
