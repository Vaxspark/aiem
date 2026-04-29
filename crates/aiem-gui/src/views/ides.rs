use aiem_core::ide;
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::i18n;
use crate::theme;
use crate::ui;

pub fn show(ui: &mut egui::Ui, _app: &mut App) {
    ui::page_toolbar(ui, i18n::t("ides.title"), i18n::t("ides.subtitle"), |_| {});
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui::settings_group(ui, "", |ui| {
                for (i, ide_def) in ide::IDES.iter().enumerate() {
                    let pal = theme::p();
                    let scope = match ide_def.default_scope {
                        ide::Scope::User => "user",
                        ide::Scope::Project => "project",
                    };
                    ui::settings_row(ui, ide_def.display_name, ide_def.id, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui::pill(ui, scope, pal.text_sec);
                            ui.label(
                                RichText::new(ide_def.skills_dir)
                                    .size(11.0)
                                    .monospace()
                                    .color(pal.accent),
                            );
                        });
                    });
                    if i < ide::IDES.len() - 1 {
                        ui.separator();
                    }
                }
            });
        });
}
