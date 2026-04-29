use aiem_core::secrets::Vault;
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::i18n;
use crate::theme;
use crate::ui;

#[derive(Default)]
pub struct State {
    pub add_open: bool,
    pub name: String,
    pub value: String,
    pub description: String,
    pub revealed_value: std::collections::HashMap<String, String>,
    pub filter: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_toolbar(
        ui,
        i18n::t("secrets.title"),
        i18n::t("secrets.subtitle"),
        |ui| {
            if ui::primary_button(ui, i18n::t("secrets.new")).clicked() {
                app.secrets_state.add_open = !app.secrets_state.add_open;
            }
        },
    );

    if app.secrets_state.add_open {
        render_add(ui, app);
    }

    ui::search_bar(
        ui,
        &mut app.secrets_state.filter,
        i18n::t("secrets.filter_hint"),
    );
    ui.add_space(8.0);

    let vault = match Vault::load() {
        Ok(v) => v,
        Err(e) => {
            app.toast_error(format!("vault: {e}"));
            return;
        }
    };

    let filter = app.secrets_state.filter.to_ascii_lowercase();
    let names: Vec<String> = vault
        .names()
        .filter(|n| filter.is_empty() || n.to_ascii_lowercase().contains(&filter))
        .cloned()
        .collect();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if vault.is_empty() {
                ui::empty_state(ui, i18n::t("secrets.empty"), i18n::t("secrets.empty_sub"));
                return;
            }
            if names.is_empty() {
                ui::empty_state(
                    ui,
                    i18n::t("secrets.no_match"),
                    i18n::t("secrets.no_match_sub"),
                );
                return;
            }
            ui::settings_group(ui, "", |ui| {
                for (i, name) in names.iter().enumerate() {
                    render_secret_row(ui, app, &vault, name);
                    if i < names.len() - 1 {
                        ui.separator();
                    }
                }
            });
        });
}

fn render_add(ui: &mut egui::Ui, app: &mut App) {
    ui::settings_group(ui, i18n::t("secrets.add_title"), |ui| {
        let pal = theme::p();
        let field_w = (ui.available_width() - 100.0).max(200.0);
        egui::Grid::new("secret-add")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(i18n::t("secrets.name"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.secrets_state.name)
                        .desired_width(field_w)
                        .hint_text("github_token"),
                );
                ui.end_row();
                ui.label(
                    RichText::new(i18n::t("secrets.value"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.secrets_state.value)
                        .desired_width(field_w)
                        .password(true)
                        .hint_text("secret value"),
                );
                ui.end_row();
                ui.label(
                    RichText::new(i18n::t("secrets.description"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.secrets_state.description)
                        .desired_width(field_w),
                );
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui::primary_button(ui, i18n::t("common.save")).clicked() {
                let name = app.secrets_state.name.trim().to_string();
                let value = std::mem::take(&mut app.secrets_state.value);
                let desc = {
                    let d = app.secrets_state.description.trim();
                    if d.is_empty() {
                        None
                    } else {
                        Some(d.to_string())
                    }
                };
                if name.is_empty() {
                    app.toast_error(i18n::t("secrets.name_required"));
                    return;
                }
                if value.is_empty() {
                    app.toast_error(i18n::t("secrets.value_required"));
                    return;
                }
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
            if ui::secondary_button(ui, i18n::t("common.cancel")).clicked() {
                app.secrets_state.add_open = false;
                app.secrets_state.value.clear();
            }
        });
    });
}

fn render_secret_row(ui: &mut egui::Ui, app: &mut App, vault: &Vault, name: &str) {
    let pal = theme::p();

    ui::settings_row(
        ui,
        name,
        vault
            .meta(name)
            .and_then(|m| m.description.as_deref())
            .unwrap_or(""),
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let revealed = app.secrets_state.revealed_value.contains_key(name);
                let reveal_label = if revealed {
                    i18n::t("secrets.hide")
                } else {
                    i18n::t("secrets.reveal")
                };
                if ui::small_action(ui, reveal_label).clicked() {
                    if revealed {
                        app.secrets_state.revealed_value.remove(name);
                    } else {
                        match vault.get(name) {
                            Ok(v) => {
                                app.secrets_state.revealed_value.insert(name.to_string(), v);
                            }
                            Err(e) => app.toast_error(format!("{e}")),
                        }
                    }
                }
                if ui::small_action(ui, i18n::t("secrets.copy_ref")).clicked() {
                    ui.output_mut(|o| o.copied_text = format!("${{secret:{name}}}"));
                    app.toast_info("copied");
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
                    match Vault::load().and_then(|mut v| v.delete(name)) {
                        Ok(()) => app.toast_info(format!("deleted `{name}`")),
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                    app.secrets_state.revealed_value.remove(name);
                }
            });
        },
    );

    if let Some(v) = app.secrets_state.revealed_value.get(name) {
        ui.label(RichText::new(v).size(12.0).monospace().color(pal.accent));
    }

    if let Some(meta) = vault.meta(name) {
        ui.label(
            RichText::new(format!(
                "updated {}",
                meta.updated_at.format("%Y-%m-%d %H:%M")
            ))
            .size(10.0)
            .color(pal.text_sec),
        );
    }
}
