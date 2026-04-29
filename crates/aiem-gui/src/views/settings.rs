use aiem_core::backup::{AutoInterval, BackupConfig};
use aiem_core::paths;
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::i18n::{self, Lang};
use crate::theme;
use crate::ui;

#[derive(Default)]
pub struct State {
    pub github_token_input: String,
    pub backup_cfg: Option<BackupConfig>,
    pub backup_repo_input: String,
    pub backup_token_input: String,
    pub backup_proxy_input: String,
    pub backup_export_path: String,
    pub backup_import_path: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_toolbar(
        ui,
        i18n::t("settings.title"),
        i18n::t("settings.subtitle"),
        |_| {},
    );

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let pal = theme::p();

            // Language
            ui::settings_group(ui, i18n::t("settings.language"), |ui| {
                ui::settings_row(ui, i18n::t("settings.language"), "", |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(app.lang == Lang::En, "English")
                            .clicked()
                        {
                            app.lang = Lang::En;
                        }
                        if ui
                            .selectable_label(
                                app.lang == Lang::Zh,
                                "\u{7b80}\u{4f53}\u{4e2d}\u{6587}",
                            )
                            .clicked()
                        {
                            app.lang = Lang::Zh;
                        }
                    });
                });
            });

            // Paths
            ui::settings_group(ui, i18n::t("settings.paths"), |ui| {
                let rows: &[(&str, Result<std::path::PathBuf, aiem_core::Error>)] = &[
                    ("aiem home", paths::home()),
                    ("skills", paths::skills_dir()),
                    ("mcp", paths::mcp_dir()),
                    ("backups", paths::backups_dir()),
                    ("cache", paths::cache_dir()),
                ];
                for (i, (label, p)) in rows.iter().enumerate() {
                    let text = match p {
                        Ok(pp) => pp.to_string_lossy().into_owned(),
                        Err(e) => format!("(error: {e})"),
                    };
                    ui::settings_row(ui, label, "", |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&text).size(11.0).monospace().color(pal.text));
                            if ui::small_action(ui, i18n::t("common.copy")).clicked() {
                                ui.output_mut(|o| o.copied_text = text.clone());
                            }
                        });
                    });
                    if i < rows.len() - 1 {
                        ui.separator();
                    }
                }
            });

            // Environment
            ui::settings_group(ui, i18n::t("settings.environment"), |ui| {
                let has_token = std::env::var("GITHUB_TOKEN")
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                ui::settings_row(ui, "GITHUB_TOKEN", i18n::t("settings.token_hint"), |ui| {
                    if has_token {
                        ui::pill(ui, i18n::t("settings.token_set"), theme::SUCCESS());
                    } else {
                        ui::pill(ui, i18n::t("settings.token_unset"), pal.text_sec);
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.settings_state.github_token_input)
                            .password(true)
                            .hint_text("ghp_...")
                            .desired_width(280.0),
                    );
                    let can_save = !app.settings_state.github_token_input.trim().is_empty();
                    if ui
                        .add_enabled(can_save, egui::Button::new(i18n::t("common.save")))
                        .clicked()
                    {
                        let tok = app.settings_state.github_token_input.trim().to_string();
                        std::env::set_var("GITHUB_TOKEN", &tok);
                        match aiem_core::secrets::Vault::load() {
                            Ok(mut vault) => {
                                if let Err(e) = vault.set(
                                    "github_token",
                                    &tok,
                                    Some("GitHub personal access token".into()),
                                ) {
                                    app.toast_error(format!("keyring: {e}"));
                                } else {
                                    app.toast_info("GITHUB_TOKEN saved");
                                }
                            }
                            Err(e) => app.toast_error(format!("vault: {e}")),
                        }
                        app.settings_state.github_token_input.clear();
                    }
                    if has_token && ui::small_action(ui, i18n::t("common.clear")).clicked() {
                        std::env::remove_var("GITHUB_TOKEN");
                        if let Ok(mut vault) = aiem_core::secrets::Vault::load() {
                            let _ = vault.delete("github_token");
                        }
                        app.toast_info("GITHUB_TOKEN cleared");
                    }
                });
                if let Ok(home) = std::env::var("AIEM_HOME") {
                    ui.add_space(4.0);
                    ui::settings_row(ui, "AIEM_HOME", "", |ui| {
                        ui.label(RichText::new(home).size(11.0).monospace().color(pal.text));
                    });
                }
            });

            render_backup(ui, app);
            render_trash(ui, app);

            // About
            ui::settings_group(ui, i18n::t("settings.about"), |ui| {
                ui::settings_row(ui, "aiem", "AI Extension Manager", |ui| {
                    ui.label(
                        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .size(12.0)
                            .color(pal.text_sec),
                    );
                });
            });
        });
}

fn render_backup(ui: &mut egui::Ui, app: &mut App) {
    if app.settings_state.backup_cfg.is_none() {
        let cfg = BackupConfig::load().unwrap_or_default();
        if app.settings_state.backup_repo_input.is_empty() {
            if let Some(repo) = &cfg.github_repo {
                app.settings_state.backup_repo_input = repo.clone();
            }
        }
        if app.settings_state.backup_proxy_input.is_empty() {
            if let Some(proxy) = &cfg.http_proxy {
                app.settings_state.backup_proxy_input = proxy.clone();
            }
        }
        app.settings_state.backup_cfg = Some(cfg);
    }
    let cfg = app.settings_state.backup_cfg.as_ref().unwrap();
    let last_backup_label = cfg
        .last_backup_ts
        .map(aiem_core::backup::time_ago)
        .unwrap_or_else(|| "never".into());
    let cur_interval = cfg.auto_interval;
    let pal = theme::p();

    ui::settings_group(ui, i18n::t("settings.backup"), |ui| {
        ui::settings_row(ui, i18n::t("settings.last_backup"), "", |ui| {
            ui.label(
                RichText::new(&last_backup_label)
                    .size(12.0)
                    .color(pal.text_sec),
            );
        });
        ui.separator();

        ui::settings_row(ui, i18n::t("settings.auto_backup"), "", |ui| {
            ui.horizontal(|ui| {
                for variant in [
                    AutoInterval::Never,
                    AutoInterval::Daily,
                    AutoInterval::Weekly,
                ] {
                    let selected = cur_interval == variant;
                    if ui.selectable_label(selected, variant.label()).clicked() && !selected {
                        if let Some(cfg) = app.settings_state.backup_cfg.as_mut() {
                            cfg.auto_interval = variant;
                            let _ = cfg.save();
                        }
                    }
                }
            });
        });
        ui.separator();

        ui.label(
            RichText::new(i18n::t("settings.local_snapshot"))
                .size(11.0)
                .color(pal.text_sec),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui::secondary_button(ui, i18n::t("settings.snapshot_now")).clicked() {
                app.bus.backup_snapshot();
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.backup_export_path)
                    .hint_text("export directory")
                    .desired_width(260.0),
            );
            let can = !app.settings_state.backup_export_path.trim().is_empty();
            if ui
                .add_enabled(can, egui::Button::new(i18n::t("common.export")))
                .clicked()
            {
                let dest = std::path::PathBuf::from(app.settings_state.backup_export_path.trim());
                app.bus.backup_export_dir(dest);
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.backup_import_path)
                    .hint_text("snapshot directory")
                    .desired_width(260.0),
            );
            let can = !app.settings_state.backup_import_path.trim().is_empty();
            if ui
                .add_enabled(can, egui::Button::new(i18n::t("common.restore")))
                .clicked()
            {
                let src = std::path::PathBuf::from(app.settings_state.backup_import_path.trim());
                app.bus.backup_import_dir(src);
            }
        });

        ui.add_space(8.0);
        ui.separator();

        ui.label(
            RichText::new(i18n::t("settings.github_backup"))
                .size(11.0)
                .color(pal.text_sec),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(i18n::t("settings.repo"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.backup_repo_input)
                    .hint_text("https://github.com/you/aiem-backup")
                    .desired_width(300.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(i18n::t("settings.token"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.backup_token_input)
                    .password(true)
                    .hint_text("ghp_...")
                    .desired_width(280.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(i18n::t("settings.proxy"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.backup_proxy_input)
                    .hint_text("http://127.0.0.1:7890")
                    .desired_width(260.0),
            );
            if ui::secondary_button(ui, i18n::t("settings.save_settings")).clicked() {
                let repo = app.settings_state.backup_repo_input.trim().to_owned();
                let proxy = app.settings_state.backup_proxy_input.trim().to_owned();
                let tok = app.settings_state.backup_token_input.trim().to_owned();
                if let Some(cfg) = app.settings_state.backup_cfg.as_mut() {
                    if !repo.is_empty() {
                        cfg.github_repo = Some(repo);
                    }
                    cfg.http_proxy = if proxy.is_empty() { None } else { Some(proxy) };
                    let _ = cfg.save();
                }
                if !tok.is_empty() {
                    std::env::set_var("GITHUB_TOKEN", &tok);
                }
                app.toast_info("saved");
            }
        });
        ui.add_space(4.0);
        let repo_ok = !app.settings_state.backup_repo_input.trim().is_empty();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    repo_ok,
                    egui::Button::new(i18n::t("settings.test_connection")),
                )
                .clicked()
            {
                let repo = app.settings_state.backup_repo_input.trim().to_owned();
                let token = {
                    let t = app.settings_state.backup_token_input.trim().to_owned();
                    if t.is_empty() {
                        None
                    } else {
                        Some(t)
                    }
                };
                app.bus.backup_test_connection(repo, token);
            }
            if ui
                .add_enabled(repo_ok, egui::Button::new(i18n::t("settings.push")))
                .clicked()
            {
                let repo = app.settings_state.backup_repo_input.trim().to_owned();
                let token = {
                    let t = app.settings_state.backup_token_input.trim().to_owned();
                    if t.is_empty() {
                        None
                    } else {
                        Some(t)
                    }
                };
                app.bus.backup_push_github(repo, token);
            }
            if ui
                .add_enabled(repo_ok, egui::Button::new(i18n::t("settings.pull")))
                .clicked()
            {
                let repo = app.settings_state.backup_repo_input.trim().to_owned();
                let token = {
                    let t = app.settings_state.backup_token_input.trim().to_owned();
                    if t.is_empty() {
                        None
                    } else {
                        Some(t)
                    }
                };
                app.bus.backup_pull_github(repo, token);
            }
        });
    });
}

fn render_trash(ui: &mut egui::Ui, app: &mut App) {
    let pal = theme::p();
    ui::settings_group(ui, i18n::t("settings.trash"), |ui| {
        ui.label(
            RichText::new(i18n::t("settings.trash_hint"))
                .size(12.0)
                .color(pal.text_sec),
        );
        ui.add_space(4.0);

        let trash_dir = match paths::trash_dir() {
            Ok(p) => p,
            Err(e) => {
                ui.label(RichText::new(format!("error: {e}")).color(theme::DANGER()));
                return;
            }
        };
        ui.label(
            RichText::new(trash_dir.to_string_lossy().into_owned())
                .size(11.0)
                .monospace()
                .color(pal.text_sec),
        );
        ui.add_space(4.0);

        let entries: Vec<(String, std::path::PathBuf)> = if trash_dir.exists() {
            let mut v: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(&trash_dir)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect();
            v.sort_by(|a, b| b.0.cmp(&a.0));
            v
        } else {
            Vec::new()
        };

        if entries.is_empty() {
            ui.label(
                RichText::new(i18n::t("settings.trash_empty"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            return;
        }

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} entries", entries.len()))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            if ui::danger_button(ui, i18n::t("settings.empty_trash")).clicked() {
                let mut removed = 0usize;
                for (_, p) in &entries {
                    if aiem_core::fs_util::remove_path(p).is_ok() {
                        removed += 1;
                    }
                }
                app.toast_info(format!("deleted {removed} entries"));
            }
        });
        ui.add_space(4.0);

        for (name, path) in &entries {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).size(11.0).monospace().color(pal.text));
                if ui
                    .small_button(RichText::new(i18n::t("common.delete")).color(theme::DANGER()))
                    .clicked()
                {
                    match aiem_core::fs_util::remove_path(path) {
                        Ok(_) => app.toast_info(format!("deleted `{name}`")),
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                }
            });
        }
    });
}
