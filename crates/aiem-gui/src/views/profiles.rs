use aiem_core::profiles::{Profile, ProfileStore};
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::i18n;
use crate::theme;
use crate::ui;

#[derive(Default)]
pub struct State {
    pub add_open: bool,
    pub name: String,
    pub description: String,
    pub editing: Option<String>,
    pub edit_skills: std::collections::BTreeSet<String>,
    pub edit_servers: std::collections::BTreeSet<String>,
    pub edit_description: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_toolbar(
        ui,
        i18n::t("profiles.title"),
        i18n::t("profiles.subtitle"),
        |ui| {
            if ui::primary_button(ui, i18n::t("profiles.new")).clicked() {
                app.profiles_state.add_open = !app.profiles_state.add_open;
            }
        },
    );

    let mut store = match ProfileStore::load() {
        Ok(s) => s,
        Err(e) => {
            app.toast_error(format!("{e}"));
            return;
        }
    };

    render_active_banner(ui, app, &mut store);

    if app.profiles_state.add_open {
        render_add(ui, app, &mut store);
    }
    if let Some(editing) = app.profiles_state.editing.clone() {
        render_editor(ui, app, &mut store, &editing);
    }

    let profiles: Vec<Profile> = store.list().cloned().collect();
    let active = store.active_name().map(str::to_string);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if profiles.is_empty() {
                ui::empty_state(ui, i18n::t("profiles.empty"), i18n::t("profiles.empty_sub"));
                return;
            }
            ui::settings_group(ui, "", |ui| {
                for (i, p) in profiles.iter().enumerate() {
                    let is_active = active.as_deref() == Some(&p.name);
                    render_profile_row(ui, app, &mut store, p, is_active);
                    if i < profiles.len() - 1 {
                        ui.separator();
                    }
                }
            });
        });
}

fn render_active_banner(ui: &mut egui::Ui, app: &mut App, store: &mut ProfileStore) {
    let name = store.active_name().map(str::to_string);
    let pal = theme::p();
    ui::settings_group(ui, "", |ui| {
        ui::settings_row(
            ui,
            i18n::t("profiles.active"),
            i18n::t("profiles.active_hint"),
            |ui| match &name {
                Some(n) => {
                    ui.label(RichText::new(n).strong().color(pal.accent));
                    if ui::small_action(ui, i18n::t("common.clear")).clicked() {
                        let _ = store.set_active(None);
                        if store.save().is_ok() {
                            app.toast_info("cleared");
                        }
                    }
                }
                None => {
                    ui.label(RichText::new(i18n::t("common.none")).color(pal.text_sec));
                }
            },
        );
    });
}

fn render_add(ui: &mut egui::Ui, app: &mut App, store: &mut ProfileStore) {
    ui::settings_group(ui, i18n::t("profiles.add_title"), |ui| {
        let pal = theme::p();
        let field_w = (ui.available_width() - 100.0).max(200.0);
        egui::Grid::new("profile-add")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(i18n::t("skills.name"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.profiles_state.name)
                        .desired_width(field_w)
                        .hint_text("work"),
                );
                ui.end_row();
                ui.label(
                    RichText::new(i18n::t("secrets.description"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.profiles_state.description)
                        .desired_width(field_w),
                );
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui::primary_button(ui, i18n::t("common.create")).clicked() {
                let name = app.profiles_state.name.trim().to_string();
                if name.is_empty() {
                    app.toast_error(i18n::t("profiles.name_required"));
                    return;
                }
                let desc = {
                    let d = app.profiles_state.description.trim();
                    if d.is_empty() {
                        None
                    } else {
                        Some(d.to_string())
                    }
                };
                store.upsert(Profile {
                    name: name.clone(),
                    description: desc,
                    skills: vec![],
                    mcp_servers: vec![],
                });
                if let Err(e) = store.save() {
                    app.toast_error(format!("{e}"));
                    return;
                }
                app.toast_info(format!("created `{name}`"));
                app.profiles_state.add_open = false;
                app.profiles_state.name.clear();
                app.profiles_state.description.clear();
                app.profiles_state.editing = Some(name);
                app.profiles_state.edit_skills.clear();
                app.profiles_state.edit_servers.clear();
                app.profiles_state.edit_description.clear();
            }
            if ui::secondary_button(ui, i18n::t("common.cancel")).clicked() {
                app.profiles_state.add_open = false;
            }
        });
    });
}

fn render_editor(ui: &mut egui::Ui, app: &mut App, store: &mut ProfileStore, name: &str) {
    let skill_ids: Vec<(String, String)> = app
        .skills
        .list()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();
    let server_names: Vec<String> = app.mcp.list().map(|s| s.name.clone()).collect();

    ui::settings_group(ui, &format!("{}: {name}", i18n::t("common.edit")), |ui| {
        let pal = theme::p();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(i18n::t("secrets.description"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.profiles_state.edit_description)
                    .desired_width(f32::INFINITY),
            );
        });
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            cols[0].label(
                RichText::new(i18n::t("profiles.skills_label"))
                    .size(12.0)
                    .strong()
                    .color(pal.text),
            );
            egui::ScrollArea::vertical()
                .id_source("ed-skills")
                .max_height(220.0)
                .show(&mut cols[0], |ui| {
                    if skill_ids.is_empty() {
                        ui.label(
                            RichText::new(i18n::t("profiles.no_skills"))
                                .size(12.0)
                                .color(pal.text_sec),
                        );
                    }
                    for (id, _) in &skill_ids {
                        let mut checked = app.profiles_state.edit_skills.contains(id);
                        let short = crate::views::skills::short_id(id);
                        if ui.checkbox(&mut checked, short).changed() {
                            if checked {
                                app.profiles_state.edit_skills.insert(id.clone());
                            } else {
                                app.profiles_state.edit_skills.remove(id);
                            }
                        }
                    }
                });

            cols[1].label(
                RichText::new(i18n::t("profiles.servers_label"))
                    .size(12.0)
                    .strong()
                    .color(pal.text),
            );
            egui::ScrollArea::vertical()
                .id_source("ed-servers")
                .max_height(220.0)
                .show(&mut cols[1], |ui| {
                    if server_names.is_empty() {
                        ui.label(
                            RichText::new(i18n::t("profiles.no_servers"))
                                .size(12.0)
                                .color(pal.text_sec),
                        );
                    }
                    for n in &server_names {
                        let mut checked = app.profiles_state.edit_servers.contains(n);
                        if ui.checkbox(&mut checked, n).changed() {
                            if checked {
                                app.profiles_state.edit_servers.insert(n.clone());
                            } else {
                                app.profiles_state.edit_servers.remove(n);
                            }
                        }
                    }
                });
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui::primary_button(ui, i18n::t("profiles.save_profile")).clicked() {
                let desc = {
                    let d = app.profiles_state.edit_description.trim();
                    if d.is_empty() {
                        None
                    } else {
                        Some(d.to_string())
                    }
                };
                let p = Profile {
                    name: name.to_string(),
                    description: desc,
                    skills: app.profiles_state.edit_skills.iter().cloned().collect(),
                    mcp_servers: app.profiles_state.edit_servers.iter().cloned().collect(),
                };
                store.upsert(p);
                match store.save() {
                    Ok(()) => {
                        app.toast_info(format!("saved `{name}`"));
                        app.profiles_state.editing = None;
                    }
                    Err(e) => app.toast_error(format!("{e}")),
                }
            }
            if ui::secondary_button(ui, i18n::t("common.close")).clicked() {
                app.profiles_state.editing = None;
            }
        });
    });
}

fn render_profile_row(
    ui: &mut egui::Ui,
    app: &mut App,
    store: &mut ProfileStore,
    p: &Profile,
    is_active: bool,
) {
    let pal = theme::p();
    ui::settings_row(ui, &p.name, p.description.as_deref().unwrap_or(""), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui::pill(ui, &format!("{} skills", p.skills.len()), theme::SUCCESS());
            ui::pill(
                ui,
                &format!("{} servers", p.mcp_servers.len()),
                theme::ACCENT(),
            );
            if is_active {
                ui::pill(ui, "active", pal.accent);
            }
            if ui::small_action(ui, i18n::t("common.edit")).clicked() {
                app.profiles_state.editing = Some(p.name.clone());
                app.profiles_state.edit_skills = p.skills.iter().cloned().collect();
                app.profiles_state.edit_servers = p.mcp_servers.iter().cloned().collect();
                app.profiles_state.edit_description = p.description.clone().unwrap_or_default();
            }
            if !is_active {
                if ui::small_action(ui, i18n::t("profiles.activate")).clicked() {
                    if let Err(e) = store.set_active(Some(&p.name)) {
                        app.toast_error(format!("{e}"));
                    } else if store.save().is_ok() {
                        app.toast_info(format!("activated `{}`", p.name));
                    }
                }
            }
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(i18n::t("common.delete"))
                            .size(12.0)
                            .color(theme::DANGER()),
                    )
                    .fill(egui::Color32::TRANSPARENT)
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 24.0)),
                )
                .clicked()
            {
                let _ = store.remove(&p.name);
                if store.save().is_ok() {
                    app.toast_info(format!("removed `{}`", p.name));
                }
            }
        });
    });
}
