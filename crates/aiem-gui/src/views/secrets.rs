use aiem_core::secrets::Vault;
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, primary_button, App};
use crate::theme;

#[derive(Default)]
pub struct State {
    pub add_open: bool,
    pub name: String,
    pub value: String,
    pub description: String,
    pub reveal: std::collections::HashSet<String>,
    pub revealed_value: std::collections::HashMap<String, String>,
    pub filter: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    page_header(
        ui,
        "Secrets",
        "Values stored in the OS keyring -- reference as ${secret:NAME} in MCP env/headers",
        |ui| {
            if primary_button(ui, "+ New secret").clicked() {
                app.secrets_state.add_open = !app.secrets_state.add_open;
            }
        },
    );

    if app.secrets_state.add_open {
        render_add(ui, app);
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new("Filter").color(theme::MUTED()));
        ui.add(egui::TextEdit::singleline(&mut app.secrets_state.filter)
            .desired_width((ui.available_width() - 20.0).min(300.0).max(120.0))
            .hint_text("name"));
    });
    ui.add_space(8.0);

    let vault = match Vault::load() {
        Ok(v) => v,
        Err(e) => {
            app.toast_error(format!("vault: {e}"));
            return;
        }
    };

    let filter = app.secrets_state.filter.to_ascii_lowercase();
    let names: Vec<String> = vault.names()
        .filter(|n| filter.is_empty() || n.to_ascii_lowercase().contains(&filter))
        .cloned()
        .collect();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if vault.is_empty() {
            empty_state(ui, "No secrets yet", "Add API tokens / keys once, reuse everywhere.");
            return;
        }
        if names.is_empty() {
            empty_state(ui, "No matches", "Try a different filter.");
            return;
        }
        for name in &names {
            render_secret_card(ui, app, &vault, name);
        }
    });
}

fn render_add(ui: &mut egui::Ui, app: &mut App) {
    card(ui, |ui| {
        ui.label(RichText::new("New secret").strong().color(theme::TEXT()));
        ui.add_space(6.0);
        let field_w = (ui.available_width() - 100.0).max(200.0);
        egui::Grid::new("secret-add").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label(RichText::new("Name").color(theme::TEXT()));
            ui.add(egui::TextEdit::singleline(&mut app.secrets_state.name)
                .desired_width(field_w).hint_text("github_token"));
            ui.end_row();

            ui.label(RichText::new("Value").color(theme::TEXT()));
            ui.add(egui::TextEdit::singleline(&mut app.secrets_state.value)
                .desired_width(field_w).password(true).hint_text("••••"));
            ui.end_row();

            ui.label(RichText::new("Description").color(theme::TEXT()));
            ui.add(egui::TextEdit::singleline(&mut app.secrets_state.description).desired_width(field_w));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if primary_button(ui, "Save").clicked() {
                let name = app.secrets_state.name.trim().to_string();
                let value = std::mem::take(&mut app.secrets_state.value);
                let desc = {
                    let d = app.secrets_state.description.trim();
                    if d.is_empty() { None } else { Some(d.to_string()) }
                };
                if name.is_empty() { app.toast_error("name is required"); return; }
                if value.is_empty() { app.toast_error("value is required"); return; }
                match Vault::load().and_then(|mut v| v.set(&name, &value, desc)) {
                    Ok(()) => {
                        app.toast_info(format!("saved `{name}`"));
                        app.secrets_state.add_open = false;
                        app.secrets_state.name.clear();
                        app.secrets_state.description.clear();
                    }
                    Err(e) => app.toast_error(format!("{e}")),
                }
            }
            if ui.button("Cancel").clicked() {
                app.secrets_state.add_open = false;
                app.secrets_state.value.clear();
            }
        });
    });
    ui.add_space(14.0);
}

fn render_secret_card(ui: &mut egui::Ui, app: &mut App, vault: &Vault, name: &str) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(name).strong().size(16.0).color(theme::TEXT()));
                if let Some(meta) = vault.meta(name) {
                    ui.label(
                        RichText::new(format!("updated {}", meta.updated_at.format("%Y-%m-%d %H:%M")))
                            .small().color(theme::MUTED()),
                    );
                    if let Some(d) = &meta.description {
                        ui.label(RichText::new(d).color(theme::MUTED()).small());
                    }
                }
                if let Some(v) = app.secrets_state.revealed_value.get(name) {
                    ui.add_space(4.0);
                    ui.label(RichText::new(v).monospace().color(theme::ACCENT()));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let btn = egui::Button::new(RichText::new("Delete").color(theme::DANGER()))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).on_hover_text("delete").clicked() {
                    match Vault::load().and_then(|mut v| v.delete(name)) {
                        Ok(()) => { app.toast_info(format!("deleted `{name}`")); }
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                    app.secrets_state.revealed_value.remove(name);
                }
                let btn = egui::Button::new(RichText::new("Copy ref"))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).on_hover_text("copy ${secret:NAME}").clicked() {
                    ui.output_mut(|o| o.copied_text = format!("${{secret:{name}}}"));
                    app.toast_info("placeholder copied");
                }
                let revealed = app.secrets_state.revealed_value.contains_key(name);
                let reveal_label = if revealed { "Hide" } else { "Reveal" };
                let btn = egui::Button::new(RichText::new(reveal_label))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).clicked() {
                    if revealed {
                        app.secrets_state.revealed_value.remove(name);
                    } else {
                        match vault.get(name) {
                            Ok(v) => { app.secrets_state.revealed_value.insert(name.to_string(), v); }
                            Err(e) => app.toast_error(format!("{e}")),
                        }
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
