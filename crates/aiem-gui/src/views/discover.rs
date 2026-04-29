use std::path::PathBuf;

use aiem_core::discover::{self, FoundMcpServer, FoundSkill};
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::i18n;
use crate::theme;
use crate::ui;

#[derive(Default)]
pub struct State {
    pub scanned: bool,
    pub skills: Vec<FoundSkill>,
    pub mcp: Vec<FoundMcpServer>,
    pub skill_checked: Vec<bool>,
    pub mcp_checked: Vec<bool>,
    pub copy_skills: bool,
    pub scan_error: Option<String>,
    pub extra_path: String,
    pub extra_dirs: Vec<PathBuf>,
    pub just_imported: bool,
}

impl State {
    pub fn scan(&mut self) {
        self.scan_error = None;
        let extras = self.extra_dirs.clone();
        let skills_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            discover::discover_skills_with_extras(&extras)
        }));
        let mcp_result = std::panic::catch_unwind(|| discover::discover_mcp());

        self.skills = match skills_result {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                self.scan_error = Some(format!("Skills scan error: {e}"));
                Vec::new()
            }
            Err(_) => {
                self.scan_error = Some("Skills scan crashed (panic)".into());
                Vec::new()
            }
        };
        self.mcp = match mcp_result {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => {
                let msg = format!("MCP scan error: {e}");
                self.scan_error = Some(match self.scan_error.take() {
                    Some(prev) => format!("{prev}; {msg}"),
                    None => msg,
                });
                Vec::new()
            }
            Err(_) => {
                let msg = "MCP scan crashed (panic)".to_string();
                self.scan_error = Some(match self.scan_error.take() {
                    Some(prev) => format!("{prev}; {msg}"),
                    None => msg,
                });
                Vec::new()
            }
        };
        self.skill_checked = vec![true; self.skills.len()];
        self.mcp_checked = vec![true; self.mcp.len()];
        self.scanned = true;
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    if app.discover_state.just_imported {
        app.discover_state.just_imported = false;
        ui.ctx().request_repaint();
        return;
    }

    ui::page_toolbar(
        ui,
        i18n::t("discover.title"),
        i18n::t("discover.subtitle"),
        |ui| {
            if ui::primary_button(ui, i18n::t("discover.scan")).clicked() {
                app.discover_state.scan();
            }
        },
    );

    ui::settings_group(ui, i18n::t("discover.scan_locations"), |ui| {
        let pal = theme::p();
        ui.label(
            RichText::new(i18n::t("discover.scan_hint"))
                .size(12.0)
                .color(pal.text_sec),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.discover_state.extra_path)
                    .hint_text("e.g. E:\\code\\myproject")
                    .desired_width((ui.available_width() - 200.0).max(160.0)),
            );
            if ui::secondary_button(ui, i18n::t("common.browse")).clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    if !app.discover_state.extra_dirs.contains(&folder) {
                        app.discover_state.extra_dirs.push(folder);
                    }
                }
            }
            if ui::secondary_button(ui, i18n::t("common.add")).clicked()
                && !app.discover_state.extra_path.trim().is_empty()
            {
                let raw = app.discover_state.extra_path.trim().to_string();
                let cleaned = raw.trim_matches('"').trim_matches('\'').trim();
                let p = PathBuf::from(cleaned);
                if !app.discover_state.extra_dirs.contains(&p) {
                    app.discover_state.extra_dirs.push(p);
                }
                app.discover_state.extra_path.clear();
            }
        });
        if !app.discover_state.extra_dirs.is_empty() {
            ui.add_space(4.0);
            let mut to_remove = None;
            for (idx, p) in app.discover_state.extra_dirs.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("x").clicked() {
                        to_remove = Some(idx);
                    }
                    ui.label(
                        RichText::new(p.to_string_lossy())
                            .size(12.0)
                            .monospace()
                            .color(pal.text),
                    );
                });
            }
            if let Some(idx) = to_remove {
                app.discover_state.extra_dirs.remove(idx);
            }
        }
    });

    if let Some(err) = &app.discover_state.scan_error {
        ui::settings_group(ui, "", |ui| {
            ui.label(RichText::new(err).color(theme::DANGER()));
        });
    }

    if !app.discover_state.scanned {
        ui::empty_state(ui, i18n::t("discover.ready"), i18n::t("discover.ready_sub"));
        return;
    }

    let n_skills = app.discover_state.skills.len();
    let n_mcp = app.discover_state.mcp.len();

    if app.discover_state.skill_checked.len() != n_skills {
        app.discover_state.skill_checked = vec![true; n_skills];
    }
    if app.discover_state.mcp_checked.len() != n_mcp {
        app.discover_state.mcp_checked = vec![true; n_mcp];
    }

    if n_skills == 0 && n_mcp == 0 {
        ui::empty_state(
            ui,
            i18n::t("discover.nothing"),
            i18n::t("discover.nothing_sub"),
        );
        return;
    }

    ui::settings_group(ui, "", |ui| {
        let sk_sel = app
            .discover_state
            .skill_checked
            .iter()
            .filter(|&&c| c)
            .count();
        let mc_sel = app
            .discover_state
            .mcp_checked
            .iter()
            .filter(|&&c| c)
            .count();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{}: {} skill(s), {} server(s)",
                    i18n::t("discover.selected"),
                    sk_sel,
                    mc_sel
                ))
                .size(13.0)
                .color(theme::p().text),
            );
            ui.checkbox(
                &mut app.discover_state.copy_skills,
                i18n::t("discover.copy_to_aiem"),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui::primary_button(ui, i18n::t("discover.import")).clicked() {
                    do_import(app);
                    return;
                }
                if ui::small_action(ui, i18n::t("common.all")).clicked() {
                    app.discover_state.skill_checked.fill(true);
                    app.discover_state.mcp_checked.fill(true);
                }
                if ui::small_action(ui, i18n::t("common.none")).clicked() {
                    app.discover_state.skill_checked.fill(false);
                    app.discover_state.mcp_checked.fill(false);
                }
            });
        });
    });

    if app.discover_state.just_imported {
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let pal = theme::p();
            if n_skills > 0 {
                ui::settings_group(
                    ui,
                    &format!("{} ({})", i18n::t("tab.skills"), n_skills),
                    |ui| {
                        for i in 0..n_skills {
                            if i >= app.discover_state.skills.len()
                                || i >= app.discover_state.skill_checked.len()
                            {
                                break;
                            }
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.discover_state.skill_checked[i], "");
                                ui.label(
                                    RichText::new(&app.discover_state.skills[i].dir_name)
                                        .size(13.0)
                                        .color(pal.text),
                                );
                                ui::pill(ui, &app.discover_state.skills[i].ide_id, theme::ACCENT());
                                if app.discover_state.skills[i].is_link {
                                    ui::pill(ui, "link", pal.text_sec);
                                }
                            });
                            ui.label(
                                RichText::new(
                                    app.discover_state.skills[i]
                                        .path
                                        .to_string_lossy()
                                        .to_string(),
                                )
                                .size(11.0)
                                .monospace()
                                .color(pal.text_sec),
                            );
                            if i < n_skills - 1 {
                                ui.separator();
                            }
                        }
                    },
                );
            }

            if n_mcp > 0 {
                ui::settings_group(ui, &format!("MCP ({})", n_mcp), |ui| {
                    for i in 0..n_mcp {
                        if i >= app.discover_state.mcp.len()
                            || i >= app.discover_state.mcp_checked.len()
                        {
                            break;
                        }
                        let srv = &app.discover_state.mcp[i];
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut app.discover_state.mcp_checked[i], "");
                            ui.label(RichText::new(&srv.server.name).size(13.0).color(pal.text));
                            ui::pill(ui, &srv.source_ide, theme::SUCCESS());
                        });
                        let desc = match &srv.server.transport {
                            aiem_core::mcp::McpTransport::Stdio { command, args, .. } => {
                                format!("{} {}", command, args.join(" "))
                            }
                            aiem_core::mcp::McpTransport::Http { url, .. }
                            | aiem_core::mcp::McpTransport::Sse { url, .. } => url.clone(),
                        };
                        ui.label(
                            RichText::new(&desc)
                                .size(11.0)
                                .monospace()
                                .color(pal.text_sec),
                        );
                        if i < n_mcp - 1 {
                            ui.separator();
                        }
                    }
                });
            }
        });
}

fn do_import(app: &mut App) {
    let copy = app.discover_state.copy_skills;
    let mut imported_skills = 0;
    let mut imported_mcp = 0;
    let mut errors: Vec<String> = Vec::new();

    let skills_to_import: Vec<FoundSkill> = app
        .discover_state
        .skill_checked
        .iter()
        .enumerate()
        .filter(|(i, &c)| c && *i < app.discover_state.skills.len())
        .map(|(i, _)| app.discover_state.skills[i].clone())
        .collect();
    let mcp_to_import: Vec<FoundMcpServer> = app
        .discover_state
        .mcp_checked
        .iter()
        .enumerate()
        .filter(|(i, &c)| c && *i < app.discover_state.mcp.len())
        .map(|(i, _)| app.discover_state.mcp[i].clone())
        .collect();

    for f in &skills_to_import {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            discover::import_skill(f, copy)
        }));
        match result {
            Ok(Ok(_)) => imported_skills += 1,
            Ok(Err(e)) => errors.push(format!("skill {}: {e}", f.dir_name)),
            Err(_) => errors.push(format!("skill {}: internal error", f.dir_name)),
        }
    }
    for f in &mcp_to_import {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| discover::import_mcp(f)));
        match result {
            Ok(Ok(())) => imported_mcp += 1,
            Ok(Err(e)) => errors.push(format!("mcp {}: {e}", f.server.name)),
            Err(_) => errors.push(format!("mcp {}: internal error", f.server.name)),
        }
    }

    for e in &errors {
        app.toast_error(e.clone());
    }
    if imported_skills > 0 || imported_mcp > 0 {
        app.toast_info(format!(
            "Imported {} skill(s), {} server(s)",
            imported_skills, imported_mcp
        ));
        app.reload_skills();
        app.reload_mcp();
    } else if errors.is_empty() {
        app.toast_info(i18n::t("discover.nothing_selected"));
    }
    app.discover_state.scan();
    app.discover_state.just_imported = true;
}
