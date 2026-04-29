# aiem

AI Extension Manager。aiem 用来统一管理 AI 编程 IDE 的 Skills、MCP 服务、项目配置、密钥和备份数据，同时提供命令行、桌面端和 Web UI 三种入口。

![Skills 页面](pic/skillspage.png)

## 当前版本

当前初始版本标记为 `v0.1.0`。

## 包含内容

- `aiem`：命令行工具，同时内置 Web UI 服务。
- `aiem-gui`：基于 egui 的原生桌面端。
- Web UI：通过 `aiem serve` 启动的浏览器管理界面。

## 支持的 IDE

- Claude Code
- Codex
- Cursor
- Copilot
- Windsurf
- Trae
- Qoder
- Kiro

## 主要功能

- 从 GitHub 或本地目录安装 Skill。
- 将 Skill 部署到全局或指定项目。
- 添加、部署、移除、启用、禁用和打包 MCP 服务。
- 按 IDE 和项目范围部署 MCP 服务。
- 创建项目配置，将 IDE、Skills 和 MCP 服务组合到同一个工作区。
- 使用系统密钥环保存密钥，并通过 `${secret:NAME}` 引用。
- 扫描本地 IDE 配置，发现并导入已有资源。
- 创建本地快照，或同步到 GitHub 备份仓库。
- 在桌面端和 Web UI 中切换中文/英文界面。

## 页面预览

![Skills 页面](pic/skillspage.png)

![发现页面](pic/discoverpage.png)

## 安装

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex
```

安装脚本会把程序放到：

```text
%LOCALAPPDATA%\aiem
```

脚本会同时把安装目录加入用户 `PATH`，如果存在桌面端程序，也会创建开始菜单快捷方式。

## 从源码构建

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

## 快速开始

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

## 项目配置

项目配置用于描述某个工作目录应该启用哪些 IDE、哪些 Skills 和哪些 MCP 服务。

- `仅保存`：只更新项目记录。
- `保存并部署`：保存记录，并立即把选中的 Skills/MCP 服务部署到项目。
- `同步`：按照已经保存的项目记录重新同步到工作区。

## 密钥

密钥值保存在系统密钥环中，不会提交到仓库。MCP 配置可以通过下面的形式引用密钥：

```text
${secret:NAME}
```

## 备份

```powershell
aiem backup snapshot
aiem backup export <DEST_DIR>
aiem backup import <SRC_DIR>
aiem backup push --repo https://github.com/owner/backup-repo
aiem backup pull --repo https://github.com/owner/backup-repo
```

GitHub token 可以通过 `GITHUB_TOKEN`、设置页或系统密钥环提供。

## 隐私和本地文件

仓库会忽略本地运行文件和编译产物，例如 `.agents/`、`.codex-preview/`、`.cursor/`、`.tmp-*`、`.env`、`target/` 和 `dist/`。上传 GitHub 前不要使用 `git add -f` 强制加入这些本地文件。

## 开发

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

## English Summary

aiem is an AI Extension Manager for Skills, MCP servers, project profiles, secrets, and backups across multiple AI coding IDEs. It includes a CLI, a native desktop GUI, and a browser-based Web UI.
