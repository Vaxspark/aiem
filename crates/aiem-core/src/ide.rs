//! Supported IDE targets for **skills** installation.
//!
//! Each IDE has a directory relative to a workspace root (for project scope) and/or
//! relative to the user home (for global scope) where skills are expected to live.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Scope { User, Project }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeTarget {
    pub id: &'static str,
    pub display_name: &'static str,
    /// Path relative to the scope root where skill directories are symlinked into.
    pub skills_dir: &'static str,
    pub default_scope: Scope,
}

pub const IDES: &[IdeTarget] = &[
    IdeTarget { id: "claude-code",  display_name: "Claude Code",  skills_dir: ".claude/skills",  default_scope: Scope::User },
    IdeTarget { id: "codex",        display_name: "Codex",        skills_dir: ".codex/skills",   default_scope: Scope::User },
    IdeTarget { id: "cursor",       display_name: "Cursor",       skills_dir: ".cursor/skills",  default_scope: Scope::Project },
    IdeTarget { id: "vscode",       display_name: "VSCode / Copilot", skills_dir: ".github/skills", default_scope: Scope::Project },
    IdeTarget { id: "windsurf",     display_name: "Windsurf",     skills_dir: ".windsurf/skills", default_scope: Scope::Project },
    IdeTarget { id: "trae",         display_name: "Trae",         skills_dir: ".trae/skills",    default_scope: Scope::Project },
    IdeTarget { id: "qoder",        display_name: "Qoder",        skills_dir: ".qoder/skills",   default_scope: Scope::Project },
    IdeTarget { id: "kiro",         display_name: "Kiro",         skills_dir: ".kiro/skills",    default_scope: Scope::Project },
];

pub fn find(id: &str) -> Option<&'static IdeTarget> {
    IDES.iter().find(|i| i.id.eq_ignore_ascii_case(id))
}
