# aiem — AI Extension Manager

> Unified skills & MCP server management across all your AI-powered IDEs.

[![Release](https://img.shields.io/github/v/release/Vaxspark/aiem)](https://github.com/Vaxspark/aiem/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

---

[English](#english) | [中文](#中文)

---

## English

### What is aiem?

**aiem** is a cross-platform command-line (and GUI) tool that lets you manage:

- **AI Skills** — prompt packs / instruction files deployed into IDE config directories
- **MCP Servers** — Model Context Protocol server registrations synced across IDEs
- **Secrets** — API keys and tokens stored in the OS keyring, injected into MCP env vars at runtime
- **Profiles** — named subsets of skills/MCPs you can switch between instantly
- **Backup & Restore** — local snapshots and GitHub-backed configuration backup

It targets **Cursor**, **Windsurf**, **VS Code**, **Zed**, and other IDEs that support `.cursor/rules`, `.windsurf/rules`, or `mcp.json`-style configuration.

A lightweight **Web UI** (`aiem serve`) is included for headless remote management over SSH port forwarding — no Node.js, no public port.

---

### Installation

#### One-line install (recommended)

**Linux / macOS:**
```sh
curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex
```

The script downloads the latest binary, places it in `~/.local/bin` (Linux/macOS) or `%LOCALAPPDATA%\aiem` (Windows), and adds it to your `PATH` automatically.

#### Manual download

If you prefer, grab the binary directly from the [Releases page](https://github.com/Vaxspark/aiem/releases):

| Platform | Binary |
|---|---|
| Windows x86_64 | `aiem-windows-x86_64.zip` |
| Linux x86_64 (musl, static) | `aiem-linux-x86_64-musl.tar.gz` |

#### Build from source

```bash
git clone https://github.com/Vaxspark/aiem.git
cd aiem
cargo build --release -p aiem-cli
# binary at target/release/aiem (or aiem.exe on Windows)
```

For the Web UI feature:

```bash
cargo build --release --features web -p aiem-cli
```

---

### Quick Start

```bash
# 1. Initialise the ~/.aiem home directory
aiem init

# 2. Add a skill from a GitHub repo
aiem skill add my-skill --url https://github.com/you/skills#main

# 3. Deploy skill to Cursor (writes .cursor/rules/)
aiem skill deploy my-skill cursor

# 4. Register an MCP server
aiem mcp add my-server --type stdio --cmd uvx --arg MCP-Server-Fetch

# 5. Sync MCP servers to all supported IDEs
aiem mcp sync

# 6. Manage secrets (stored in OS keyring)
aiem secret set MY_API_KEY
```

---

### CLI Reference

```
aiem <COMMAND>

Commands:
  init       Initialise the aiem home directory (~/.aiem)
  ide        List supported IDEs
  skill      Add, remove, deploy/undeploy, and list AI skills
  mcp        Add, remove, sync, deploy/undeploy MCP servers
  secret     Store and retrieve secrets from the OS keyring
  profile    Create and switch between named skill/MCP profiles
  discover   Scan this machine for existing skills & MCP configs
  backup     Local snapshots and GitHub backup/restore
  serve      Start the Web UI (requires --features web build)
```

#### `skill` subcommands

```
aiem skill add <NAME> --url <GITHUB_URL>   # register from GitHub
aiem skill add <NAME> --path <DIR>          # register from local directory
aiem skill deploy <NAME> <IDE>              # write skill files into IDE config
aiem skill undeploy <NAME> <IDE>            # remove skill files from IDE config
aiem skill sync                             # re-deploy all skills to all IDEs
aiem skill list                             # list registered skills
aiem skill remove <NAME>                    # remove skill registration
```

#### `mcp` subcommands

```
aiem mcp add <NAME> --type stdio --cmd <CMD> [--arg <ARG>]…
aiem mcp add <NAME> --type sse  --url <URL>
aiem mcp sync                               # write to all registered IDE configs
aiem mcp deploy <NAME> --project <PATH>     # attach server to one project
aiem mcp undeploy <NAME> --project <PATH>   # detach server from one project
aiem mcp list
aiem mcp remove <NAME>
aiem mcp supported                          # list IDE targets
```

#### `backup` subcommands

```
aiem backup snapshot                        # local timestamped snapshot
aiem backup export <DEST_DIR>               # zip export to explicit path
aiem backup import <SRC_DIR>                # restore from snapshot or export
aiem backup push [--repo <URL>] [--token <PAT>]  # commit & push to GitHub
aiem backup pull [--repo <URL>] [--token <PAT>]  # pull & restore from GitHub
aiem backup status                          # show config and last backup time
```

---

### Web UI

Start the server locally:

```bash
aiem serve --open          # opens http://127.0.0.1:8787 in your browser
```

For headless servers (SSH port forwarding):

```bash
# On the server:
aiem serve                 # binds to 127.0.0.1:8787

# On your laptop:
ssh -L 8787:localhost:8787 user@yourserver
# then open http://localhost:8787
```

#### Systemd user service

```bash
cp aiem-user.service ~/.config/systemd/user/aiem.service
systemctl --user daemon-reload
systemctl --user enable --now aiem
```

---

### GUI

A native desktop GUI is available as a separate binary (`aiem-gui`). It provides the same skill/MCP management as the CLI with a graphical interface built on [egui](https://github.com/emilk/egui).

---

## 中文

### aiem 是什么？

**aiem**（AI Extension Manager）是一款跨平台命令行工具（附带 GUI），用于统一管理：

- **AI Skills（提示包）** — 部署到 IDE 配置目录的提示文件 / 指令集
- **MCP 服务器** — 跨 IDE 同步的 Model Context Protocol 服务器注册信息
- **Secrets（密钥）** — 保存在系统钥匙串中的 API Key，在 MCP 运行时自动注入
- **Profiles（配置集）** — 可随时切换的命名 skill/MCP 子集
- **备份与还原** — 本地快照 + GitHub 远程备份

支持的 IDE 包括 **Cursor**、**Windsurf**、**VS Code**、**Zed** 等。

同时内置轻量级 **Web UI**（`aiem serve`），可通过 SSH 端口转发远程管理，无需 Node.js，无需公网端口。

---

### 安装

#### 一键安装（推荐）

**Linux / macOS：**
```sh
curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
```

**Windows（PowerShell）：**
```powershell
irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex
```

脚本会自动下载最新版二进制并配置好 PATH，运行完即可直接使用 `aiem` 命令。

#### 手动下载

也可以从 [Releases 页面](https://github.com/Vaxspark/aiem/releases) 直接下载：

| 平台 | 文件 |
|---|---|
| Windows x86_64 | `aiem-windows-x86_64.zip` |
| Linux x86_64（musl 静态链接） | `aiem-linux-x86_64-musl.tar.gz` |

#### 源码编译

```bash
git clone https://github.com/Vaxspark/aiem.git
cd aiem
cargo build --release -p aiem-cli
```

如需 Web UI：

```bash
cargo build --release --features web -p aiem-cli
```

---

### 快速上手

```bash
# 初始化 ~/.aiem 目录
aiem init

# 从 GitHub 添加 skill
aiem skill add my-skill --url https://github.com/you/skills#main

# 部署到 Cursor
aiem skill deploy my-skill cursor

# 注册 MCP 服务器
aiem mcp add my-server --type stdio --cmd uvx --arg MCP-Server-Fetch

# 同步到所有已注册 IDE
aiem mcp sync

# 存储密钥（保存在系统钥匙串）
aiem secret set MY_API_KEY
```

---

### 主要命令速览

| 命令 | 说明 |
|---|---|
| `aiem init` | 初始化 `~/.aiem` 目录 |
| `aiem skill add/deploy/sync` | 管理 AI skills |
| `aiem mcp add/sync/deploy` | 管理 MCP 服务器 |
| `aiem secret set/get` | 管理密钥 |
| `aiem profile create/switch` | 管理配置集 |
| `aiem discover` | 自动发现本机已有配置 |
| `aiem backup snapshot/push/pull` | 备份与还原 |
| `aiem serve` | 启动 Web UI |

---

### Web UI 远程访问

```bash
# 服务器端
aiem serve          # 监听 127.0.0.1:8787

# 本机
ssh -L 8787:localhost:8787 user@yourserver
# 然后打开 http://localhost:8787
```

#### Systemd 用户服务

```bash
cp aiem-user.service ~/.config/systemd/user/aiem.service
systemctl --user daemon-reload
systemctl --user enable --now aiem
```

---

### 备份功能

```bash
# 本地快照
aiem backup snapshot

# 导出为 zip
aiem backup export ~/my-backup.zip

# 推送到 GitHub 私有仓库
aiem backup push --repo https://github.com/you/aiem-backup --token <PAT>

# 从 GitHub 恢复
aiem backup pull --repo https://github.com/you/aiem-backup

# 查看备份状态
aiem backup status
```

> **提示**：GitHub token 也可通过 `GITHUB_TOKEN` 环境变量或 Web UI 的 Settings 页面设置，无需每次在命令行中传入。

---

## License

MIT — see [LICENSE](LICENSE).
