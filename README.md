# aiem

AI Extension Manager.

aiem manages AI coding IDE Skills, MCP servers, project profiles, secrets, and backups. It includes a command line tool, a native desktop GUI, and a browser-based Web UI.

![Skills page](pic/skillspage.png)

- [中文说明](#中文说明)
- [English](#english)

## 中文说明

### 当前版本

当前初始版本标记为 `v0.1.0`。

### 包含内容

- `aiem`：命令行工具，同时内置 Web UI 服务。
- `aiem-gui`：基于 egui 的原生桌面端。
- Web UI：通过 `aiem serve` 启动的浏览器管理界面。

### 支持的 IDE

- Claude Code
- Codex
- Cursor
- Copilot
- Windsurf
- Trae
- Qoder
- Kiro

### 主要功能

- 从 GitHub 或本地目录安装 Skill。
- 将 Skill 部署到全局或指定项目。
- 添加、部署、移除、启用、禁用和打包 MCP 服务。
- 按 IDE 和项目范围部署 MCP 服务。
- 创建项目配置，将 IDE、Skills 和 MCP 服务组合到同一个工作区。
- 使用系统密钥环保存密钥，并通过 `${secret:NAME}` 引用。
- 扫描本地 IDE 配置，发现并导入已有资源。
- 创建本地快照，或同步到 GitHub 备份仓库。
- 在桌面端和 Web UI 中切换中文/英文界面。

### 页面预览

![Skills 页面](pic/skillspage.png)

![发现页面](pic/discoverpage.png)

### 安装

推荐使用一键安装脚本。Windows 会打开正常的安装向导，可以选择安装目录；macOS 会安装 `.pkg`；Debian/Ubuntu 会安装 `.deb`。

#### Windows

```powershell
irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex
```

安装向导会安装：

- 桌面端：`aiem-gui.exe`
- 命令行：`aiem.exe`
- 开始菜单快捷方式
- 可选桌面快捷方式
- 可选用户 `PATH`

默认安装目录：

```text
%LOCALAPPDATA%\Programs\aiem
```

如果只想使用便携版 zip 安装到用户目录：

```powershell
$env:AIEM_INSTALL_MODE="portable"
irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex
```

#### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
```

脚本会根据芯片架构下载对应的 `.pkg`：

- Apple Silicon：`aiem-*-macos-arm64.pkg`
- Intel Mac：`aiem-*-macos-x86_64.pkg`

安装后：

- 桌面端位于 `/Applications/aiem.app`
- 命令行位于 `/usr/local/bin/aiem`

如果遇到未签名应用提示，请在系统设置的隐私与安全中允许打开，或右键应用选择打开。

#### Linux

Debian/Ubuntu 推荐：

```bash
curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
```

脚本会优先安装：

```text
aiem-*-linux-x86_64-gnu.deb
```

安装后：

- 命令行位于 `/usr/bin/aiem`
- 桌面端位于 `/usr/bin/aiem-gui`
- 桌面菜单会出现 `aiem`

非 Debian 系发行版或服务器环境可以使用便携模式：

```bash
AIEM_INSTALL_MODE=portable curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
```

便携模式会把 `aiem` 安装到：

```text
~/.local/bin
```

#### 直接下载发布包

GitHub Release 会提供以下产物：

- `aiem-*-windows-x86_64-setup.exe`：Windows 安装向导。
- `aiem-*-windows-x86_64.zip`：Windows 便携包。
- `aiem-*-macos-arm64.pkg`：Apple Silicon macOS 安装包。
- `aiem-*-macos-x86_64.pkg`：Intel macOS 安装包。
- `aiem-*-linux-x86_64-gnu.deb`：Debian/Ubuntu 安装包。
- `aiem-*-linux-x86_64-gnu.tar.gz`：Linux glibc 便携包。
- `aiem-*-linux-x86_64-musl.tar.gz`：Linux musl 便携包。

zip/tar 包是便携版，不会自动注册系统快捷方式。Windows 便携包里请启动 `Launch aiem GUI.cmd` 或 `aiem-gui.exe`，不要双击 `aiem.exe`，因为 `aiem.exe` 是命令行程序。

### 从源码构建

```powershell
git clone git@github.com:Vaxspark/aiem.git
cd aiem
cargo build --release -p aiem-cli --features web
cargo build --release -p aiem-gui
```

构建产物：

```text
target\release\aiem.exe
target\release\aiem-gui.exe
```

### 快速开始

```powershell
aiem init
aiem ide
aiem skill list
aiem mcp list
aiem serve --open
```

添加并部署一个 Skill：

```powershell
aiem skill add my-skill --url https://github.com/owner/repo
aiem skill deploy my-skill codex
```

添加并同步一个 MCP 服务：

```powershell
aiem mcp add fetch --type stdio --cmd uvx --arg mcp-server-fetch
aiem mcp sync
```

启动 Web UI：

```powershell
aiem serve --host 127.0.0.1 --port 8787 --open
```

启动桌面端：

```powershell
aiem-gui
```

### 项目配置

项目配置用于描述某个工作目录应该启用哪些 IDE、哪些 Skills 和哪些 MCP 服务。

- `仅保存`：只更新项目记录。
- `保存并部署`：保存记录，并立即把选中的 Skills/MCP 服务部署到项目。
- `同步`：按照已经保存的项目记录重新同步到工作区。

### 密钥

密钥值保存在系统密钥环中，不会提交到仓库。MCP 配置可以通过下面的形式引用密钥：

```text
${secret:NAME}
```

### 备份

```powershell
aiem backup snapshot
aiem backup export <DEST_DIR>
aiem backup import <SRC_DIR>
aiem backup push --repo https://github.com/owner/backup-repo
aiem backup pull --repo https://github.com/owner/backup-repo
```

GitHub token 可以通过 `GITHUB_TOKEN`、设置页或系统密钥环提供。

### 隐私和本地文件

仓库会忽略本地运行文件和编译产物，例如 `.agents/`、`.codex-preview/`、`.cursor/`、`.tmp-*`、`.env`、`target/` 和 `dist/`。上传 GitHub 前不要使用 `git add -f` 强制加入这些本地文件。

### 开发

格式化和检查：

```powershell
cargo fmt
cargo check --workspace
```

发布构建：

```powershell
cargo build --release -p aiem-cli --features web -p aiem-gui
```

本地运行：

```powershell
cargo run -p aiem-cli --features web -- serve --port 8787 --open
cargo run -p aiem-gui
```

## English

### Current Version

The initial release tag is `v0.1.0`.

### What Is Included

- `aiem`: command line tool with the embedded Web UI server.
- `aiem-gui`: native desktop GUI built with egui.
- Web UI: browser-based management launched with `aiem serve`.

### Supported IDE Targets

- Claude Code
- Codex
- Cursor
- Copilot
- Windsurf
- Trae
- Qoder
- Kiro

### Features

- Install Skills from GitHub or local folders.
- Deploy Skills globally or into project workspaces.
- Add, deploy, remove, enable, disable, and bundle MCP servers.
- Deploy MCP servers per IDE and per project.
- Create project profiles that combine IDE targets, Skills, and MCP servers.
- Store secrets in the OS keyring and reference them with `${secret:NAME}`.
- Discover existing local IDE configuration and import unmanaged resources.
- Create local snapshots or sync backups to a GitHub repository.
- Switch the desktop GUI and Web UI between English and Chinese.

### Screenshots

![Skills page](pic/skillspage.png)

![Discover page](pic/discoverpage.png)

### Installation

The recommended path is the one-line installer. Windows opens a normal setup wizard with an install directory selector; macOS installs a `.pkg`; Debian/Ubuntu installs a `.deb`.

#### Windows

```powershell
irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex
```

The setup wizard installs:

- Desktop app: `aiem-gui.exe`
- Command line: `aiem.exe`
- Start Menu shortcuts
- Optional desktop shortcut
- Optional user `PATH` entry

Default install directory:

```text
%LOCALAPPDATA%\Programs\aiem
```

To use the portable zip installer instead:

```powershell
$env:AIEM_INSTALL_MODE="portable"
irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex
```

#### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
```

The script downloads the matching `.pkg`:

- Apple Silicon: `aiem-*-macos-arm64.pkg`
- Intel Mac: `aiem-*-macos-x86_64.pkg`

After installation:

- Desktop app: `/Applications/aiem.app`
- Command line: `/usr/local/bin/aiem`

If macOS blocks the unsigned app, allow it in Privacy & Security or right-click the app and choose Open.

#### Linux

For Debian/Ubuntu:

```bash
curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
```

The script prefers:

```text
aiem-*-linux-x86_64-gnu.deb
```

After installation:

- Command line: `/usr/bin/aiem`
- Desktop app: `/usr/bin/aiem-gui`
- App menu entry: `aiem`

For non-Debian distributions or server environments, use portable mode:

```bash
AIEM_INSTALL_MODE=portable curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
```

Portable mode installs `aiem` into:

```text
~/.local/bin
```

#### Release Assets

GitHub Releases provide:

- `aiem-*-windows-x86_64-setup.exe`: Windows setup wizard.
- `aiem-*-windows-x86_64.zip`: Windows portable package.
- `aiem-*-macos-arm64.pkg`: Apple Silicon macOS package.
- `aiem-*-macos-x86_64.pkg`: Intel macOS package.
- `aiem-*-linux-x86_64-gnu.deb`: Debian/Ubuntu package.
- `aiem-*-linux-x86_64-gnu.tar.gz`: Linux glibc portable package.
- `aiem-*-linux-x86_64-musl.tar.gz`: Linux musl portable package.

zip/tar packages are portable and do not register shortcuts automatically. On Windows, launch `Launch aiem GUI.cmd` or `aiem-gui.exe`; do not double-click `aiem.exe`, because it is the command line tool.

### Build From Source

```powershell
git clone git@github.com:Vaxspark/aiem.git
cd aiem
cargo build --release -p aiem-cli --features web
cargo build --release -p aiem-gui
```

Outputs:

```text
target\release\aiem.exe
target\release\aiem-gui.exe
```

### Quick Start

```powershell
aiem init
aiem ide
aiem skill list
aiem mcp list
aiem serve --open
```

Add and deploy a Skill:

```powershell
aiem skill add my-skill --url https://github.com/owner/repo
aiem skill deploy my-skill codex
```

Add and sync an MCP server:

```powershell
aiem mcp add fetch --type stdio --cmd uvx --arg mcp-server-fetch
aiem mcp sync
```

Start the Web UI:

```powershell
aiem serve --host 127.0.0.1 --port 8787 --open
```

Launch the desktop GUI:

```powershell
aiem-gui
```

### Project Profiles

Project profiles describe which IDEs, Skills, and MCP servers should be active for one workspace path.

- `Save only`: update the project record only.
- `Save & Deploy`: save the record and deploy the selected Skills/MCP servers.
- `Sync`: re-apply the already saved project record to the workspace.

### Secrets

Secret values are stored in the OS keyring and are not committed to the repository. MCP configuration can reference secrets with:

```text
${secret:NAME}
```

### Backup

```powershell
aiem backup snapshot
aiem backup export <DEST_DIR>
aiem backup import <SRC_DIR>
aiem backup push --repo https://github.com/owner/backup-repo
aiem backup pull --repo https://github.com/owner/backup-repo
```

GitHub tokens can be provided through `GITHUB_TOKEN`, the Settings page, or the OS keyring.

### Privacy And Local Files

The repository ignores local runtime files and build outputs such as `.agents/`, `.codex-preview/`, `.cursor/`, `.tmp-*`, `.env`, `target/`, and `dist/`. Do not use `git add -f` to force-add those local files before publishing.

### Development

Format and check:

```powershell
cargo fmt
cargo check --workspace
```

Release builds:

```powershell
cargo build --release -p aiem-cli --features web -p aiem-gui
```

Run locally:

```powershell
cargo run -p aiem-cli --features web -- serve --port 8787 --open
cargo run -p aiem-gui
```
