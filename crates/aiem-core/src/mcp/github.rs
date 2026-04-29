//! GitHub-based MCP import: two-phase preview → confirm flow.
//!
//! 1. `preview_github_mcp(owner, repo, ref, subdir)` downloads and analyzes
//!    the repo, returning a [`McpPreview`] that describes detected servers,
//!    bundle files, auth requirements, and warnings — without writing anything.
//!
//! 2. `import_github_mcp(preview, selected)` materialises the preview: writes
//!    servers to the registry, saves bundles to `~/.aiem/mcp/bundles/`, and
//!    stores detected tokens in the Vault.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{fs, io::Cursor};

use crate::mcp::bundles;
use crate::mcp::model::*;
use crate::mcp::McpRegistry;
use crate::secrets::Vault;
use crate::{Error, Result};

// ─── Preview types ──────────────────────────────────────────────────────────

/// A single detected MCP server with its proposed bundle and metadata.
#[derive(Debug, Clone)]
pub struct PreviewServer {
    pub server: McpServer,
    /// Directory relative to repo root that will be copied as the bundle root.
    /// Empty string means the repository root. `None` means this server has no bundle.
    pub bundle_source: Option<String>,
    /// Files relative to repo root that will be copied into the bundle.
    pub kept_files: Vec<String>,
    /// Files that were found but will be dropped by the filter.
    pub dropped_files: Vec<String>,
    /// Entry point file relative to the bundle root (e.g. `server.py`).
    pub entrypoint: Option<String>,
    /// Detected secrets in env/headers (name → raw value before vault save).
    pub detected_secrets: Vec<(String, String)>,
    /// Human-readable warnings.
    pub warnings: Vec<String>,
}

/// Result of the analysis phase.
#[derive(Debug, Clone)]
pub struct McpPreview {
    pub owner: String,
    pub repo: String,
    pub r#ref: String,
    pub commit: String,
    pub servers: Vec<PreviewServer>,
    pub warnings: Vec<String>,
}

// ─── File filter ────────────────────────────────────────────────────────────

const DROP_DIRS: &[&str] = &[
    ".git",
    ".github",
    ".vscode",
    ".idea",
    "docs",
    "doc",
    "tests",
    "test",
    "examples",
    "example",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
    "target",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "coverage",
    ".tox",
    "egg-info",
];

const DROP_ROOT_FILES: &[&str] = &[
    "README.md",
    "README.rst",
    "README.txt",
    "README",
    "CHANGELOG.md",
    "CHANGELOG",
    "CONTRIBUTING.md",
    "CONTRIBUTING",
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
    "AGENTS.md",
    "CLAUDE.md",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    ".pre-commit-config.yaml",
    "Makefile",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    ".dockerignore",
    "Jenkinsfile",
    ".travis.yml",
    "tox.ini",
    "mypy.ini",
    ".flake8",
    ".eslintrc",
    ".eslintrc.json",
    ".prettierrc",
    "tsconfig.json",
    "jest.config.js",
    "jest.config.ts",
    "babel.config.js",
    "webpack.config.js",
    "rollup.config.js",
    "vite.config.ts",
    "vitest.config.ts",
];

const KEEP_DEP_FILES: &[&str] = &[
    "requirements.txt",
    "requirements-dev.txt",
    "requirements-test.txt",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "uv.lock",
    "poetry.lock",
    "package.json",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    ".env.example",
];

const KEEP_EXTENSIONS: &[&str] = &[
    "py", "js", "ts", "mjs", "cjs", "jsx", "tsx", "json", "toml", "yaml", "yml", "cfg", "ini",
    "sh", "bat", "ps1", "sql", "graphql", "gql",
];

const KEEP_DATA_DIRS: &[&str] = &[
    "data",
    "templates",
    "assets",
    "config",
    "configs",
    "schemas",
    "schema",
    "prompts",
    "migrations",
    "static",
    "public",
    "resources",
    "src",
    "lib",
    "utils",
    "helpers",
    "core",
    "models",
    "api",
    "services",
    "middleware",
    "routes",
    "handlers",
    "plugins",
    "tools",
];

/// Classify a file path (relative to the bundle root) as kept or dropped.
fn should_keep_file(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    if parts.is_empty() {
        return false;
    }

    // Drop files inside blacklisted directories at any depth.
    for part in &parts[..parts.len().saturating_sub(1)] {
        let lower = part.to_ascii_lowercase();
        if DROP_DIRS.iter().any(|d| lower == *d) {
            return false;
        }
        if lower.ends_with(".egg-info") {
            return false;
        }
    }

    let filename = parts.last().unwrap_or(&"");
    let filename_lower = filename.to_ascii_lowercase();

    // Always keep dependency manifests.
    if KEEP_DEP_FILES.iter().any(|f| filename_lower == *f) {
        return true;
    }
    // Catch any requirements*.txt variant (requirements-prod.txt, etc.)
    if filename_lower.starts_with("requirements") && filename_lower.ends_with(".txt") {
        return true;
    }

    // Drop root-level documentation / CI files.
    if parts.len() == 1 && DROP_ROOT_FILES.iter().any(|f| filename_lower == *f) {
        return false;
    }

    // Keep files in known data/source directories.
    if parts.len() > 1 {
        let first_dir = parts[0].to_ascii_lowercase();
        if KEEP_DATA_DIRS.iter().any(|d| first_dir == *d) {
            return true;
        }
    }

    // Keep files with source/config extensions.
    if let Some(ext) = filename_lower.rsplit('.').next() {
        if KEEP_EXTENSIONS.iter().any(|e| ext == *e) {
            return true;
        }
    }

    // Keep __init__.py anywhere (Python packages).
    if filename_lower == "__init__.py" {
        return true;
    }

    // Drop everything else (images, binaries, docs).
    false
}

// ─── Runtime detection ──────────────────────────────────────────────────────

fn detect_runtime(dir: &Path) -> McpRuntime {
    let has_py = dir.join("server.py").is_file()
        || dir.join("main.py").is_file()
        || dir.join("__init__.py").is_file()
        || dir.join("pyproject.toml").is_file()
        || has_extension_in_dir(dir, "py");
    if has_py {
        return McpRuntime::Python;
    }
    let has_node = dir.join("index.js").is_file()
        || dir.join("index.ts").is_file()
        || dir.join("package.json").is_file()
        || has_extension_in_dir(dir, "js")
        || has_extension_in_dir(dir, "ts");
    if has_node {
        return McpRuntime::Node;
    }
    McpRuntime::Other
}

fn has_extension_in_dir(dir: &Path, ext: &str) -> bool {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().map(|x| x == ext).unwrap_or(false))
}

fn detect_entrypoint(dir: &Path, server: &McpServer) -> Option<String> {
    if let McpTransport::Stdio { args, .. } = &server.transport {
        for a in args {
            let p = dir.join(a);
            if p.is_file() {
                return Some(a.clone());
            }
            if let Some(name) = Path::new(a).file_name().and_then(|n| n.to_str()) {
                if dir.join(name).is_file() {
                    return Some(name.to_string());
                }
            }
        }
    }
    for candidate in &["server.py", "main.py", "index.js", "index.ts", "app.py"] {
        if dir.join(candidate).is_file() {
            return Some(candidate.to_string());
        }
    }
    let mut py_candidates = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map(|ext| ext == "py").unwrap_or(false))
        .filter_map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.to_string())
        })
        .collect::<Vec<_>>();
    py_candidates.sort_by_key(|name| {
        let lower = name.to_ascii_lowercase();
        (
            !(lower.contains("mcp") || lower.contains("server")),
            name.clone(),
        )
    });
    py_candidates.into_iter().next()
}

fn normalize_pathish_value(value: &str, bundle_root: &Path) -> Option<String> {
    let file_name = Path::new(value).file_name().and_then(|n| n.to_str())?;
    let candidate = bundle_root.join(file_name);
    if candidate.is_file() {
        return Some(file_name.to_string());
    }
    None
}

fn normalize_bundle_env_value(key: &str, value: &str, bundle_root: &Path) -> String {
    if value == "{BUNDLE}" || value.contains("${secret:") {
        return value.to_string();
    }
    let p = Path::new(value);
    let looks_absolute = p.is_absolute()
        || value.starts_with('/')
        || value.as_bytes().get(1).map(|b| *b == b':').unwrap_or(false);
    if looks_absolute {
        if key.eq_ignore_ascii_case("PYTHONPATH") {
            return "{BUNDLE}".to_string();
        }
        if p.file_name().is_none() {
            return "{BUNDLE}".to_string();
        }
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            if bundle_root.join(name).exists() {
                return format!("{{BUNDLE}}/{name}");
            }
            if key.to_ascii_uppercase().ends_with("_PATH") {
                return format!("{{BUNDLE}}/{name}");
            }
        }
    }
    value.to_string()
}

// ─── Auth detection ─────────────────────────────────────────────────────────

fn looks_like_token(val: &str) -> bool {
    let v = val.trim();
    if v.is_empty() || v.starts_with("${secret:") {
        return false;
    }
    let patterns = [
        "ghp_", "gho_", "ghu_", "ghs_", "sk-", "xoxb-", "xoxp-", "key-", "token-", "Bearer ",
        "Basic ", "eyJ",
    ];
    if patterns.iter().any(|p| v.starts_with(p)) {
        return true;
    }
    // Long alphanumeric strings (32+) that look like API keys.
    v.len() >= 32
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn detect_auth(server: &McpServer) -> (McpAuthMode, Vec<(String, String)>) {
    let mut secrets = Vec::new();
    let mut has_secret_ref = false;
    let mut has_missing = false;

    let vault = Vault::load().ok();

    let server_part = sanitize_secret_part(&server.name);
    let check = |key: &str,
                 val: &str,
                 secrets: &mut Vec<(String, String)>,
                 has_ref: &mut bool,
                 has_miss: &mut bool| {
        if val.contains("${secret:") {
            *has_ref = true;
            let re_name = val
                .split("${secret:")
                .nth(1)
                .and_then(|s| s.split('}').next())
                .unwrap_or("");
            if !re_name.is_empty() {
                if let Some(ref v) = vault {
                    if v.get(re_name).is_err() {
                        *has_miss = true;
                    }
                }
            }
        } else if looks_like_token(val) {
            let key_part = sanitize_secret_part(key);
            let secret_name = format!("MCP_{server_part}_{key_part}_KEY");
            secrets.push((secret_name, val.to_string()));
        }
    };

    match &server.transport {
        McpTransport::Stdio { env, .. } => {
            for (k, v) in env {
                check(k, v, &mut secrets, &mut has_secret_ref, &mut has_missing);
            }
        }
        McpTransport::Http { headers, .. } | McpTransport::Sse { headers, .. } => {
            for (k, v) in headers {
                check(k, v, &mut secrets, &mut has_secret_ref, &mut has_missing);
            }
        }
    }

    let mode = if has_missing {
        McpAuthMode::MissingSecret
    } else if !secrets.is_empty() || has_secret_ref {
        McpAuthMode::SecretRef
    } else if looks_like_external_auth(server) {
        McpAuthMode::External
    } else {
        McpAuthMode::None
    };
    (mode, secrets)
}

fn sanitize_secret_part(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn looks_like_external_auth(server: &McpServer) -> bool {
    let mut haystack = server.name.to_ascii_lowercase();
    if let Some(desc) = &server.description {
        haystack.push(' ');
        haystack.push_str(&desc.to_ascii_lowercase());
    }
    for tag in &server.tags {
        haystack.push(' ');
        haystack.push_str(&tag.to_ascii_lowercase());
    }
    if let McpTransport::Stdio { command, args, .. } = &server.transport {
        haystack.push(' ');
        haystack.push_str(&command.to_ascii_lowercase());
        for arg in args {
            haystack.push(' ');
            haystack.push_str(&arg.to_ascii_lowercase());
        }
    }
    [
        "oauth",
        "login",
        "sign-in",
        "signin",
        "browser auth",
        "credential",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

/// Replace raw token values in env/headers with `${secret:NAME}` references,
/// and save the tokens to the Vault.
fn vault_secrets(server: &mut McpServer, secrets: &[(String, String)]) {
    if secrets.is_empty() {
        return;
    }
    let Ok(mut vault) = Vault::load() else {
        return;
    };
    for (name, raw) in secrets {
        let _ = vault.set(
            name,
            raw,
            Some(format!("Auto-imported from MCP server {}", server.name)),
        );
        let placeholder = format!("${{secret:{name}}}");
        match &mut server.transport {
            McpTransport::Stdio { env, .. } => {
                for v in env.values_mut() {
                    if v == raw {
                        *v = placeholder.clone();
                    }
                }
            }
            McpTransport::Http { headers, .. } | McpTransport::Sse { headers, .. } => {
                for v in headers.values_mut() {
                    if v == raw {
                        *v = placeholder.clone();
                    }
                }
            }
        }
    }
}

// ─── Preview ────────────────────────────────────────────────────────────────

/// Download and analyze a GitHub repo for MCP servers without writing anything.
pub async fn preview_github_mcp(
    owner: &str,
    repo: &str,
    r#ref: Option<&str>,
    subdir: Option<&str>,
) -> Result<McpPreview> {
    use crate::skills::github::detect_mcp_servers;

    let (real_owner, real_repo, requested_ref, requested_subdir) =
        normalize_github_source(owner, repo, r#ref, subdir)?;

    let client = crate::skills::github::make_client()?;
    let (resolved_ref, commit) = crate::skills::github::resolve_ref_pub(
        &client,
        &real_owner,
        &real_repo,
        requested_ref.as_deref(),
    )
    .await?;

    let zip_bytes = crate::skills::github::download_zip(
        &client,
        &real_owner,
        &real_repo,
        &commit,
        "mcp-preview",
    )
    .await?;

    let staging = tempfile::tempdir()?;
    zip::ZipArchive::new(Cursor::new(&zip_bytes))?.extract(staging.path())?;
    let top = find_single_top_dir(staging.path())?;

    // When a subdir is specified, restrict analysis to that subdirectory.
    let analysis_root = if let Some(sd) = requested_subdir.as_deref() {
        let sub = top.join(sd);
        if !sub.is_dir() {
            return Err(Error::NotFound(format!(
                "subdirectory `{sd}` not found in {}/{}",
                real_owner, real_repo
            )));
        }
        sub
    } else {
        top.clone()
    };

    let raw_servers = detect_mcp_servers(&analysis_root);
    let mut warnings = Vec::new();

    if raw_servers.is_empty() {
        warnings.push("No MCP servers detected in this repository.".into());
        return Ok(McpPreview {
            owner: real_owner,
            repo: real_repo,
            r#ref: resolved_ref,
            commit,
            servers: vec![],
            warnings,
        });
    }

    let mut preview_servers = Vec::new();
    for mut srv in raw_servers {
        let bundle_name = crate::fs_util::sanitize_for_path(&format!(
            "{}__{}__{}",
            real_owner, real_repo, srv.name
        ));

        // Determine the server's bundle root directory.
        let (bundle_root, bundle_source) = match &srv.transport {
            McpTransport::Stdio { cwd: Some(cwd), .. } => {
                let p = top.join(cwd);
                if p.is_dir() {
                    (p, Some(cwd.replace('\\', "/")))
                } else {
                    (top.clone(), Some(String::new()))
                }
            }
            McpTransport::Stdio { .. } => (top.clone(), Some(String::new())),
            _ => (top.clone(), None),
        };

        let runtime = detect_runtime(&bundle_root);
        let entrypoint = detect_entrypoint(&bundle_root, &srv);

        // Classify files.
        let (kept, dropped) = classify_files(&bundle_root);

        // Normalize transport to use {BUNDLE} placeholder.
        if let McpTransport::Stdio {
            ref mut cwd,
            ref mut command,
            ref mut args,
            ref mut env,
            ref mut bundle,
            ..
        } = srv.transport
        {
            *bundle = Some(bundle_name.clone());
            *cwd = Some("{BUNDLE}".to_string());
            for (key, value) in env.iter_mut() {
                *value = normalize_bundle_env_value(key, value, &bundle_root);
            }
            if runtime == McpRuntime::Python {
                if *command != "python" && *command != "python3" && *command != "uv" {
                    *command = "python".to_string();
                }
                if let Some(ep) = &entrypoint {
                    *args = vec![ep.clone()];
                } else {
                    for arg in args.iter_mut() {
                        if let Some(normalized) = normalize_pathish_value(arg, &bundle_root) {
                            *arg = normalized;
                        }
                    }
                }
            }
            if runtime == McpRuntime::Node {
                if let Some(ep) = &entrypoint {
                    if *command == "node" || *command == "npx" {
                        *args = vec![ep.clone()];
                    }
                }
            }
        }

        let (auth_mode, detected_secrets) = detect_auth(&srv);
        srv.source = Some(McpSource {
            owner: real_owner.to_string(),
            repo: real_repo.to_string(),
            r#ref: Some(resolved_ref.clone()),
            subdir: bundle_source.as_ref().filter(|s| !s.is_empty()).cloned(),
            commit: Some(commit.clone()),
        });
        srv.runtime = Some(runtime);
        srv.auth_mode = auth_mode;
        srv.targets = crate::mcp::adapters::SUPPORTED
            .iter()
            .map(|ide| ide.to_string())
            .collect();

        let mut srv_warnings = Vec::new();
        if entrypoint.is_none() {
            srv_warnings.push(format!(
                "Could not detect entry point for server `{}`",
                srv.name
            ));
        }
        if !detected_secrets.is_empty() {
            srv_warnings.push(format!(
                "{} token(s) detected in config; will be saved to Vault on import",
                detected_secrets.len()
            ));
        }

        preview_servers.push(PreviewServer {
            server: srv,
            bundle_source,
            kept_files: kept,
            dropped_files: dropped,
            entrypoint,
            detected_secrets,
            warnings: srv_warnings,
        });
    }

    Ok(McpPreview {
        owner: real_owner,
        repo: real_repo,
        r#ref: resolved_ref,
        commit,
        servers: preview_servers,
        warnings,
    })
}

fn normalize_github_source(
    owner: &str,
    repo: &str,
    r#ref: Option<&str>,
    subdir: Option<&str>,
) -> Result<(String, String, Option<String>, Option<String>)> {
    use crate::skills::model::apply_github_proxy_env;
    use crate::skills::SkillSource;

    let input = format!("{owner}/{repo}");
    let canonical = apply_github_proxy_env(&input);
    let SkillSource::GitHub {
        owner,
        repo,
        r#ref: parsed_ref,
        subdir: parsed_subdir,
    } = SkillSource::parse_github(canonical)
        .ok_or_else(|| Error::Invalid("expected owner/repo or GitHub URL".into()))?
    else {
        return Err(Error::Invalid("expected GitHub source".into()));
    };
    Ok((
        owner,
        repo,
        r#ref.map(str::to_string).or(parsed_ref),
        subdir.map(str::to_string).or(parsed_subdir),
    ))
}

/// Walk a directory and classify all files as kept or dropped.
fn classify_files(root: &Path) -> (Vec<String>, Vec<String>) {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    let walker = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in walker {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if should_keep_file(&rel_str) {
            kept.push(rel_str);
        } else {
            dropped.push(rel_str);
        }
    }
    kept.sort();
    dropped.sort();
    (kept, dropped)
}

// ─── Import (confirm) ───────────────────────────────────────────────────────

/// Materialise a confirmed preview: save servers to registry, copy bundle files,
/// store detected tokens in Vault. `selected` is a set of server names the user
/// wants to import (use all names from preview if None).
pub async fn import_github_mcp(
    preview: &McpPreview,
    selected: Option<&BTreeSet<String>>,
) -> Result<Vec<String>> {
    let client = crate::skills::github::make_client()?;

    // Re-download at the same commit SHA to guarantee version match.
    let zip_bytes = crate::skills::github::download_zip(
        &client,
        &preview.owner,
        &preview.repo,
        &preview.commit,
        "mcp-import",
    )
    .await?;

    let staging = tempfile::tempdir()?;
    zip::ZipArchive::new(Cursor::new(&zip_bytes))?.extract(staging.path())?;
    let top = find_single_top_dir(staging.path())?;

    let mut reg = McpRegistry::load()?;
    let mut imported = Vec::new();

    for ps in &preview.servers {
        if let Some(sel) = selected {
            if !sel.contains(&ps.server.name) {
                continue;
            }
        }

        let mut server = ps.server.clone();

        // Save detected secrets and replace raw values with ${secret:...}.
        vault_secrets(&mut server, &ps.detected_secrets);
        if !ps.detected_secrets.is_empty() {
            server.auth_mode = McpAuthMode::SecretRef;
        }

        // Copy filtered bundle files.
        if let McpTransport::Stdio {
            bundle: Some(ref bundle_name),
            cwd: Some(_),
            ..
        } = &server.transport
        {
            let bundle_root = ps
                .bundle_source
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| top.join(s))
                .unwrap_or_else(|| top.clone());

            let dest = bundles::bundle_path(bundle_name)?;
            if dest.exists() {
                let _ = crate::fs_util::move_to_trash(&dest, &format!("mcp-bundle-{bundle_name}"));
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::create_dir_all(&dest)?;

            // Copy only kept files.
            copy_filtered_files(&bundle_root, &dest, &ps.kept_files)?;
        }

        reg.upsert(server.clone());
        imported.push(server.name.clone());
    }

    reg.save()?;
    Ok(imported)
}

/// Copy only the listed files from `src` to `dest`, preserving directory structure.
fn copy_filtered_files(src: &Path, dest: &Path, kept: &[String]) -> Result<()> {
    for rel in kept {
        let from = src.join(rel);
        let to = dest.join(rel);
        if !from.is_file() {
            continue;
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&from, &to)?;
    }
    Ok(())
}

fn find_single_top_dir(dir: &Path) -> Result<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    if entries.len() == 1 {
        Ok(entries.remove(0))
    } else {
        Ok(dir.to_path_buf())
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_keeps_python_sources() {
        assert!(should_keep_file("server.py"));
        assert!(should_keep_file("main.py"));
        assert!(should_keep_file("src/handler.py"));
        assert!(should_keep_file("utils/helpers.py"));
    }

    #[test]
    fn filter_keeps_dependency_files() {
        assert!(should_keep_file("requirements.txt"));
        assert!(should_keep_file("pyproject.toml"));
        assert!(should_keep_file("package.json"));
        assert!(should_keep_file("uv.lock"));
        assert!(should_keep_file(".env.example"));
    }

    #[test]
    fn filter_drops_docs_and_tests() {
        assert!(!should_keep_file("tests/test_server.py"));
        assert!(!should_keep_file("docs/guide.md"));
        assert!(!should_keep_file("examples/demo.py"));
        assert!(!should_keep_file("node_modules/foo/index.js"));
        assert!(!should_keep_file("__pycache__/server.cpython-312.pyc"));
    }

    #[test]
    fn filter_drops_root_noise() {
        assert!(!should_keep_file("README.md"));
        assert!(!should_keep_file("LICENSE"));
        assert!(!should_keep_file("AGENTS.md"));
        assert!(!should_keep_file(".gitignore"));
        assert!(!should_keep_file("Dockerfile"));
    }

    #[test]
    fn filter_keeps_nested_source() {
        assert!(should_keep_file("core/auth.py"));
        assert!(should_keep_file("lib/parser.js"));
        assert!(should_keep_file("src/index.ts"));
        assert!(should_keep_file("tools/fetch.py"));
    }

    #[test]
    fn token_detection() {
        assert!(looks_like_token("ghp_abc123def456ghi789jkl012mno345pqr678"));
        assert!(looks_like_token("sk-proj-abcdefghij1234567890abcdefghij12"));
        assert!(looks_like_token("xoxb-123456-789012-abcdef"));
        assert!(!looks_like_token("${secret:MY_KEY}"));
        assert!(!looks_like_token(""));
        assert!(!looks_like_token("short"));
    }

    #[test]
    fn normalize_github_source_accepts_full_https_url() {
        let (owner, repo, r#ref, subdir) = normalize_github_source(
            "https:",
            "/github.com/GongRzhe/Office-PowerPoint-MCP-Server.git",
            Some("main"),
            None,
        )
        .unwrap();
        assert_eq!(owner, "GongRzhe");
        assert_eq!(repo, "Office-PowerPoint-MCP-Server");
        assert_eq!(r#ref.as_deref(), Some("main"));
        assert_eq!(subdir, None);
    }

    #[test]
    fn detect_entrypoint_accepts_absolute_config_args() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("ppt_mcp_server.py"), "print('ok')\n").unwrap();
        let server = McpServer {
            name: "ppt".into(),
            transport: McpTransport::Stdio {
                command: "/repo/.venv/bin/python".into(),
                args: vec!["/repo/Office-PowerPoint-MCP-Server/ppt_mcp_server.py".into()],
                env: Default::default(),
                cwd: None,
                bundle: None,
            },
            targets: vec![],
            description: None,
            tags: vec![],
            disabled: false,
            source: None,
            runtime: None,
            auth_mode: Default::default(),
        };

        assert_eq!(
            detect_entrypoint(root.path(), &server).as_deref(),
            Some("ppt_mcp_server.py")
        );
    }

    #[test]
    fn normalize_bundle_env_reanchors_foreign_paths() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("slide_layout_templates.json"), "{}\n").unwrap();

        assert_eq!(
            normalize_bundle_env_value(
                "PYTHONPATH",
                "/Users/gongzhe/GitRepos/Office-PowerPoint-MCP-Server",
                root.path()
            ),
            "{BUNDLE}"
        );
        assert_eq!(
            normalize_bundle_env_value(
                "PPT_TEMPLATE_PATH",
                "/Users/gongzhe/GitRepos/Office-PowerPoint-MCP-Server/templates",
                root.path()
            ),
            "{BUNDLE}/templates"
        );
        assert_eq!(
            normalize_bundle_env_value(
                "TEMPLATE_FILE_PATH",
                "/Users/gongzhe/GitRepos/Office-PowerPoint-MCP-Server/slide_layout_templates.json",
                root.path()
            ),
            "{BUNDLE}/slide_layout_templates.json"
        );
    }
}
