use aiem_core::ide;
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, App};
use crate::theme;

pub fn show(ui: &mut egui::Ui, _app: &mut App) {
    page_header(ui, "IDE Targets", "Directories aiem will symlink skill packages into", |_| {});
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for i in ide::IDES {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(i.display_name).strong().size(16.0).color(theme::TEXT()));
                        ui.label(RichText::new(i.id).monospace().small().color(theme::MUTED()));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(i.skills_dir).monospace().color(theme::ACCENT()));
                        let scope = match i.default_scope {
                            aiem_core::ide::Scope::User => "user",
                            aiem_core::ide::Scope::Project => "project",
                        };
                        theme::tag(ui, scope, theme::MUTED());
                    });
                });
            });
            ui.add_space(10.0);
        }
    });
}
