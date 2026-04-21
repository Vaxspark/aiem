use aiem_core::registry::{RegistryItem, RegistrySource};
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, primary_button, App};
use crate::i18n;
use crate::theme;

#[derive(Default)]
pub struct State {
    pub query: String,
    pub results: Vec<RegistryItem>,
    pub searching: bool,
    pub searched_once: bool,
    pub error: Option<String>,
    pub popular: Vec<RegistryItem>,
    pub popular_loaded: bool,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    // Auto-load popular items on first frame
    if !app.store_state.popular_loaded {
        app.store_state.popular_loaded = true;
        app.bus.search_popular();
    }

    page_header(
        ui,
        i18n::t("store.title"),
        i18n::t("store.subtitle"),
        |_| {},
    );

    ui.horizontal(|ui| {
        let field_w = (ui.available_width() - 100.0).max(200.0);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.store_state.query)
                .desired_width(field_w)
                .hint_text("Search servers & skills..."),
        );
        let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if primary_button(ui, "Search").clicked() || enter_pressed {
            if !app.store_state.query.trim().is_empty() {
                let query = app.store_state.query.trim().to_string();
                app.store_state.searching = true;
                app.store_state.error = None;
                app.bus.search_registry(query);
            }
        }
        if app.store_state.searching {
            ui.spinner();
        }
    });
    ui.add_space(12.0);

    if let Some(err) = &app.store_state.error {
        ui.label(RichText::new(err).color(theme::DANGER()));
        ui.add_space(8.0);
    }

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if app.store_state.results.is_empty() && app.store_state.searched_once && !app.store_state.searching {
            empty_state(ui, "No results", "Try different keywords.");
            return;
        }
        if !app.store_state.searched_once && !app.store_state.searching {
            // Show popular items
            if !app.store_state.popular.is_empty() {
                ui.label(RichText::new("\u{1f525} Popular").strong().size(15.0).color(theme::TEXT()));
                ui.add_space(8.0);
                for item in app.store_state.popular.clone() {
                    render_item(ui, app, &item);
                }
            } else {
                empty_state(ui, "Loading popular...", "");
            }
            return;
        }
        for item in app.store_state.results.clone() {
            render_item(ui, app, &item);
        }
    });
}

fn render_item(ui: &mut egui::Ui, app: &mut App, item: &RegistryItem) {
    card(ui, |ui| {
        // Title row
        ui.horizontal(|ui| {
            ui.label(RichText::new(&item.name).strong().size(15.0).color(theme::TEXT()));
            let (src_label, src_color) = match item.source {
                RegistrySource::Smithery => ("smithery", theme::ACCENT()),
                RegistrySource::Glama => ("glama", theme::SUCCESS()),
                RegistrySource::Skills => ("skills", egui::Color32::from_rgb(0x9B, 0x59, 0xB6)),
            };
            theme::tag(ui, src_label, src_color);
            if item.use_count > 0 {
                ui.label(
                    RichText::new(format!("\u{2b50} {}", item.use_count))
                        .small()
                        .color(theme::MUTED()),
                );
            }
        });
        // Description
        if !item.description.is_empty() {
            let desc = if item.description.len() > 200 {
                format!("{}...", &item.description[..200])
            } else {
                item.description.clone()
            };
            ui.label(RichText::new(desc).color(theme::MUTED()).small());
        }
        // URL
        if !item.url.is_empty() {
            ui.label(RichText::new(&item.url).monospace().small().color(theme::ACCENT()));
        }
        // Action buttons row
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if let Some(gh) = &item.github {
                let pal = theme::p();
                let btn = egui::Button::new(RichText::new("⬇ Install").color(pal.accent_fg))
                    .fill(pal.accent)
                    .rounding(egui::Rounding::same(6.0));
                if ui.add(btn).clicked() {
                    app.bus.add_skill_from_github(gh.clone(), None);
                    app.toast_info(format!("Installing {}...", item.name));
                }
            }
            if !item.url.is_empty() {
                if ui.button("\u{1f4cb} Copy URL").clicked() {
                    ui.output_mut(|o| o.copied_text = item.url.clone());
                    app.toast_info("URL copied");
                }
            }
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
