use aiem_core::paths;
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, App};
use crate::i18n::{self, Lang};
use crate::theme;

#[derive(Default)]
pub struct State {
    pub github_token_input: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    page_header(ui, i18n::t("settings.title"), i18n::t("settings.subtitle"), |_| {});

    // Language selector
    card(ui, |ui| {
        ui.label(RichText::new(i18n::t("settings.language")).strong().color(theme::TEXT()));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.selectable_label(app.lang == Lang::En, "English").clicked() {
                app.lang = Lang::En;
            }
            if ui.selectable_label(app.lang == Lang::Zh, "简体中文").clicked() {
                app.lang = Lang::Zh;
            }
        });
    });
    ui.add_space(10.0);

    let rows: &[(&str, Result<std::path::PathBuf, aiem_core::Error>)] = &[
        ("aiem home", paths::home()),
        ("skills",    paths::skills_dir()),
        ("mcp",       paths::mcp_dir()),
        ("backups",   paths::backups_dir()),
        ("cache",     paths::cache_dir()),
    ];

    card(ui, |ui| {
        ui.label(RichText::new("Paths").strong().color(theme::TEXT()));
        ui.add_space(6.0);
        egui::Grid::new("paths-grid").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
            for (label, p) in rows {
                ui.label(RichText::new(*label).color(theme::MUTED()));
                let text = match p {
                    Ok(pp) => pp.to_string_lossy().into_owned(),
                    Err(e) => format!("(error: {e})"),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&text).monospace().small().color(theme::TEXT()));
                    if ui.small_button("📋").on_hover_text("copy").clicked() {
                        ui.output_mut(|o| o.copied_text = text.clone());
                    }
                });
                ui.end_row();
            }
        });
    });
    ui.add_space(10.0);

    card(ui, |ui| {
        ui.label(RichText::new("Environment").strong().color(theme::TEXT()));
        ui.add_space(6.0);
        let has_token = std::env::var("GITHUB_TOKEN").map(|v| !v.is_empty()).unwrap_or(false);
        ui.horizontal(|ui| {
            ui.label(RichText::new("GITHUB_TOKEN").color(theme::MUTED()));
            if has_token {
                theme::tag(ui, "set", theme::SUCCESS());
            } else {
                theme::tag(ui, "not set", theme::MUTED());
            }
            ui.label(RichText::new("(helps avoid GitHub API rate limits)").small().color(theme::MUTED()));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.github_token_input)
                    .password(true)
                    .hint_text("ghp_... (paste token here)")
                    .desired_width(320.0),
            );
            let can_save = !app.settings_state.github_token_input.trim().is_empty();
            if ui.add_enabled(can_save, egui::Button::new(i18n::t("common.save"))).clicked() {
                let tok = app.settings_state.github_token_input.trim().to_string();
                std::env::set_var("GITHUB_TOKEN", &tok);
                // Persist to OS keyring
                match aiem_core::secrets::Vault::load() {
                    Ok(mut vault) => {
                        if let Err(e) = vault.set("github_token", &tok, Some("GitHub personal access token".into())) {
                            app.toast_error(format!("keyring save failed: {e}"));
                        } else {
                            app.toast_info("GITHUB_TOKEN saved to keyring");
                        }
                    }
                    Err(e) => app.toast_error(format!("vault load failed: {e}")),
                }
                app.settings_state.github_token_input.clear();
            }
            if has_token && ui.button(i18n::t("common.clear")).clicked() {
                std::env::remove_var("GITHUB_TOKEN");
                // Remove from keyring
                if let Ok(mut vault) = aiem_core::secrets::Vault::load() {
                    let _ = vault.delete("github_token");
                }
                app.toast_info("GITHUB_TOKEN cleared");
            }
        });
        ui.label(
            RichText::new("Saved to OS keyring (Windows Credential Manager / macOS Keychain). Loaded automatically on startup.")
                .small()
                .color(theme::MUTED()),
        );
        if let Ok(home) = std::env::var("AIEM_HOME") {
            ui.horizontal(|ui| {
                ui.label(RichText::new("AIEM_HOME").color(theme::MUTED()));
                ui.label(RichText::new(home).monospace().small().color(theme::TEXT()));
            });
        }
    });
    ui.add_space(10.0);

    card(ui, |ui| {
        ui.label(RichText::new("About").strong().color(theme::TEXT()));
        ui.add_space(6.0);
        ui.label(RichText::new("aiem -- AI Extension Manager").color(theme::TEXT()));
        ui.label(RichText::new(format!("version {}", env!("CARGO_PKG_VERSION"))).small().color(theme::MUTED()));
        ui.label(RichText::new("Pure-Rust desktop app · eframe + egui").small().color(theme::MUTED()));
    });
}
