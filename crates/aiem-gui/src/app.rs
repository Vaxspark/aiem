use std::sync::mpsc::Receiver;
use std::time::Instant;

use aiem_core::{mcp::McpRegistry, skills::SkillRegistry};
use eframe::egui::{self, Align, Color32, Layout, RichText, Rounding, Stroke};

use crate::i18n::{self, Lang};
use crate::tasks::{TaskBus, TaskMsg};
use crate::theme;
use crate::views::{discover as discover_view, ides, mcp as mcp_view, profiles as profiles_view, projects as projects_view, secrets as secrets_view, settings, skills as skills_view, store as store_view};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Tab {
    Skills,
    Mcp,
    Secrets,
    Profiles,
    Projects,
    Discover,
    Store,
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
    pub profiles_state: profiles_view::State,
    pub projects_state: projects_view::State,
    pub discover_state: discover_view::State,
    pub store_state: store_view::State,
    pub settings_state: settings::State,
    pub theme_mode: ThemeMode,
    pub lang: Lang,
    /// Cached OS theme detection, refreshed periodically so switching System mode reflects the actual OS.
    pub detected_os_dark: bool,
    pub last_os_theme_check: Instant,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let _ = aiem_core::paths::ensure_layout();
        // Load GITHUB_TOKEN from keyring into env (if saved previously).
        if std::env::var("GITHUB_TOKEN").map(|v| v.is_empty()).unwrap_or(true) {
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
            profiles_state: profiles_view::State::default(),
            projects_state: projects_view::State::default(),
            discover_state: discover_view::State::default(),
            store_state: store_view::State::default(),
            settings_state: settings::State::default(),
            theme_mode: ThemeMode::System,
            lang: Lang::Zh,
            detected_os_dark: detect_os_dark(),
            last_os_theme_check: Instant::now(),
        }
    }

    pub fn reload_skills(&mut self) {
        self.skills = SkillRegistry::load().unwrap_or_default();
    }
    pub fn reload_mcp(&mut self) {
        self.mcp = McpRegistry::load().unwrap_or_default();
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
                TaskMsg::RegistryResults(results) => {
                    self.store_state.results = results;
                    self.store_state.searching = false;
                    self.store_state.searched_once = true;
                    self.store_state.error = None;
                }
                TaskMsg::RegistryError(e) => {
                    self.store_state.searching = false;
                    self.store_state.searched_once = true;
                    self.store_state.error = Some(e);
                }
                TaskMsg::PopularResults(results) => {
                    self.store_state.popular = results;
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Re-apply theme every frame so mode toggle takes effect immediately.
        // For System mode, refresh OS detection every 2s (cheap enough) so the
        // user sees the change if they flip OS-wide dark mode while the app runs.
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
                if self.detected_os_dark { theme::Mode::Dark } else { theme::Mode::Light }
            }
        };
        theme::install(ctx, mode);
        i18n::set_lang(self.lang);
        let pal = theme::p();

        self.drain_task_messages();

        // Sidebar
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(170.0)
            .frame(egui::Frame::none()
                .fill(pal.surface)
                .stroke(Stroke::new(1.0, pal.stroke))
                .inner_margin(egui::Margin { left: 14.0, right: 14.0, top: 14.0, bottom: 14.0 }))
            .show(ctx, |ui| self.sidebar(ui));

        // Main content -- responsive margins
        let avail_w = ctx.screen_rect().width() - 170.0;
        let h_margin = (avail_w * 0.03).clamp(12.0, 32.0);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(pal.bg).inner_margin(egui::Margin::symmetric(h_margin, 18.0)))
            .show(ctx, |ui| match self.tab {
                Tab::Skills => skills_view::show(ui, self),
                Tab::Mcp => mcp_view::show(ui, self),
                Tab::Secrets => secrets_view::show(ui, self),
                Tab::Profiles => profiles_view::show(ui, self),
                Tab::Projects => projects_view::show(ui, self),
                Tab::Discover => discover_view::show(ui, self),
                Tab::Store => store_view::show(ui, self),
                Tab::Ides => ides::show(ui, self),
                Tab::Settings => settings::show(ui, self),
            });

        self.render_toasts(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

impl App {
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let pal = theme::p();
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("aiem").heading().strong().color(pal.text));
            ui.label(RichText::new("skills & mcp manager").color(pal.text_sec).small());
            ui.add_space(24.0);

            let items: [(Tab, &str); 9] = [
                (Tab::Skills,   i18n::t("tab.skills")),
                (Tab::Mcp,      i18n::t("tab.mcp")),
                (Tab::Store,    i18n::t("tab.store")),
                (Tab::Projects, i18n::t("tab.projects")),
                (Tab::Secrets,  i18n::t("tab.secrets")),
                (Tab::Profiles, i18n::t("tab.profiles")),
                (Tab::Discover, i18n::t("tab.discover")),
                (Tab::Ides,     i18n::t("tab.ides")),
                (Tab::Settings, i18n::t("tab.settings")),
            ];
            for (tab, label) in items {
                if sidebar_item(ui, label, self.tab == tab).clicked() {
                    self.tab = tab;
                }
            }

            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).small().color(pal.text_sec));
                // Theme mode selector: Light / Dark / System
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let modes = [
                        (ThemeMode::Light, "\u{2600}", "Light"),
                        (ThemeMode::Dark, "\u{1F319}", "Dark"),
                        (ThemeMode::System, "\u{1F4BB}", "Auto"),
                    ];
                    for (mode, icon, _label) in modes {
                        let active = self.theme_mode == mode;
                        let bg = if active { pal.surface_hov } else { pal.surface_hi };
                        let fg = if active { pal.text } else { pal.text_sec };
                        let btn = egui::Button::new(RichText::new(icon).size(13.0).color(fg))
                            .fill(bg)
                            .rounding(Rounding::same(10.0))
                            .min_size(egui::vec2(36.0, 24.0));
                        if ui.add(btn).on_hover_text(_label).clicked() {
                            self.theme_mode = mode;
                            // Force immediate OS theme refresh when switching to System
                            if mode == ThemeMode::System {
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
        if self.toasts.is_empty() { return; }
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
    let bg = if active { pal.surface_hov }
             else if resp.hovered() { pal.surface_hi }
             else { Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, Rounding::same(6.0), bg);
    if active {
        let bar = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 4.0), egui::vec2(3.0, rect.height() - 8.0));
        ui.painter().rect_filled(bar, Rounding::same(1.5), pal.accent);
    }
    let text_color = if active { pal.text } else if resp.hovered() { pal.text } else { pal.text_sec };
    ui.painter().text(
        rect.min + egui::vec2(16.0, rect.height() / 2.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.5),
        text_color,
    );
    resp
}

/// Detect OS-level dark mode preference (Windows/macOS/Linux via `dark-light`).
pub fn detect_os_dark() -> bool {
    matches!(dark_light::detect(), dark_light::Mode::Dark)
}

/// Reusable "card" frame -- consistent size, minimal border, always full width.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let pal = theme::p();
    let w = ui.available_width();
    egui::Frame::none()
        .fill(pal.surface)
        .stroke(Stroke::new(1.0, pal.stroke))
        .rounding(Rounding::same(8.0))
        .shadow(theme::card_shadow())
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_min_width(w - 32.0); // 32 = 16 inner_margin * 2
            add(ui)
        })
        .inner
}

/// Reusable page header (title + optional subtitle + right-aligned content).
pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str, right: impl FnOnce(&mut egui::Ui)) {
    let pal = theme::p();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).heading().strong().color(pal.text));
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).color(pal.text_sec));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(10.0);
}

/// A flat primary button (accent bg, accent_fg text).
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let pal = theme::p();
    let btn = egui::Button::new(RichText::new(label).color(pal.accent_fg).strong())
        .fill(pal.accent)
        .rounding(Rounding::same(6.0))
        .min_size(egui::vec2(0.0, 30.0));
    ui.add(btn)
}
