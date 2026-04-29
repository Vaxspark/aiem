use std::path::Path;

use aiem_core::ide;
use aiem_core::mcp::model::McpServer;
use aiem_core::projects::{self, Project, ProjectStore};
use aiem_core::skills::model::{Skill, SkillSource};
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::i18n;
use crate::tasks;
use crate::theme;
use crate::ui;

const PROJECT_SKILL_PREVIEW_MAX_H: f32 = 300.0;
const PROJECT_MCP_PREVIEW_MAX_H: f32 = 300.0;
const PROJECT_ACTION_FOOTER_H: f32 = 30.0;
const PROJECT_ACTION_FOOTER_GAP: f32 = 8.0;

#[derive(Default)]
pub struct State {
    pub add_open: bool,
    pub add_path: String,
    pub add_name: String,
    pub edit_skills: std::collections::BTreeSet<String>,
    pub edit_servers: std::collections::BTreeSet<String>,
    pub edit_ides: std::collections::BTreeSet<String>,
    pub skill_filter: String,
    pub mcp_filter: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_toolbar(
        ui,
        i18n::t("projects.title"),
        i18n::t("projects.subtitle"),
        |ui| {
            if ui::primary_button(ui, i18n::t("projects.add")).clicked() {
                app.projects_state.add_open = !app.projects_state.add_open;
            }
        },
    );

    let mut store = match ProjectStore::load() {
        Ok(s) => s,
        Err(e) => {
            app.toast_error(format!("{e}"));
            return;
        }
    };

    if app.projects_state.add_open {
        render_add(ui, app, &mut store);
    }

    let projects_list: Vec<Project> = store.list().cloned().collect();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if projects_list.is_empty() && !app.projects_state.add_open {
                ui::empty_state(ui, i18n::t("projects.empty"), i18n::t("projects.empty_sub"));
                return;
            }
            for p in &projects_list {
                let is_selected = app.selected_project.as_deref() == Some(&p.path);
                let path_clone = p.path.clone();
                let resp = ui::resource_row(ui, &format!("proj-{}", p.path), is_selected, |ui| {
                    render_project_row(ui, p);
                });
                if resp.clicked() {
                    if is_selected {
                        app.selected_project = None;
                    } else {
                        app.selected_project = Some(path_clone.clone());
                        app.projects_state.skill_filter.clear();
                        app.projects_state.mcp_filter.clear();
                        app.projects_state.edit_skills = p.skills.iter().cloned().collect();
                        app.projects_state.edit_servers = p.mcp_servers.iter().cloned().collect();
                        app.projects_state.edit_ides = p.ides.iter().cloned().collect();
                    }
                }
            }
        });
}

fn render_project_row(ui: &mut egui::Ui, p: &Project) {
    let pal = theme::p();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(&p.name).strong().size(14.0).color(pal.text));
            ui.label(
                RichText::new(&p.path)
                    .size(11.0)
                    .monospace()
                    .color(pal.text_sec),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui::pill(ui, &format!("{} IDE", p.ides.len()), pal.text_sec);
            ui::pill(ui, &format!("{} MCP", p.mcp_servers.len()), theme::ACCENT());
            ui::pill(ui, &format!("{} skills", p.skills.len()), theme::SUCCESS());
        });
    });
}

pub fn detail(ui: &mut egui::Ui, app: &mut App) {
    let project_path = match &app.selected_project {
        Some(p) => p.clone(),
        None => return,
    };

    let mut store = match ProjectStore::load() {
        Ok(s) => s,
        Err(e) => {
            app.toast_error(format!("{e}"));
            return;
        }
    };

    let project = match store.get(&project_path) {
        Some(p) => p.clone(),
        None => {
            app.selected_project = None;
            return;
        }
    };

    let pal = theme::p();

    if ui::detail_header(ui, &project.name, &project.path) {
        app.selected_project = None;
        return;
    }

    let body_h =
        (ui.available_height() - PROJECT_ACTION_FOOTER_H - PROJECT_ACTION_FOOTER_GAP).max(120.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), body_h),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            ui::detail_scroll_body(ui, "project-detail", |ui| {
                ui::detail_section(ui, i18n::t("detail.overview"), |ui| {
                    ui::settings_row(ui, i18n::t("detail.path"), "", |ui| {
                        ui.label(
                            RichText::new(&project.path)
                                .size(12.0)
                                .monospace()
                                .color(pal.text),
                        );
                    });
                    ui::settings_row(ui, i18n::t("projects.skills"), "", |ui| {
                        ui.label(
                            RichText::new(format!("{}", project.skills.len())).color(pal.text),
                        );
                    });
                    ui::settings_row(ui, i18n::t("projects.mcp_servers"), "", |ui| {
                        ui.label(
                            RichText::new(format!("{}", project.mcp_servers.len())).color(pal.text),
                        );
                    });
                });

                ui::detail_section(ui, i18n::t("projects.target_ides"), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for ide_def in ide::IDES {
                            let mut checked = app.projects_state.edit_ides.contains(ide_def.id);
                            if ui.checkbox(&mut checked, ide_def.display_name).changed() {
                                if checked {
                                    app.projects_state.edit_ides.insert(ide_def.id.to_string());
                                } else {
                                    app.projects_state.edit_ides.remove(ide_def.id);
                                }
                            }
                        }
                    });
                });

                ui::detail_section(ui, i18n::t("projects.skills"), |ui| {
                    ui::search_bar(
                        ui,
                        &mut app.projects_state.skill_filter,
                        i18n::t("projects.filter_hint"),
                    );
                    ui.add_space(6.0);

                    let all_skills: Vec<_> = app.skills.list().cloned().collect();
                    let filter = app.projects_state.skill_filter.to_ascii_lowercase();

                    egui::ScrollArea::vertical()
                        .id_source("proj-skills-table")
                        .max_height(PROJECT_SKILL_PREVIEW_MAX_H)
                        .min_scrolled_height(PROJECT_SKILL_PREVIEW_MAX_H)
                        .show(ui, |ui| {
                            if all_skills.is_empty() {
                                ui.label(
                                    RichText::new(i18n::t("projects.no_skills"))
                                        .size(12.0)
                                        .color(pal.text_sec),
                                );
                                return;
                            }
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} {}",
                                        app.projects_state.edit_skills.len(),
                                        i18n::t("discover.selected")
                                    ))
                                    .size(12.0)
                                    .color(pal.text_sec),
                                );
                            });
                            ui.add_space(4.0);
                            for skill in &all_skills {
                                let short = crate::views::skills::short_id(&skill.id);
                                if !filter.is_empty()
                                    && !skill.id.to_lowercase().contains(&filter)
                                    && !short.to_lowercase().contains(&filter)
                                {
                                    continue;
                                }
                                render_project_skill_option(ui, app, skill, short);
                            }
                        });
                });

                ui::detail_section(ui, i18n::t("projects.mcp_servers"), |ui| {
                    let all_servers: Vec<_> = app.mcp.list().cloned().collect();
                    ui::search_bar(
                        ui,
                        &mut app.projects_state.mcp_filter,
                        i18n::t("mcp.search_hint"),
                    );
                    ui.add_space(6.0);
                    let filter = app.projects_state.mcp_filter.to_ascii_lowercase();
                    if all_servers.is_empty() {
                        ui.label(
                            RichText::new(i18n::t("projects.no_servers"))
                                .size(12.0)
                                .color(pal.text_sec),
                        );
                    } else {
                        egui::ScrollArea::vertical()
                            .id_source("proj-mcp-table")
                            .max_height(PROJECT_MCP_PREVIEW_MAX_H)
                            .min_scrolled_height(PROJECT_MCP_PREVIEW_MAX_H)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} {}",
                                            app.projects_state.edit_servers.len(),
                                            i18n::t("discover.selected")
                                        ))
                                        .size(12.0)
                                        .color(pal.text_sec),
                                    );
                                });
                                ui.add_space(4.0);
                                let mut shown = 0;
                                for srv in &all_servers {
                                    if !filter.is_empty()
                                        && !srv.name.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    shown += 1;
                                    render_project_mcp_option(ui, app, srv);
                                }
                                if shown == 0 {
                                    ui.label(
                                        RichText::new(i18n::t("mcp.no_match"))
                                            .size(12.0)
                                            .color(pal.text_sec),
                                    );
                                }
                            });
                    }
                });
            });
        },
    );
    ui.add_space(PROJECT_ACTION_FOOTER_GAP);
    render_project_action_row(ui, app, &mut store, &project, &project_path);
}

fn render_project_action_row(
    ui: &mut egui::Ui,
    app: &mut App,
    store: &mut ProjectStore,
    project: &Project,
    project_path: &str,
) {
    let row_w = (ui.available_width() - ui::DETAIL_GUTTER).max(0.0);
    ui.allocate_ui_with_layout(
        egui::vec2(row_w, PROJECT_ACTION_FOOTER_H),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let gap = ui.spacing().item_spacing.x;
                let row_w = ui.available_width();
                let base = [116.0, 78.0, 58.0, 86.0];
                let base_sum: f32 = base.iter().sum();
                let content_w = (row_w - gap * 3.0).max(0.0);
                let shrink = (content_w / base_sum).min(1.0);
                let extra = (content_w - base_sum).max(0.0);
                let save_deploy_w = base[0] * shrink + extra * 0.32;
                let save_only_w = base[1] * shrink + extra * 0.22;
                let sync_w = base[2] * shrink + extra * 0.18;
                let remove_w = base[3] * shrink + extra * 0.28;

                if ui::fixed_primary_button(ui, i18n::t("projects.save_deploy"), save_deploy_w)
                    .clicked()
                {
                    save_and_deploy(app, store, project_path, true);
                }
                if ui::fixed_secondary_button(ui, i18n::t("projects.save_only"), save_only_w)
                    .clicked()
                {
                    save_and_deploy(app, store, project_path, false);
                }
                if ui::fixed_secondary_button(ui, i18n::t("common.sync"), sync_w).clicked() {
                    let p_path = std::path::PathBuf::from(project_path);
                    let mut ok = 0;
                    for sid in &project.skills {
                        for ide_id in &project.ides {
                            if tasks::deploy_skill(sid, ide_id, Some(&p_path)).is_ok() {
                                ok += 1;
                            }
                        }
                    }
                    if !project.mcp_servers.is_empty() {
                        let _ = tasks::mcp_sync_all(Some(&p_path));
                    }
                    app.reload_skills();
                    app.toast_info(format!("synced {ok} deployment(s)"));
                }
                if ui::fixed_danger_button(ui, i18n::t("projects.remove_project"), remove_w)
                    .clicked()
                {
                    let path = Path::new(project_path);
                    for sid in &project.skills {
                        for ide_id in &project.ides {
                            let _ = tasks::undeploy_skill(sid, ide_id, Some(path));
                        }
                    }
                    let _ = store.remove(project_path);
                    if store.save().is_ok() {
                        app.toast_info("removed");
                        app.selected_project = None;
                        app.reload_skills();
                    }
                }
            });
        },
    );
}

fn render_project_skill_option(ui: &mut egui::Ui, app: &mut App, skill: &Skill, short: &str) {
    let pal = theme::p();
    let mut checked = app.projects_state.edit_skills.contains(&skill.id);
    let source_label = match &skill.source {
        SkillSource::GitHub { owner, repo, .. } => format!("{}/{}", owner, repo),
        SkillSource::Local { .. } => i18n::t("skills.local").to_string(),
    };
    let bg = if checked {
        pal.selected
    } else {
        egui::Color32::TRANSPARENT
    };

    egui::Frame::none()
        .fill(bg)
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 7.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut checked, "").changed() {
                    if checked {
                        app.projects_state.edit_skills.insert(skill.id.clone());
                    } else {
                        app.projects_state.edit_skills.remove(&skill.id);
                    }
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(short).size(13.0).strong().color(pal.text));
                    ui.label(
                        RichText::new(source_label)
                            .size(11.0)
                            .monospace()
                            .color(pal.text_sec),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if checked {
                        ui::pill(ui, i18n::t("discover.selected"), pal.accent);
                    }
                });
            });
        });
    ui.add_space(3.0);
}

fn render_project_mcp_option(ui: &mut egui::Ui, app: &mut App, srv: &McpServer) {
    let pal = theme::p();
    let mut checked = app.projects_state.edit_servers.contains(&srv.name);
    let bg = if checked {
        pal.selected
    } else {
        egui::Color32::TRANSPARENT
    };

    egui::Frame::none()
        .fill(bg)
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 7.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut checked, "").changed() {
                    if checked {
                        app.projects_state.edit_servers.insert(srv.name.clone());
                    } else {
                        app.projects_state.edit_servers.remove(&srv.name);
                    }
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(&srv.name).size(13.0).strong().color(pal.text));
                    let subtitle = if srv.targets.is_empty() {
                        "no targets".to_string()
                    } else {
                        format!("{} target(s)", srv.targets.len())
                    };
                    ui.label(RichText::new(subtitle).size(11.0).color(pal.text_sec));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if checked {
                        ui::pill(ui, i18n::t("discover.selected"), pal.accent);
                    }
                });
            });
        });
    ui.add_space(3.0);
}

fn render_add(ui: &mut egui::Ui, app: &mut App, store: &mut ProjectStore) {
    ui::settings_group(ui, i18n::t("projects.add_title"), |ui| {
        let pal = theme::p();
        let field_w = (ui.available_width() - 160.0).max(200.0);
        egui::Grid::new("project-add")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(i18n::t("projects.path"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.projects_state.add_path)
                            .desired_width(field_w)
                            .hint_text("e.g. C:\\Projects\\my-app"),
                    );
                    if ui::secondary_button(ui, i18n::t("common.browse")).clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            app.projects_state.add_path = folder.to_string_lossy().to_string();
                            if app.projects_state.add_name.is_empty() {
                                if let Some(name) = folder.file_name() {
                                    app.projects_state.add_name =
                                        name.to_string_lossy().to_string();
                                }
                            }
                        }
                    }
                });
                ui.end_row();
                ui.label(
                    RichText::new(i18n::t("projects.name"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.projects_state.add_name)
                        .desired_width(field_w),
                );
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui::primary_button(ui, i18n::t("common.add")).clicked() {
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
                        Path::new(&path)
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| "project".to_string())
                    } else {
                        n.to_string()
                    }
                };
                let detected = projects::detect_project_ides(Path::new(&path));
                store.upsert(Project {
                    name,
                    path: path.clone(),
                    ides: detected.clone(),
                    skills: vec![],
                    mcp_servers: vec![],
                });
                if let Err(e) = store.save() {
                    app.toast_error(format!("{e}"));
                    return;
                }
                app.toast_info(format!("added: {path}"));
                app.projects_state.add_open = false;
                app.projects_state.add_path.clear();
                app.projects_state.add_name.clear();
                app.selected_project = Some(path);
                app.projects_state.skill_filter.clear();
                app.projects_state.mcp_filter.clear();
                app.projects_state.edit_skills.clear();
                app.projects_state.edit_servers.clear();
                app.projects_state.edit_ides = detected.into_iter().collect();
            }
            if ui::secondary_button(ui, i18n::t("common.cancel")).clicked() {
                app.projects_state.add_open = false;
            }
        });
    });
}

fn save_and_deploy(app: &mut App, store: &mut ProjectStore, project_path: &str, deploy: bool) {
    let skills: Vec<String> = app.projects_state.edit_skills.iter().cloned().collect();
    let servers: Vec<String> = app.projects_state.edit_servers.iter().cloned().collect();
    let ides: Vec<String> = app.projects_state.edit_ides.iter().cloned().collect();
    let project_name = store
        .get(project_path)
        .map(|p| p.name.clone())
        .unwrap_or_default();

    let prev_skills: Vec<String> = store
        .get(project_path)
        .map(|p| p.skills.clone())
        .unwrap_or_default();
    let prev_ides: Vec<String> = store
        .get(project_path)
        .map(|p| p.ides.clone())
        .unwrap_or_default();

    if let Some(proj) = store.get_mut(project_path) {
        proj.skills = skills.clone();
        proj.mcp_servers = servers;
        proj.ides = ides.clone();
    }
    if let Err(e) = store.save() {
        app.toast_error(format!("save: {e}"));
        return;
    }

    if !deploy {
        app.toast_info("saved");
        return;
    }

    let path = Path::new(project_path);
    let mut ok_count = 0;
    let mut err_count = 0;

    for old_skill in &prev_skills {
        if !skills.contains(old_skill) {
            for ide_id in &prev_ides {
                let _ = tasks::undeploy_skill(old_skill, ide_id, Some(path));
            }
        }
    }

    for skill_id in &skills {
        for ide_id in &ides {
            match tasks::deploy_skill(skill_id, ide_id, Some(path)) {
                Ok(_) => ok_count += 1,
                Err(e) => {
                    let e_str = format!("{e}");
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

    if !store
        .get(project_path)
        .map(|p| p.mcp_servers.is_empty())
        .unwrap_or(true)
    {
        match tasks::mcp_sync_all(Some(path)) {
            Ok(_) => app.toast_info(format!("MCP synced to {project_name}")),
            Err(e) => app.toast_error(format!("MCP sync: {e}")),
        }
    }

    if err_count == 0 {
        app.toast_info(format!(
            "deployed {ok_count} skill*IDE(s) to {project_name}"
        ));
    } else {
        app.toast_info(format!("deployed {ok_count}, failed {err_count}"));
    }
}
