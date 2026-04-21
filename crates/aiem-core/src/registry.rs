//! Search online registries (smithery.ai, glama.ai, claude-plugins.dev) for MCP servers and skills.

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct RegistryItem {
    pub name: String,
    pub description: String,
    pub url: String,
    pub source: RegistrySource,
    pub use_count: u64,
    pub github: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrySource {
    Smithery,
    Glama,
    Skills,
}

impl RegistrySource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Smithery => "smithery.ai",
            Self::Glama => "glama.ai",
            Self::Skills => "skills",
        }
    }
}

// ─── Smithery ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SmitheryResponse {
    servers: Vec<SmitheryServer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SmitheryServer {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    qualified_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    use_count: u64,
}

pub async fn search_smithery(query: &str) -> crate::Result<Vec<RegistryItem>> {
    let url = format!(
        "https://registry.smithery.ai/servers?q={}&pageSize=20",
        urlencoding::encode(query)
    );
    let resp = reqwest::get(&url).await?;
    let body: SmitheryResponse = resp.json().await?;
    Ok(body.servers.into_iter().map(|s| {
        let name = if s.display_name.is_empty() { s.qualified_name.clone() } else { s.display_name };
        RegistryItem {
            name,
            description: s.description,
            url: s.homepage.clone(),
            source: RegistrySource::Smithery,
            use_count: s.use_count,
            github: extract_github(&s.homepage),
        }
    }).collect())
}

// ─── Glama ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GlamaResponse {
    #[serde(default)]
    data: Vec<GlamaServer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlamaServer {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    repository_url: String,
    #[serde(default)]
    stars_count: u64,
}

pub async fn search_glama(query: &str) -> crate::Result<Vec<RegistryItem>> {
    let url = format!(
        "https://glama.ai/api/mcp/v1/servers?query={}&first=20",
        urlencoding::encode(query)
    );
    let resp = reqwest::get(&url).await?;
    let body: GlamaResponse = resp.json().await?;
    Ok(body.data.into_iter().map(|s| {
        let url = if s.repository_url.is_empty() {
            format!("https://glama.ai/mcp/servers")
        } else {
            s.repository_url.clone()
        };
        RegistryItem {
            name: s.name,
            description: s.description,
            url: url.clone(),
            source: RegistrySource::Glama,
            use_count: s.stars_count,
            github: extract_github(&url),
        }
    }).collect())
}

// ─── Claude-Plugins / Skills ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct SkillsResponse {
    skills: Vec<SkillsEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillsEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    source_url: String,
    #[serde(default)]
    stars: u64,
    #[serde(default)]
    installs: u64,
}

pub async fn search_skills(query: &str) -> crate::Result<Vec<RegistryItem>> {
    let url = format!(
        "https://claude-plugins.dev/api/skills?q={}&limit=20",
        urlencoding::encode(query)
    );
    let resp = reqwest::get(&url).await?;
    let body: SkillsResponse = resp.json().await?;
    Ok(body.skills.into_iter().map(|s| {
        let github = extract_github(&s.source_url);
        RegistryItem {
            name: s.name,
            description: s.description,
            url: s.source_url,
            source: RegistrySource::Skills,
            use_count: s.installs.max(s.stars),
            github,
        }
    }).collect())
}

/// Combined search across both registries.
pub async fn search_all(query: &str) -> crate::Result<Vec<RegistryItem>> {
    let (smithery, glama, skills) = tokio::join!(
        search_smithery(query),
        search_glama(query),
        search_skills(query)
    );
    let mut results = smithery.unwrap_or_default();
    results.extend(glama.unwrap_or_default());
    results.extend(skills.unwrap_or_default());
    // Sort by use_count desc
    results.sort_by(|a, b| b.use_count.cmp(&a.use_count));
    Ok(results)
}

/// Fetch popular / trending items (no query, just top by usage).
pub async fn popular() -> crate::Result<Vec<RegistryItem>> {
    // Smithery: empty query returns popular servers sorted by usage
    let url = "https://registry.smithery.ai/servers?pageSize=15";
    let resp = reqwest::get(url).await?;
    let body: SmitheryResponse = resp.json().await?;
    let mut results: Vec<RegistryItem> = body.servers.into_iter().map(|s| {
        let name = if s.display_name.is_empty() { s.qualified_name.clone() } else { s.display_name };
        RegistryItem {
            name,
            description: s.description,
            url: s.homepage.clone(),
            source: RegistrySource::Smithery,
            use_count: s.use_count,
            github: extract_github(&s.homepage),
        }
    }).collect();
    results.sort_by(|a, b| b.use_count.cmp(&a.use_count));
    Ok(results)
}

fn extract_github(url: &str) -> Option<String> {
    let stripped = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let parts: Vec<&str> = stripped.split('/').collect();
    if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        // Handle URLs like owner/repo/tree/main/subdir → owner/repo//subdir
        if parts.len() >= 5 && parts[2] == "tree" {
            let subdir = parts[4..].join("/");
            if !subdir.is_empty() {
                return Some(format!("{}/{}//{}", parts[0], parts[1], subdir));
            }
        }
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}
