use std::path::PathBuf;

use aiem_core::discover::{self, FoundMcpServer, FoundSkill};
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, primary_button, App};
use crate::theme;

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
            Ok(Err(e)) => { self.scan_error = Some(format!("Skills scan error: {e}")); Vec::new() }
            Err(_) => { self.scan_error = Some("Skills scan crashed (panic)".into()); Vec::new() }
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

    page_header(
        ui,
        "Discover",
        "Find existing skills & MCP servers on this machine not yet managed by aiem",
        |ui| {
            if primary_button(ui, "Scan").clicked() {
                app.discover_state.scan();
            }
        },
    );

    // ── Extra scan paths ─────────────────────────────────────────────
    card(ui, |ui| {
        ui.label(RichText::new("Scan locations").strong().color(theme::TEXT()));
        ui.label(
            RichText::new("Home IDE dirs + ~/.agents/skills are always scanned. Add project roots below:")
                .small()
                .color(theme::MUTED()),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Path:").color(theme::TEXT()));
            let resp = ui.add(
                egui::TextEdit::singleline(&mut app.discover_state.extra_path)
                    .hint_text("e.g. E:\\code\\myproject")
                    .text_color(theme::TEXT())
                    .desired_width((ui.available_width() - 200.0).max(160.0)),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Browse…").clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    if !app.discover_state.extra_dirs.contains(&folder) {
                        app.discover_state.extra_dirs.push(folder);
                    }
                }
            }
            if (ui.button("Add").clicked() || enter)
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
                    if ui.small_button("✕").clicked() {
                        to_remove = Some(idx);
                    }
                    ui.label(RichText::new(p.to_string_lossy()).monospace().small().color(theme::TEXT()));
                });
            }
            if let Some(idx) = to_remove {
                app.discover_state.extra_dirs.remove(idx);
            }
        }
    });
    ui.add_space(10.0);

    // ── Scan errors ──────────────────────────────────────────────────
    if let Some(err) = &app.discover_state.scan_error {
        card(ui, |ui| {
            ui.label(RichText::new(format!("⚠ {err}")).color(theme::DANGER()));
        });
        ui.add_space(10.0);
    }

    if !app.discover_state.scanned {
        empty_state(
            ui,
            "Ready to scan",
            "Click \"Scan\" to search your IDE configs and skills directories.",
        );
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
        empty_state(
            ui,
            "Nothing found",
            "All skills and MCP servers are already managed by aiem.",
        );
        return;
    }

    // ── Import actions bar ───────────────────────────────────────────
    card(ui, |ui| {
        ui.horizontal(|ui| {
            let sk_sel = app.discover_state.skill_checked.iter().filter(|&&c| c).count();
            let mc_sel = app.discover_state.mcp_checked.iter().filter(|&&c| c).count();
            ui.label(
                RichText::new(format!("Selected: {} skill(s), {} server(s)", sk_sel, mc_sel))
                    .color(theme::TEXT()),
            );
            ui.checkbox(&mut app.discover_state.copy_skills, "Copy to ~/.aiem");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_button(ui, "Import selected").clicked() {
                    do_import(app);
                    return;
                }
                if ui.button("All").clicked() {
                    app.discover_state.skill_checked.fill(true);
                    app.discover_state.mcp_checked.fill(true);
                }
                if ui.button("None").clicked() {
                    app.discover_state.skill_checked.fill(false);
                    app.discover_state.mcp_checked.fill(false);
                }
            });
        });
    });
    ui.add_space(10.0);

    if app.discover_state.just_imported {
        return;
    }

    // ── Scrollable results ───────────────────────────────────────────
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if n_skills > 0 {
            ui.label(
                RichText::new(format!("Skills ({n_skills})"))
                    .heading()
                    .strong()
                    .color(theme::TEXT()),
            );
            ui.add_space(6.0);
            for i in 0..n_skills {
                if i >= app.discover_state.skills.len()
                    || i >= app.discover_state.skill_checked.len()
                {
                    break;
                }
                let dir_name = app.discover_state.skills[i].dir_name.clone();
                let ide_id = app.discover_state.skills[i].ide_id.clone();
                let is_link = app.discover_state.skills[i].is_link;
                let path_str = app.discover_state.skills[i].path.to_string_lossy().to_string();

                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut app.discover_state.skill_checked[i], "");
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&dir_name).strong().color(theme::TEXT()));
                                theme::tag(ui, &ide_id, theme::ACCENT());
                                if is_link {
                                    theme::tag(ui, "link", theme::MUTED());
                                }
                            });
                            ui.label(
                                RichText::new(&path_str).monospace().small().color(theme::MUTED()),
                            );
                        });
                    });
                });
                ui.add_space(4.0);
            }
            ui.add_space(10.0);
        }

        if n_mcp > 0 {
            ui.label(
                RichText::new(format!("MCP Servers ({n_mcp})"))
                    .heading()
                    .strong()
                    .color(theme::TEXT()),
            );
            ui.add_space(6.0);
            for i in 0..n_mcp {
                if i >= app.discover_state.mcp.len()
                    || i >= app.discover_state.mcp_checked.len()
                {
                    break;
                }
                let name = app.discover_state.mcp[i].server.name.clone();
                let source_ide = app.discover_state.mcp[i].source_ide.clone();
                let targets: Vec<String> = app.discover_state.mcp[i]
                    .server
                    .targets
                    .iter()
                    .filter(|t| *t != &source_ide)
                    .cloned()
                    .collect();
                let transport_desc = match &app.discover_state.mcp[i].server.transport {
                    aiem_core::mcp::McpTransport::Stdio { command, args, .. } => {
                        format!("stdio: {} {}", command, args.join(" "))
                    }
                    aiem_core::mcp::McpTransport::Http { url, .. } => format!("http: {url}"),
                    aiem_core::mcp::McpTransport::Sse { url, .. } => format!("sse: {url}"),
                };

                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut app.discover_state.mcp_checked[i], "");
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&name).strong().color(theme::TEXT()));
                                theme::tag(ui, &source_ide, theme::SUCCESS());
                                for t in &targets {
                                    theme::tag(ui, t, theme::SUCCESS());
                                }
                            });
                            ui.label(
                                RichText::new(&transport_desc)
                                    .monospace()
                                    .small()
                                    .color(theme::MUTED()),
                            );
                        });
                    });
                });
                ui.add_space(4.0);
            }
        }
    });
}

fn do_import(app: &mut App) {
    let copy = app.discover_state.copy_skills;
    let mut imported_skills = 0;
    let mut imported_mcp = 0;
    let mut errors: Vec<String> = Vec::new();

    let skills_to_import: Vec<discover::FoundSkill> = app
        .discover_state
        .skill_checked
        .iter()
        .enumerate()
        .filter(|(i, &c)| c && *i < app.discover_state.skills.len())
        .map(|(i, _)| app.discover_state.skills[i].clone())
        .collect();

    let mcp_to_import: Vec<discover::FoundMcpServer> = app
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
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            discover::import_mcp(f)
        }));
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
        app.toast_info("Nothing selected to import");
    }

    app.discover_state.scan();
    app.discover_state.just_imported = true;
}

fn empty_state(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("---").size(32.0).color(theme::MUTED()));
        ui.add_space(4.0);
        ui.label(RichText::new(title).strong().size(18.0).color(theme::TEXT()));
        ui.label(RichText::new(sub).color(theme::MUTED()));
    });
}
