use std::path::Path;

use aiem_core::ide;
use aiem_core::projects::{self, Project, ProjectStore};
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, primary_button, App};
use crate::i18n;
use crate::tasks;
use crate::theme;

#[derive(Default)]
pub struct State {
    pub add_open: bool,
    pub add_path: String,
    pub add_name: String,
    /// Project path being edited
    pub editing: Option<String>,
    pub edit_skills: std::collections::BTreeSet<String>,
    pub edit_servers: std::collections::BTreeSet<String>,
    pub edit_ides: std::collections::BTreeSet<String>,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    page_header(
        ui,
        i18n::t("projects.title"),
        i18n::t("projects.subtitle"),
        |ui| {
            if primary_button(ui, i18n::t("projects.add")).clicked() {
                app.projects_state.add_open = !app.projects_state.add_open;
            }
        },
    );

    let mut store = match ProjectStore::load() {
        Ok(s) => s,
        Err(e) => { app.toast_error(format!("{e}")); return; }
    };

    if app.projects_state.add_open {
        render_add(ui, app, &mut store);
    }

    if let Some(editing) = app.projects_state.editing.clone() {
        render_editor(ui, app, &mut store, &editing);
    }

    let projects_list: Vec<Project> = store.list().cloned().collect();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if projects_list.is_empty() && !app.projects_state.add_open {
            empty_state(ui);
            return;
        }
        for p in &projects_list {
            let is_editing = app.projects_state.editing.as_deref() == Some(&p.path);
            if !is_editing {
                render_project_card(ui, app, &mut store, p);
            }
        }
    });
}

fn render_add(ui: &mut egui::Ui, app: &mut App, store: &mut ProjectStore) {
    card(ui, |ui| {
        ui.label(RichText::new("Add project directory").strong().color(theme::TEXT()));
        ui.add_space(6.0);
        let field_w = (ui.available_width() - 180.0).max(200.0);
        egui::Grid::new("project-add").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label(RichText::new("Path").color(theme::TEXT()));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut app.projects_state.add_path)
                    .desired_width(field_w)
                    .hint_text("e.g. C:\\Projects\\my-app"));
                if ui.button("Browse…").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        app.projects_state.add_path = folder.to_string_lossy().to_string();
                        if app.projects_state.add_name.is_empty() {
                            if let Some(name) = folder.file_name() {
                                app.projects_state.add_name = name.to_string_lossy().to_string();
                            }
                        }
                    }
                }
            });
            ui.end_row();

            ui.label(RichText::new("Name").color(theme::TEXT()));
            ui.add(egui::TextEdit::singleline(&mut app.projects_state.add_name)
                .desired_width(field_w)
                .hint_text("display name (auto-filled from folder)"));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if primary_button(ui, "Add").clicked() {
                let path = app.projects_state.add_path.trim().to_string();
                if path.is_empty() {
                    app.toast_error("path is required");
                    return;
                }
                if !Path::new(&path).is_dir() {
                    app.toast_error("directory does not exist");
                    return;
                }
                let name = {
                    let n = app.projects_state.add_name.trim();
                    if n.is_empty() {
                        Path::new(&path).file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| "project".to_string())
                    } else {
                        n.to_string()
                    }
                };
                // Auto-detect existing IDE dirs
                let detected = projects::detect_project_ides(Path::new(&path));
                store.upsert(Project {
                    name,
                    path: path.clone(),
                    ides: detected.clone(),
                    skills: vec![],
                    mcp_servers: vec![],
                });
                if let Err(e) = store.save() { app.toast_error(format!("{e}")); return; }
                app.toast_info(format!("added project: {path}"));
                app.projects_state.add_open = false;
                app.projects_state.add_path.clear();
                app.projects_state.add_name.clear();
                // Open editor immediately
                app.projects_state.editing = Some(path);
                app.projects_state.edit_skills.clear();
                app.projects_state.edit_servers.clear();
                app.projects_state.edit_ides = detected.into_iter().collect();
            }
            if ui.button("Cancel").clicked() {
                app.projects_state.add_open = false;
            }
        });
    });
    ui.add_space(14.0);
}

fn render_editor(ui: &mut egui::Ui, app: &mut App, store: &mut ProjectStore, project_path: &str) {
    let skill_ids: Vec<(String, String)> = app.skills.list()
        .map(|s| (s.id.clone(), s.name.clone())).collect();
    let server_names: Vec<String> = app.mcp.list().map(|s| s.name.clone()).collect();
    let project_name = store.get(project_path).map(|p| p.name.clone()).unwrap_or_default();

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Configure: {}", project_name)).strong().color(theme::TEXT()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() { app.projects_state.editing = None; }
            });
        });
        ui.label(RichText::new(project_path).monospace().small().color(theme::MUTED()));
        ui.add_space(10.0);

        // IDE checkboxes
        ui.label(RichText::new("Target IDEs").strong().color(theme::TEXT()));
        ui.label(RichText::new("Select which IDEs to deploy skills into for this project").small().color(theme::MUTED()));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for ide in ide::IDES {
                let mut checked = app.projects_state.edit_ides.contains(ide.id);
                if ui.checkbox(&mut checked, ide.display_name).changed() {
                    if checked { app.projects_state.edit_ides.insert(ide.id.to_string()); }
                    else { app.projects_state.edit_ides.remove(ide.id); }
                }
            }
        });
        ui.add_space(10.0);

        ui.columns(2, |cols| {
            cols[0].label(RichText::new("Skills").strong().color(theme::TEXT()));
            cols[0].label(RichText::new("Check skills to deploy").small().color(theme::MUTED()));
            egui::ScrollArea::vertical().id_source("proj-skills").max_height(240.0).show(&mut cols[0], |ui| {
                if skill_ids.is_empty() {
                    ui.label(RichText::new("no skills installed").color(theme::MUTED()).small());
                }
                for (id, _dname) in &skill_ids {
                    let mut checked = app.projects_state.edit_skills.contains(id);
                    // Display only the last segment of the id for clarity
                    let short = crate::views::skills::short_id(id);
                    if ui.checkbox(&mut checked, short).changed() {
                        if checked { app.projects_state.edit_skills.insert(id.clone()); }
                        else { app.projects_state.edit_skills.remove(id); }
                    }
                }
            });

            cols[1].label(RichText::new("MCP Servers").strong().color(theme::TEXT()));
            cols[1].label(RichText::new("Check servers to configure").small().color(theme::MUTED()));
            egui::ScrollArea::vertical().id_source("proj-servers").max_height(240.0).show(&mut cols[1], |ui| {
                if server_names.is_empty() {
                    ui.label(RichText::new("no servers registered").color(theme::MUTED()).small());
                }
                for n in &server_names {
                    let mut checked = app.projects_state.edit_servers.contains(n);
                    if ui.checkbox(&mut checked, n).changed() {
                        if checked { app.projects_state.edit_servers.insert(n.clone()); }
                        else { app.projects_state.edit_servers.remove(n); }
                    }
                }
            });
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if primary_button(ui, "Save & Deploy").clicked() {
                save_and_deploy(ui, app, store, project_path, true);
            }
            if ui.button("Save only").clicked() {
                save_and_deploy(ui, app, store, project_path, false);
            }
        });
    });
    ui.add_space(14.0);
}

fn save_and_deploy(_ui: &mut egui::Ui, app: &mut App, store: &mut ProjectStore, project_path: &str, deploy: bool) {
    let skills: Vec<String> = app.projects_state.edit_skills.iter().cloned().collect();
    let servers: Vec<String> = app.projects_state.edit_servers.iter().cloned().collect();
    let ides: Vec<String> = app.projects_state.edit_ides.iter().cloned().collect();
    let proj_path = project_path.to_string();
    let project_name = store.get(&proj_path).map(|p| p.name.clone()).unwrap_or_default();

    // Get previous skills to know what to undeploy
    let prev_skills: Vec<String> = store.get(&proj_path)
        .map(|p| p.skills.clone()).unwrap_or_default();
    let prev_ides: Vec<String> = store.get(&proj_path)
        .map(|p| p.ides.clone()).unwrap_or_default();

    // Save project config
    if let Some(proj) = store.get_mut(&proj_path) {
        proj.skills = skills.clone();
        proj.mcp_servers = servers;
        proj.ides = ides.clone();
    }
    if let Err(e) = store.save() {
        app.toast_error(format!("save: {e}"));
        return;
    }

    if !deploy {
        app.toast_info("project config saved");
        app.projects_state.editing = None;
        return;
    }

    let path = Path::new(&proj_path);
    let mut ok_count = 0;
    let mut err_count = 0;

    // Undeploy skills removed from the config
    for old_skill in &prev_skills {
        if !skills.contains(old_skill) {
            for ide_id in &prev_ides {
                let _ = tasks::undeploy_skill(old_skill, ide_id, Some(path));
            }
        }
    }

    // Deploy selected skills to all selected IDEs
    for skill_id in &skills {
        for ide_id in &ides {
            match tasks::deploy_skill(skill_id, ide_id, Some(path)) {
                Ok(_) => ok_count += 1,
                Err(e) => {
                    let e_str = format!("{e}");
                    // Ignore "already exists" type errors
                    if !e_str.contains("already exists") {
                        app.toast_error(format!("{skill_id}@{ide_id}: {e}"));
                        err_count += 1;
                    } else {
                        ok_count += 1;
                    }
                }
            }
        }
    }

    app.reload_skills();

    // Sync MCP servers to project if any selected
    if !store.get(&proj_path).map(|p| p.mcp_servers.is_empty()).unwrap_or(true) {
        match tasks::mcp_sync_all(Some(path)) {
            Ok(_) => app.toast_info(format!("MCP servers synced to {project_name}")),
            Err(e) => app.toast_error(format!("MCP sync: {e}")),
        }
    }

    if err_count == 0 {
        app.toast_info(format!("deployed {ok_count} skill×IDE(s) to {project_name}"));
    } else {
        app.toast_info(format!("deployed {ok_count}, failed {err_count}"));
    }
    app.projects_state.editing = None;
}

fn render_project_card(ui: &mut egui::Ui, app: &mut App, store: &mut ProjectStore, p: &Project) {
    let project_path = p.path.clone();
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&p.name).strong().size(16.0).color(theme::TEXT()));
                ui.label(RichText::new(&p.path).monospace().small().color(theme::MUTED()));
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for ide_id in &p.ides {
                        theme::tag(ui, ide_id, theme::ACCENT());
                    }
                    theme::tag(ui, &format!("{} skills", p.skills.len()), theme::SUCCESS());
                    theme::tag(ui, &format!("{} servers", p.mcp_servers.len()), theme::SUCCESS());
                });
                if !p.skills.is_empty() {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new("skills:").small().color(theme::MUTED()));
                        for sid in &p.skills {
                            let short = crate::views::skills::short_id(sid);
                            ui.label(RichText::new(short).small().color(theme::TEXT()));
                        }
                    });
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let btn = egui::Button::new(RichText::new("Remove").color(theme::DANGER()))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).on_hover_text("remove project").clicked() {
                    let path = Path::new(&project_path);
                    for sid in &p.skills {
                        for ide_id in &p.ides {
                            let _ = tasks::undeploy_skill(sid, ide_id, Some(path));
                        }
                    }
                    let _ = store.remove(&project_path);
                    if store.save().is_ok() {
                        app.toast_info("removed project");
                        app.reload_skills();
                    }
                }
                let btn = egui::Button::new(RichText::new("Configure"))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).clicked() {
                    app.projects_state.editing = Some(project_path.clone());
                    app.projects_state.edit_skills = p.skills.iter().cloned().collect();
                    app.projects_state.edit_servers = p.mcp_servers.iter().cloned().collect();
                    app.projects_state.edit_ides = p.ides.iter().cloned().collect();
                }
                if !p.skills.is_empty() && !p.ides.is_empty() {
                    let btn = egui::Button::new(RichText::new("\u{21BB} Sync"))
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(0.0, 26.0));
                    if ui.add(btn).on_hover_text("re-deploy all skills & MCP to this project").clicked() {
                        let mut ok = 0;
                        let path = Path::new(&project_path);
                        for sid in &p.skills {
                            for ide_id in &p.ides {
                                if tasks::deploy_skill(sid, ide_id, Some(path)).is_ok() {
                                    ok += 1;
                                }
                            }
                        }
                        if !p.mcp_servers.is_empty() {
                            let _ = tasks::mcp_sync_all(Some(path));
                        }
                        app.reload_skills();
                        app.toast_info(format!("synced {ok} deployment(s)"));
                    }
                }
            });
        });
    });
    ui.add_space(10.0);
}

fn empty_state(ui: &mut egui::Ui) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("---").size(32.0).color(theme::MUTED()));
        ui.add_space(4.0);
        ui.label(RichText::new("No projects yet").strong().size(18.0).color(theme::TEXT()));
        ui.label(RichText::new("Add a project to manage per-project skill deployments across multiple IDEs.").color(theme::MUTED()));
    });
}
