# aiem

**AI Extension Manager** — a single, cross-platform Rust CLI that unifies two
messy pieces of the AI-IDE ecosystem:

1. **Skills** — downloadable prompt/skill packages, synced from **GitHub** into a
   central local repo and symlinked into each IDE's skills directory.
2. **MCP servers** — one source of truth for every MCP server you run, written
   back into **Codex**, **Claude Code** and **GitHub Copilot (VS Code)** with
   one command.

No GUI, no Electron — just a fast Rust binary and plain config files on disk.

---

## Install (after scaffolding)

Requires Rust (stable, `>= 1.75`). Install via [`rustup`](https://rustup.rs).

```bash
cargo build --release
# binary: target/release/aiem (aiem.exe on Windows)
```

Or install to `~/.cargo/bin`:

```bash
cargo install --path crates/aiem-cli
```

First-time setup:

```bash
aiem init        # creates ~/.aiem/{skills,mcp,backups,cache}
aiem ide list    # show supported IDEs for skills
aiem mcp supported   # show IDEs with MCP sync support
```

Set `AIEM_HOME` to override the default `~/.aiem`. Set `GITHUB_TOKEN` to avoid
GitHub API rate limiting.

---

## Skills

Skills live in `~/.aiem/skills/<owner>__<repo>[__<subdir>]/` and are tracked in
`~/.aiem/skills/index.json`. Deploying a skill creates a **symlink** (or a
junction / copy fallback on Windows) inside the IDE's expected directory.

### Add a skill from GitHub

```bash
# Full repo
aiem skill add anthropics/skills

# Specific subdir, pinned to a tag
aiem skill add anthropics/skills//writing@v1.2.3

# Or via URL
aiem skill add https://github.com/anthropics/skills
```

### Update, remove, list

```bash
aiem skill list
aiem skill info anthropics__skills
aiem skill update anthropics__skills
aiem skill remove anthropics__skills
```

### Deploy into IDEs

```bash
# User scope (e.g. Claude Code ~/.claude/skills)
aiem skill deploy anthropics__skills --ide claude-code

# Project scope (e.g. VS Code .github/skills)
aiem skill deploy anthropics__skills --ide vscode --project E:\code\my-project

# Multiple IDEs at once
aiem skill sync anthropics__skills --ides claude-code,cursor,vscode --project .

# Remove
aiem skill undeploy anthropics__skills --ide claude-code
```

Supported IDE skill targets (see `aiem ide list`):

| ID | Directory |
|---|---|
| `claude-code` | `.claude/skills` |
| `codex` | `.codex/skills` |
| `cursor` | `.cursor/skills` |
| `vscode` | `.github/skills` |
| `windsurf` | `.windsurf/skills` |
| `trae` | `.trae/skills` |
| `qoder` | `.qoder/skills` |
| `kiro` | `.kiro/skills` |

---

## MCP

aiem keeps one list of MCP servers in `~/.aiem/mcp/servers.json`, then writes
each targeted IDE's native config file. Existing keys in those files are
preserved; aiem only owns the servers it knows about.

### Supported IDEs (Phase 1)

| IDE | Scope | Native config |
|---|---|---|
| `codex` | User | `~/.codex/config.toml` (`[mcp_servers.*]`) |
| `claude-code` | User / Project | `~/.claude.json` / `.mcp.json` (`mcpServers`) |
| `copilot` | User / Project | `$APPDATA/Code/User/mcp.json` or `.vscode/mcp.json` (`servers`) |

### Register a server

```bash
# stdio server, targets all three IDEs
aiem mcp add filesystem \
  --type stdio \
  --command npx --arg -y --arg @modelcontextprotocol/server-filesystem --arg C:\workspace \
  --env LOG_LEVEL=info \
  --target codex,claude-code,copilot \
  --description "local filesystem browser"

# HTTP server (works for claude-code / copilot, skipped on codex)
aiem mcp add remote-tool \
  --type http \
  --url https://mcp.example.com/sse \
  --header Authorization=Bearer\ xxx \
  --target claude-code,copilot
```

### Sync

```bash
aiem mcp list
aiem mcp show filesystem

# Dry-run first
aiem mcp sync --dry-run

# Write to all targets (user scope)
aiem mcp sync

# Only Codex
aiem mcp sync --ide codex

# Project-scoped Copilot / Claude Code
aiem mcp sync --ide copilot,claude-code --project .
```

Every write is preceded by a timestamped backup under
`~/.aiem/backups/<ide>/<timestamp>/`, so a bad sync is always recoverable.

### Enable / disable / retarget

```bash
aiem mcp disable filesystem
aiem mcp enable filesystem
aiem mcp target add filesystem codex
aiem mcp target remove filesystem copilot
aiem mcp remove filesystem
```

### Import existing servers

Already have a bunch of servers configured in Codex? Pull them into aiem:

```bash
aiem mcp import --from codex
aiem mcp list
```

### Where is the config file?

```bash
aiem mcp path --ide codex
aiem mcp path --ide copilot --project .
```

---

## Project layout

```
aiem/
├── Cargo.toml                     # workspace
├── rust-toolchain.toml
└── crates/
    ├── aiem-core/                 # library: models, fs, github, adapters
    │   └── src/
    │       ├── lib.rs
    │       ├── error.rs
    │       ├── paths.rs
    │       ├── fs_util.rs         # symlink / junction / atomic write / backup
    │       ├── ide.rs             # IDE targets for skills
    │       ├── skills/
    │       │   ├── model.rs       # Skill, SkillSource
    │       │   ├── registry.rs    # index.json
    │       │   ├── github.rs      # zipball download + extract
    │       │   └── install.rs     # deploy / undeploy
    │       └── mcp/
    │           ├── model.rs       # McpServer, McpTransport
    │           ├── registry.rs    # servers.json
    │           ├── sync.rs        # plan + execute
    │           └── adapters/
    │               ├── codex.rs
    │               ├── claude_code.rs
    │               └── copilot.rs
    └── aiem-cli/                  # binary: clap front-end
        └── src/
            ├── main.rs
            └── commands/{ide,skill,mcp}.rs
```

---

## Roadmap

- **Phase 1 (now):** GitHub skills sync · Codex / Claude Code / Copilot MCP sync
- **Phase 2:** Secret Vault (OS keyring) with `${SECRET:name}` expansion in configs
- **Phase 3:** Profiles — named bundles of skills + MCP servers, one-command switching
- **Phase 4:** More IDEs (Cursor / Windsurf / Cline MCP), `aiem search` against public registries
- **Phase 5:** Optional Tauri GUI over the same core
