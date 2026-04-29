use std::sync::mpsc::Receiver;
use std::time::Instant;

use aiem_core::{mcp::McpRegistry, skills::SkillRegistry};
use eframe::egui::{self, Color32, Layout, RichText, Rounding, Stroke};

use crate::i18n::{self, Lang};
use crate::tasks::{TaskBus, TaskMsg};
use crate::theme;
use crate::views::{
    discover as discover_view, ides, mcp as mcp_view, projects as projects_view,
    secrets as secrets_view, settings, skills as skills_view,
};

const SIDEBAR_WIDTH: f32 = 216.0;
const DETAIL_DEFAULT_WIDTH: f32 = 440.0;
const DETAIL_MIN_WIDTH: f32 = 380.0;
const DETAIL_MAX_WIDTH: f32 = 520.0;
const DETAIL_SPLIT_MIN_WIDTH: f32 = 1320.0;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Tab {
    Skills,
    Mcp,
    Secrets,
    Projects,
    Discover,
    Ides,
    Settings,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

pub struct Toast {
    pub text: String,
    pub color: Color32,
    pub until: Instant,
}

pub struct App {
    pub tab: Tab,
    pub bus: TaskBus,
    pub rx: Receiver<TaskMsg>,
    pub skills: SkillRegistry,
    pub mcp: McpRegistry,
    pub toasts: Vec<Toast>,
    pub skills_state: skills_view::State,
    pub mcp_state: mcp_view::State,
    pub secrets_state: secrets_view::State,
    pub projects_state: projects_view::State,
    pub discover_state: discover_view::State,
    pub settings_state: settings::State,
    pub theme_mode: ThemeMode,
    pub lang: Lang,
    pub detected_os_dark: bool,
    pub last_os_theme_check: Instant,
    pub last_auto_backup_check: Instant,

    pub selected_skill: Option<String>,
    pub selected_mcp: Option<String>,
    pub selected_project: Option<String>,
    pub detail_skill_content: Option<String>,
    pub detail_skill_files: Option<Vec<(String, u64)>>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let _ = aiem_core::paths::ensure_layout();
        if std::env::var("GITHUB_TOKEN")
            .map(|v| v.is_empty())
            .unwrap_or(true)
        {
            if let Ok(vault) = aiem_core::secrets::Vault::load() {
                if let Ok(token) = vault.get("github_token") {
                    if !token.is_empty() {
                        std::env::set_var("GITHUB_TOKEN", token);
                    }
                }
            }
        }
        let (bus, rx) = TaskBus::new();
        Self {
            tab: Tab::Skills,
            bus,
            rx,
            skills: SkillRegistry::load().unwrap_or_default(),
            mcp: McpRegistry::load().unwrap_or_default(),
            toasts: Vec::new(),
            skills_state: skills_view::State::default(),
            mcp_state: mcp_view::State::default(),
            secrets_state: secrets_view::State::default(),
            projects_state: projects_view::State::default(),
            discover_state: discover_view::State::default(),
            settings_state: settings::State::default(),
            theme_mode: ThemeMode::System,
            lang: Lang::Zh,
            detected_os_dark: detect_os_dark(),
            last_os_theme_check: Instant::now(),
            last_auto_backup_check: Instant::now(),
            selected_skill: None,
            selected_mcp: None,
            selected_project: None,
            detail_skill_content: None,
            detail_skill_files: None,
        }
    }

    pub fn reload_skills(&mut self) {
        self.skills = SkillRegistry::load().unwrap_or_default();
        self.skills_state.deployed_projects_cache.clear();
        if let Some(id) = &self.selected_skill {
            if self.skills.get(id).is_none() {
                self.selected_skill = None;
                self.detail_skill_content = None;
                self.detail_skill_files = None;
            }
        }
    }
    pub fn reload_mcp(&mut self) {
        self.mcp = McpRegistry::load().unwrap_or_default();
        self.mcp_state.deployed_cache.clear();
        if let Some(name) = &self.selected_mcp {
            if self.mcp.get(name).is_none() {
                self.selected_mcp = None;
            }
        }
    }

    pub fn select_skill(&mut self, id: &str) {
        self.selected_skill = Some(id.to_string());
        self.detail_skill_content = aiem_core::skills::read_skill_content(id).ok();
        self.detail_skill_files = aiem_core::skills::list_skill_files(id).ok();
    }

    pub fn toast_info(&mut self, s: impl Into<String>) {
        self.toasts.push(Toast {
            text: s.into(),
            color: theme::SUCCESS(),
            until: Instant::now() + std::time::Duration::from_secs(3),
        });
    }
    pub fn toast_error(&mut self, s: impl Into<String>) {
        self.toasts.push(Toast {
            text: s.into(),
            color: theme::DANGER(),
            until: Instant::now() + std::time::Duration::from_secs(5),
        });
    }

    fn drain_task_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                TaskMsg::Info(s) => self.toast_info(s),
                TaskMsg::Error(s) => self.toast_error(s),
                TaskMsg::SkillsChanged => self.reload_skills(),
                TaskMsg::McpChanged => self.reload_mcp(),
            }
        }
    }

    fn wants_detail(&self) -> bool {
        match self.tab {
            Tab::Skills => self.selected_skill.is_some(),
            Tab::Mcp => self.selected_mcp.is_some(),
            Tab::Projects => self.selected_project.is_some(),
            _ => false,
        }
    }

    fn render_detail_content(&mut self, ui: &mut egui::Ui) {
        match self.tab {
            Tab::Skills => skills_view::detail(ui, self),
            Tab::Mcp => mcp_view::detail(ui, self),
            Tab::Projects => projects_view::detail(ui, self),
            _ => {}
        }
    }

    fn page_max_width(&self) -> f32 {
        match self.tab {
            Tab::Skills | Tab::Mcp | Tab::Projects => 1040.0,
            _ => 860.0,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.theme_mode == ThemeMode::System
            && self.last_os_theme_check.elapsed() >= std::time::Duration::from_secs(2)
        {
            self.detected_os_dark = detect_os_dark();
            self.last_os_theme_check = Instant::now();
        }
        let mode = match self.theme_mode {
            ThemeMode::Light => theme::Mode::Light,
            ThemeMode::Dark => theme::Mode::Dark,
            ThemeMode::System => {
                if self.detected_os_dark {
                    theme::Mode::Dark
                } else {
                    theme::Mode::Light
                }
            }
        };
        theme::install(ctx, mode);
        i18n::set_lang(self.lang);
        let pal = theme::p();

        self.drain_task_messages();

        if self.last_auto_backup_check.elapsed() >= std::time::Duration::from_secs(60) {
            self.last_auto_backup_check = Instant::now();
            if let Ok(cfg) = aiem_core::backup::BackupConfig::load() {
                if cfg.is_due() {
                    self.bus.backup_snapshot();
                }
            }
        }

        // ── Sidebar ──
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(SIDEBAR_WIDTH)
            .frame(
                egui::Frame::none()
                    .fill(pal.sidebar_bg)
                    .stroke(Stroke::new(1.0, pal.stroke))
                    .inner_margin(egui::Margin {
                        left: 14.0,
                        right: 14.0,
                        top: 14.0,
                        bottom: 14.0,
                    }),
            )
            .show(ctx, |ui| self.sidebar(ui));

        let screen_w = ctx.screen_rect().width();
        let screen_h = ctx.screen_rect().height();
        let wants_detail = self.wants_detail();
        let use_side_panel = wants_detail && screen_w >= DETAIL_SPLIT_MIN_WIDTH;
        let use_floating_detail = wants_detail && !use_side_panel;
        let detail_width =
            ((screen_w - SIDEBAR_WIDTH) * 0.34).clamp(DETAIL_MIN_WIDTH, DETAIL_MAX_WIDTH);

        // Wide windows reserve layout space for details; compact windows use a floating inspector.
        if use_side_panel {
            egui::SidePanel::right("detail-panel-v3")
                .resizable(true)
                .min_width(DETAIL_MIN_WIDTH)
                .max_width(DETAIL_MAX_WIDTH)
                .default_width(detail_width.min(DETAIL_DEFAULT_WIDTH))
                .frame(
                    egui::Frame::none()
                        .fill(pal.surface)
                        .stroke(Stroke::new(1.0, pal.stroke))
                        .inner_margin(egui::Margin::symmetric(18.0, 18.0)),
                )
                .show(ctx, |ui| self.render_detail_content(ui));
        }

        // ── Central panel (list content) ──
        let max_w = self.page_max_width();
        let main_width = (screen_w - SIDEBAR_WIDTH).max(0.0);
        let content_margin_x = if use_side_panel {
            24.0
        } else {
            ((main_width - max_w) * 0.5).max(24.0)
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(pal.bg)
                    .inner_margin(egui::Margin::symmetric(content_margin_x, 18.0)),
            )
            .show(ctx, |ui| self.render_page(ui));

        if use_floating_detail {
            let max_float_w = (screen_w - SIDEBAR_WIDTH - 32.0).max(260.0);
            let float_w = max_float_w
                .min(DETAIL_MAX_WIDTH)
                .max(DETAIL_MIN_WIDTH.min(max_float_w));
            let float_h = (screen_h - 88.0).max(320.0);
            let x = screen_w - float_w - 18.0;
            let y = 56.0;

            egui::Area::new("detail-float-v2".into())
                .fixed_pos(egui::pos2(x, y))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_width(float_w);
                    egui::Frame::none()
                        .fill(pal.surface)
                        .stroke(Stroke::new(1.0, pal.stroke))
                        .rounding(Rounding::same(12.0))
                        .shadow(theme::toast_shadow())
                        .inner_margin(egui::Margin::symmetric(18.0, 18.0))
                        .show(ui, |ui| {
                            ui.set_width(float_w - 36.0);
                            ui.set_max_height(float_h - 36.0);
                            self.render_detail_content(ui);
                        });
                });
        }

        self.render_toasts(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

impl App {
    fn render_page(&mut self, ui: &mut egui::Ui) {
        match self.tab {
            Tab::Skills => skills_view::show(ui, self),
            Tab::Mcp => mcp_view::show(ui, self),
            Tab::Secrets => secrets_view::show(ui, self),
            Tab::Projects => projects_view::show(ui, self),
            Tab::Discover => discover_view::show(ui, self),
            Tab::Ides => ides::show(ui, self),
            Tab::Settings => settings::show(ui, self),
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let pal = theme::p();
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("aiem").size(20.0).strong().color(pal.text));
            ui.label(
                RichText::new(i18n::t("app.subtitle"))
                    .color(pal.text_sec)
                    .size(12.0),
            );
            ui.add_space(22.0);

            let groups: &[(&str, &[(Tab, &str)])] = &[
                (
                    i18n::t("group.library"),
                    &[
                        (Tab::Skills, i18n::t("tab.skills")),
                        (Tab::Mcp, i18n::t("tab.mcp")),
                    ],
                ),
                (
                    i18n::t("group.workspaces"),
                    &[
                        (Tab::Projects, i18n::t("tab.projects")),
                        (Tab::Discover, i18n::t("tab.discover")),
                    ],
                ),
                (
                    i18n::t("group.configuration"),
                    &[(Tab::Secrets, i18n::t("tab.secrets"))],
                ),
                (
                    i18n::t("group.system"),
                    &[
                        (Tab::Ides, i18n::t("tab.ides")),
                        (Tab::Settings, i18n::t("tab.settings")),
                    ],
                ),
            ];
            for (group_label, items) in groups {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(group_label.to_uppercase())
                        .color(pal.text_sec)
                        .size(11.0),
                );
                ui.add_space(4.0);
                for (tab, label) in *items {
                    let prev_tab = self.tab;
                    if sidebar_item(ui, label, self.tab == *tab).clicked() {
                        self.tab = *tab;
                        if prev_tab != *tab {
                            self.selected_skill = None;
                            self.selected_mcp = None;
                            self.selected_project = None;
                            self.detail_skill_content = None;
                            self.detail_skill_files = None;
                        }
                    }
                }
            }

            ui.with_layout(Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .size(10.0)
                        .color(pal.text_sec),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let modes = [
                        (ThemeMode::Light, i18n::t("theme.light")),
                        (ThemeMode::Dark, i18n::t("theme.dark")),
                        (ThemeMode::System, i18n::t("theme.auto")),
                    ];
                    for (tmode, label) in modes {
                        let active = self.theme_mode == tmode;
                        let bg = if active {
                            pal.selected
                        } else {
                            Color32::TRANSPARENT
                        };
                        let fg = if active { pal.accent } else { pal.text_sec };
                        let btn = egui::Button::new(RichText::new(label).size(11.0).color(fg))
                            .fill(bg)
                            .stroke(if active {
                                Stroke::new(1.0, pal.accent.gamma_multiply(0.3))
                            } else {
                                Stroke::NONE
                            })
                            .rounding(Rounding::same(7.0))
                            .min_size(egui::vec2(52.0, 24.0));
                        if ui.add(btn).clicked() {
                            self.theme_mode = tmode;
                            if tmode == ThemeMode::System {
                                self.detected_os_dark = detect_os_dark();
                                self.last_os_theme_check = Instant::now();
                            }
                        }
                    }
                });
            });
        });
    }

    fn render_toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let pal = theme::p();

        egui::Area::new("toasts".into())
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    for t in &self.toasts {
                        egui::Frame::none()
                            .fill(pal.surface)
                            .stroke(Stroke::new(1.0, pal.stroke))
                            .rounding(Rounding::same(8.0))
                            .shadow(theme::toast_shadow())
                            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("\u{2022}").color(t.color).size(14.0));
                                    ui.label(RichText::new(&t.text).color(pal.text));
                                });
                            });
                        ui.add_space(4.0);
                    }
                });
            });
    }
}

fn sidebar_item(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let pal = theme::p();
    let desired = egui::vec2(ui.available_width(), 32.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    let bg = if active {
        pal.selected
    } else if resp.hovered() {
        pal.surface_hov.gamma_multiply(0.5)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, Rounding::same(8.0), bg);
    if active {
        let bar = egui::Rect::from_min_size(
            rect.min + egui::vec2(0.0, 5.0),
            egui::vec2(3.0, rect.height() - 10.0),
        );
        ui.painter()
            .rect_filled(bar, Rounding::same(1.5), pal.accent);
    }
    let text_color = if active { pal.text } else { pal.text_sec };
    ui.painter().text(
        rect.min + egui::vec2(16.0, rect.height() / 2.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.5),
        text_color,
    );
    resp
}

pub fn detect_os_dark() -> bool {
    matches!(dark_light::detect(), dark_light::Mode::Dark)
}
