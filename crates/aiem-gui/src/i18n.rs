//! Simple i18n — bilingual string table (English / 简体中文).

use std::cell::RefCell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Zh,
}

impl Default for Lang {
    fn default() -> Self { Lang::En }
}

impl Lang {
    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Zh => "简体中文",
        }
    }
}

thread_local! {
    static CURRENT_LANG: RefCell<Lang> = RefCell::new(Lang::En);
}

pub fn set_lang(lang: Lang) {
    CURRENT_LANG.with(|c| *c.borrow_mut() = lang);
}

pub fn lang() -> Lang {
    CURRENT_LANG.with(|c| *c.borrow())
}

/// Translate a key to the current language. Falls back to the key itself.
pub fn t(key: &str) -> &'static str {
    let l = lang();
    let table = match l {
        Lang::En => EN,
        Lang::Zh => ZH,
    };
    for &(k, v) in table {
        if k == key { return v; }
    }
    // Fallback: search EN
    for &(k, v) in EN {
        if k == key { return v; }
    }
    // Last resort: leak the key (only happens for missing keys)
    ""
}

// ─── String tables ──────────────────────────────────────────────────────────

const EN: &[(&str, &str)] = &[
    // App
    ("app.title", "aiem"),
    ("app.subtitle", "skills & mcp manager"),

    // Sidebar
    ("tab.skills", "Skills"),
    ("tab.mcp", "MCP"),
    ("tab.store", "Store"),
    ("tab.projects", "Projects"),
    ("tab.secrets", "Secrets"),
    ("tab.profiles", "Profiles"),
    ("tab.discover", "Discover"),
    ("tab.ides", "IDEs"),
    ("tab.settings", "Settings"),

    // Skills page
    ("skills.title", "Skills"),
    ("skills.subtitle", "Download from GitHub and link into any supported IDE"),
    ("skills.add", "+ Add from GitHub"),
    ("skills.clear_global", "⚠ Clear All Global"),
    ("skills.filter", "Filter"),
    ("skills.add_title", "Add skill from GitHub"),
    ("skills.add_hint", "Paste a GitHub URL or shorthand: owner/repo · owner/repo//subdir · owner/repo@v1.2"),
    ("skills.source", "Source"),
    ("skills.subdir", "Subdir (opt)"),
    ("skills.ref", "Ref (opt)"),
    ("skills.name", "Name (opt)"),
    ("skills.download", "Download & install"),
    ("skills.cancel", "Cancel"),
    ("skills.remove", "🗑 Remove"),
    ("skills.update", "Update"),
    ("skills.deploy_all", "Deploy All"),
    ("skills.undeploy_all", "Undeploy All"),
    ("skills.remove_all", "Remove All"),
    ("skills.remove_all_confirm_pre", "Delete all"),
    ("skills.remove_all_confirm_post", "skills in this group? This cannot be undone."),
    ("skills.remove_all_ok", "Confirm Delete"),
    ("skills.remove_all_cancel", "Cancel"),
    ("skills.remove_all_done", "skill(s) removed"),
    ("skills.scope_global", "Global"),
    ("skills.link_github", "🔗 Link GitHub"),
    ("skills.deploy", "Deploy"),
    ("skills.undeploy", "Undeploy"),
    ("skills.local", "local"),
    ("skills.empty", "No skills yet"),
    ("skills.empty_sub", "Click \"Add from GitHub\" to pull your first one."),
    ("skills.no_match", "No matches"),
    ("skills.no_match_sub", "Try a different filter."),

    // MCP page
    ("mcp.title", "MCP Servers"),
    ("mcp.subtitle", "One source of truth -- synced to Codex, Claude Code & Copilot"),
    ("mcp.sync", "Sync all"),
    ("mcp.new", "+ New server"),
    ("mcp.register", "Register MCP server"),
    ("mcp.name", "Name"),
    ("mcp.transport", "Transport"),
    ("mcp.command", "Command"),
    ("mcp.args", "Args"),
    ("mcp.env", "Env"),
    ("mcp.url", "URL"),
    ("mcp.headers", "Headers"),
    ("mcp.targets", "Targets"),
    ("mcp.description", "Description"),
    ("mcp.save", "Save"),
    ("mcp.filter", "Filter"),
    ("mcp.empty", "No MCP servers yet"),
    ("mcp.empty_sub", "Click \"New server\" to register one."),
    ("mcp.remove_hint", "remove"),
    ("mcp.enable", "Enable"),
    ("mcp.disable", "Disable"),
    ("mcp.paths", "IDE config paths"),

    // Store page
    ("store.title", "Store"),
    ("store.subtitle", "Search skills & MCP servers from smithery.ai, glama.ai, and claude-plugins.dev"),
    ("store.search_hint", "Search servers & skills..."),
    ("store.search", "Search"),
    ("store.install", "⬇ Install"),
    ("store.copy_url", "📋 Copy URL"),
    ("store.popular", "🔥 Popular"),
    ("store.no_results", "No results"),
    ("store.no_results_sub", "Try different keywords."),
    ("store.loading", "Loading popular..."),

    // Projects page
    ("projects.title", "Projects"),
    ("projects.subtitle", "Per-project skill & MCP deployment — select skills and IDEs for each project"),
    ("projects.add", "+ Add project"),
    ("projects.add_title", "Add project directory"),
    ("projects.path", "Path"),
    ("projects.browse", "Browse…"),
    ("projects.name", "Name"),
    ("projects.add_btn", "Add"),
    ("projects.cancel", "Cancel"),
    ("projects.configure", "Configure"),
    ("projects.target_ides", "Target IDEs"),
    ("projects.target_ides_hint", "Select which IDEs to deploy skills into for this project"),
    ("projects.skills", "Skills"),
    ("projects.skills_hint", "Check skills to deploy"),
    ("projects.mcp", "MCP Servers"),
    ("projects.mcp_hint", "Check servers to configure"),
    ("projects.save_deploy", "Save & Deploy"),
    ("projects.save_only", "Save only"),
    ("projects.sync", "↻ Sync"),
    ("projects.remove", "remove project"),
    ("projects.empty", "No projects yet"),
    ("projects.empty_sub", "Add a project to manage per-project skill deployments across multiple IDEs."),
    ("projects.close", "Close"),
    ("projects.no_skills", "no skills installed"),
    ("projects.no_servers", "no servers registered"),

    // Profiles page
    ("profiles.title", "Profiles"),
    ("profiles.subtitle", "Named overlays -- switch between skill & MCP sets (work / oss / demo...)"),
    ("profiles.new", "+ New profile"),
    ("profiles.active", "Active profile:"),
    ("profiles.no_active", "No active profile"),
    ("profiles.no_active_hint", "-- sync uses the full registry"),
    ("profiles.clear", "Clear"),
    ("profiles.activate", "Activate"),
    ("profiles.edit", "Edit"),
    ("profiles.save", "Save profile"),

    // Settings
    ("settings.title", "Settings"),
    ("settings.subtitle", "Paths, environment, and diagnostics"),
    ("settings.paths", "Paths"),
    ("settings.env", "Environment"),
    ("settings.about", "About"),
    ("settings.language", "Language"),

    // Common
    ("common.save", "Save"),
    ("common.clear", "Clear"),
    ("common.cancel", "Cancel"),
    ("common.close", "Close"),
    ("common.delete", "Delete"),
    ("common.copy", "copy"),
];

const ZH: &[(&str, &str)] = &[
    // App
    ("app.title", "aiem"),
    ("app.subtitle", "技能与MCP管理器"),

    // Sidebar
    ("tab.skills", "技能"),
    ("tab.mcp", "MCP"),
    ("tab.store", "商店"),
    ("tab.projects", "项目"),
    ("tab.secrets", "密钥"),
    ("tab.profiles", "配置"),
    ("tab.discover", "发现"),
    ("tab.ides", "IDE"),
    ("tab.settings", "设置"),

    // Skills page
    ("skills.title", "技能"),
    ("skills.subtitle", "从GitHub下载并链接到任意支持的IDE"),
    ("skills.add", "+ 从GitHub添加"),
    ("skills.clear_global", "⚠ 清除全局部署"),
    ("skills.filter", "筛选"),
    ("skills.add_title", "从GitHub添加技能"),
    ("skills.add_hint", "粘贴GitHub URL或简写: owner/repo · owner/repo//subdir · owner/repo@v1.2"),
    ("skills.source", "来源"),
    ("skills.subdir", "子目录(可选)"),
    ("skills.ref", "分支/标签(可选)"),
    ("skills.name", "名称(可选)"),
    ("skills.download", "下载并安装"),
    ("skills.cancel", "取消"),
    ("skills.remove", "🗑 删除"),
    ("skills.update", "更新"),
    ("skills.deploy_all", "全部部署"),
    ("skills.undeploy_all", "全部卸载"),
    ("skills.remove_all", "全部删除"),
    ("skills.remove_all_confirm_pre", "确认删除该组全部"),
    ("skills.remove_all_confirm_post", "个技能？此操作不可撤销。"),
    ("skills.remove_all_ok", "确认删除"),
    ("skills.remove_all_cancel", "取消"),
    ("skills.remove_all_done", "个技能已删除"),
    ("skills.scope_global", "全局"),
    ("skills.link_github", "🔗 关联GitHub"),
    ("skills.deploy", "部署"),
    ("skills.undeploy", "取消部署"),
    ("skills.local", "本地"),
    ("skills.empty", "暂无技能"),
    ("skills.empty_sub", "点击\"从GitHub添加\"来获取你的第一个技能。"),
    ("skills.no_match", "无匹配"),
    ("skills.no_match_sub", "尝试不同的筛选条件。"),

    // MCP page
    ("mcp.title", "MCP 服务"),
    ("mcp.subtitle", "统一管理——同步到 Codex、Claude Code 和 Copilot"),
    ("mcp.sync", "全部同步"),
    ("mcp.new", "+ 新建服务"),
    ("mcp.register", "注册MCP服务"),
    ("mcp.name", "名称"),
    ("mcp.transport", "传输方式"),
    ("mcp.command", "命令"),
    ("mcp.args", "参数"),
    ("mcp.env", "环境变量"),
    ("mcp.url", "URL"),
    ("mcp.headers", "请求头"),
    ("mcp.targets", "目标IDE"),
    ("mcp.description", "描述"),
    ("mcp.save", "保存"),
    ("mcp.filter", "筛选"),
    ("mcp.empty", "暂无MCP服务"),
    ("mcp.empty_sub", "点击\"新建服务\"来注册一个。"),
    ("mcp.remove_hint", "删除"),
    ("mcp.enable", "启用"),
    ("mcp.disable", "禁用"),
    ("mcp.paths", "IDE配置路径"),

    // Store page
    ("store.title", "商店"),
    ("store.subtitle", "从 smithery.ai、glama.ai 和 claude-plugins.dev 搜索技能和MCP服务"),
    ("store.search_hint", "搜索服务和技能..."),
    ("store.search", "搜索"),
    ("store.install", "⬇ 安装"),
    ("store.copy_url", "📋 复制URL"),
    ("store.popular", "🔥 热门"),
    ("store.no_results", "无结果"),
    ("store.no_results_sub", "尝试不同的关键词。"),
    ("store.loading", "加载热门..."),

    // Projects page
    ("projects.title", "项目"),
    ("projects.subtitle", "按项目管理技能和MCP部署——为每个项目选择技能和IDE"),
    ("projects.add", "+ 添加项目"),
    ("projects.add_title", "添加项目目录"),
    ("projects.path", "路径"),
    ("projects.browse", "浏览…"),
    ("projects.name", "名称"),
    ("projects.add_btn", "添加"),
    ("projects.cancel", "取消"),
    ("projects.configure", "配置"),
    ("projects.target_ides", "目标IDE"),
    ("projects.target_ides_hint", "选择要为此项目部署技能的IDE"),
    ("projects.skills", "技能"),
    ("projects.skills_hint", "勾选要部署的技能"),
    ("projects.mcp", "MCP 服务"),
    ("projects.mcp_hint", "勾选要配置的服务"),
    ("projects.save_deploy", "保存并部署"),
    ("projects.save_only", "仅保存"),
    ("projects.sync", "↻ 同步"),
    ("projects.remove", "删除项目"),
    ("projects.empty", "暂无项目"),
    ("projects.empty_sub", "添加一个项目来管理按项目的技能部署。"),
    ("projects.close", "关闭"),
    ("projects.no_skills", "暂无已安装的技能"),
    ("projects.no_servers", "暂无已注册的服务"),

    // Profiles page
    ("profiles.title", "配置"),
    ("profiles.subtitle", "命名的叠加层——在不同技能和MCP集之间切换 (工作/开源/演示...)"),
    ("profiles.new", "+ 新建配置"),
    ("profiles.active", "当前配置:"),
    ("profiles.no_active", "无活动配置"),
    ("profiles.no_active_hint", "——同步使用完整注册表"),
    ("profiles.clear", "清除"),
    ("profiles.activate", "激活"),
    ("profiles.edit", "编辑"),
    ("profiles.save", "保存配置"),

    // Settings
    ("settings.title", "设置"),
    ("settings.subtitle", "路径、环境和诊断信息"),
    ("settings.paths", "路径"),
    ("settings.env", "环境变量"),
    ("settings.about", "关于"),
    ("settings.language", "语言"),

    // Common
    ("common.save", "保存"),
    ("common.clear", "清除"),
    ("common.cancel", "取消"),
    ("common.close", "关闭"),
    ("common.delete", "删除"),
    ("common.copy", "复制"),
];
