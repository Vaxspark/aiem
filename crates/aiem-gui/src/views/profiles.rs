use aiem_core::profiles::{Profile, ProfileStore};
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, primary_button, App};
use crate::theme;

#[derive(Default)]
pub struct State {
    pub add_open: bool,
    pub name: String,
    pub description: String,
    /// Editor state: profile name being edited, then skill_id/server_name -> checked.
    pub editing: Option<String>,
    pub edit_skills: std::collections::BTreeSet<String>,
    pub edit_servers: std::collections::BTreeSet<String>,
    pub edit_description: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    page_header(
        ui,
        "Profiles",
        "Named overlays -- switch between skill & MCP sets (work / oss / demo...)",
        |ui| {
            if primary_button(ui, "+ New profile").clicked() {
                app.profiles_state.add_open = !app.profiles_state.add_open;
            }
        },
    );

    let mut store = match ProfileStore::load() {
        Ok(s) => s,
        Err(e) => { app.toast_error(format!("{e}")); return; }
    };

    // Active profile banner
    render_active_banner(ui, app, &mut store);

    if app.profiles_state.add_open {
        render_add(ui, app, &mut store);
    }

    if let Some(editing) = app.profiles_state.editing.clone() {
        render_editor(ui, app, &mut store, &editing);
    }

    let profiles: Vec<Profile> = store.list().cloned().collect();
    let active = store.active_name().map(str::to_string);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if profiles.is_empty() {
            empty_state(ui, "No profiles yet", "Create one to scope MCP sync or skill deployment.");
            return;
        }
        for p in &profiles {
            render_profile_card(ui, app, &mut store, p, active.as_deref() == Some(&p.name));
        }
    });
}

fn render_active_banner(ui: &mut egui::Ui, app: &mut App, store: &mut ProfileStore) {
    let name = store.active_name().map(str::to_string);
    card(ui, |ui| {
        ui.horizontal(|ui| {
            match &name {
                Some(n) => {
                    ui.label(RichText::new("Active profile:").color(theme::MUTED()));
                    ui.label(RichText::new(n).strong().color(theme::ACCENT()));
                }
                None => {
                    ui.label(RichText::new("No active profile").color(theme::MUTED()));
                    ui.label(RichText::new("-- sync uses the full registry").small().color(theme::MUTED()));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if name.is_some() && ui.button("Clear").clicked() {
                    let _ = store.set_active(None);
                    if store.save().is_ok() { app.toast_info("active profile cleared"); }
                }
            });
        });
    });
    ui.add_space(14.0);
}

fn render_add(ui: &mut egui::Ui, app: &mut App, store: &mut ProfileStore) {
    card(ui, |ui| {
        ui.label(RichText::new("New profile").strong().color(theme::TEXT()));
        ui.add_space(6.0);
        let field_w = (ui.available_width() - 100.0).max(200.0);
        egui::Grid::new("profile-add").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label(RichText::new("Name").color(theme::TEXT()));
            ui.add(egui::TextEdit::singleline(&mut app.profiles_state.name)
                .desired_width(field_w).hint_text("work"));
            ui.end_row();
            ui.label(RichText::new("Description").color(theme::TEXT()));
            ui.add(egui::TextEdit::singleline(&mut app.profiles_state.description).desired_width(field_w));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if primary_button(ui, "Create").clicked() {
                let name = app.profiles_state.name.trim().to_string();
                if name.is_empty() { app.toast_error("name is required"); return; }
                let desc = {
                    let d = app.profiles_state.description.trim();
                    if d.is_empty() { None } else { Some(d.to_string()) }
                };
                store.upsert(Profile { name: name.clone(), description: desc, skills: vec![], mcp_servers: vec![] });
                if let Err(e) = store.save() { app.toast_error(format!("{e}")); return; }
                app.toast_info(format!("created `{name}`"));
                app.profiles_state.add_open = false;
                app.profiles_state.name.clear();
                app.profiles_state.description.clear();
                app.profiles_state.editing = Some(name);
                // seed editor with empty sets; user will check boxes
                app.profiles_state.edit_skills.clear();
                app.profiles_state.edit_servers.clear();
                app.profiles_state.edit_description.clear();
            }
            if ui.button("Cancel").clicked() {
                app.profiles_state.add_open = false;
            }
        });
    });
    ui.add_space(14.0);
}

fn render_editor(ui: &mut egui::Ui, app: &mut App, store: &mut ProfileStore, name: &str) {
    let skill_ids: Vec<(String, String)> = app.skills.list()
        .map(|s| (s.id.clone(), s.name.clone())).collect();
    let server_names: Vec<String> = app.mcp.list().map(|s| s.name.clone()).collect();

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Editing `{name}`")).strong().color(theme::TEXT()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() { app.profiles_state.editing = None; }
            });
        });
        ui.add_space(6.0);

        ui.label(RichText::new("Description").color(theme::MUTED()).small());
        ui.add(egui::TextEdit::singleline(&mut app.profiles_state.edit_description).desired_width(f32::INFINITY));
        ui.add_space(10.0);

        ui.columns(2, |cols| {
            cols[0].label(RichText::new("Skills (empty = all)").strong().color(theme::TEXT()));
            egui::ScrollArea::vertical().id_source("ed-skills").max_height(260.0).show(&mut cols[0], |ui| {
                if skill_ids.is_empty() {
                    ui.label(RichText::new("no skills installed").color(theme::MUTED()).small());
                }
                for (id, _dname) in &skill_ids {
                    let mut checked = app.profiles_state.edit_skills.contains(id);
                    let short = crate::views::skills::short_id(id);
                    if ui.checkbox(&mut checked, short).changed() {
                        if checked { app.profiles_state.edit_skills.insert(id.clone()); }
                        else { app.profiles_state.edit_skills.remove(id); }
                    }
                }
            });

            cols[1].label(RichText::new("MCP servers (empty = all)").strong().color(theme::TEXT()));
            egui::ScrollArea::vertical().id_source("ed-servers").max_height(260.0).show(&mut cols[1], |ui| {
                if server_names.is_empty() {
                    ui.label(RichText::new("no servers registered").color(theme::MUTED()).small());
                }
                for n in &server_names {
                    let mut checked = app.profiles_state.edit_servers.contains(n);
                    if ui.checkbox(&mut checked, n).changed() {
                        if checked { app.profiles_state.edit_servers.insert(n.clone()); }
                        else { app.profiles_state.edit_servers.remove(n); }
                    }
                }
            });
        });

        ui.add_space(10.0);
        if primary_button(ui, "Save profile").clicked() {
            let desc = {
                let d = app.profiles_state.edit_description.trim();
                if d.is_empty() { None } else { Some(d.to_string()) }
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
    });
    ui.add_space(14.0);
}

fn render_profile_card(ui: &mut egui::Ui, app: &mut App, store: &mut ProfileStore, p: &Profile, is_active: bool) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&p.name).strong().size(16.0).color(theme::TEXT()));
                    if is_active { theme::tag(ui, "active", theme::ACCENT()); }
                });
                if let Some(d) = &p.description {
                    ui.label(RichText::new(d).color(theme::MUTED()).small());
                }
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    theme::tag(ui, &format!("{} skills", p.skills.len()), theme::SUCCESS());
                    theme::tag(ui, &format!("{} servers", p.mcp_servers.len()), theme::SUCCESS());
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let btn = egui::Button::new(RichText::new("Delete").color(theme::DANGER()))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).on_hover_text("delete").clicked() {
                    let _ = store.remove(&p.name);
                    if store.save().is_ok() { app.toast_info(format!("removed `{}`", p.name)); }
                }
                let btn = egui::Button::new(RichText::new("Edit"))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).clicked() {
                    app.profiles_state.editing = Some(p.name.clone());
                    app.profiles_state.edit_skills = p.skills.iter().cloned().collect();
                    app.profiles_state.edit_servers = p.mcp_servers.iter().cloned().collect();
                    app.profiles_state.edit_description = p.description.clone().unwrap_or_default();
                }
                if !is_active {
                    let pal = theme::p();
                    let btn = egui::Button::new(RichText::new("Activate").color(pal.accent_fg))
                        .fill(pal.accent)
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(0.0, 26.0));
                    if ui.add(btn).clicked() {
                        if let Err(e) = store.set_active(Some(&p.name)) { app.toast_error(format!("{e}")); }
                        else if store.save().is_ok() { app.toast_info(format!("activated `{}`", p.name)); }
                    }
                }
            });
        });
    });
    ui.add_space(10.0);
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
