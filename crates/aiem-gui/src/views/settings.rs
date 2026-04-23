use aiem_core::backup::{AutoInterval, BackupConfig};
use aiem_core::paths;
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, App};
use crate::i18n::{self, Lang};
use crate::theme;

#[derive(Default)]
pub struct State {
    pub github_token_input: String,
    // ── Backup ──────────────────────────────────────────────────────────────
    pub backup_cfg: Option<BackupConfig>,
    pub backup_repo_input: String,
    pub backup_token_input: String,    pub backup_proxy_input: String,    pub backup_export_path: String,
    pub backup_import_path: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    page_header(ui, i18n::t("settings.title"), i18n::t("settings.subtitle"), |_| {});

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {

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

    render_backup_card(ui, app);
    ui.add_space(10.0);

    render_trash_card(ui, app);
    ui.add_space(10.0);

    card(ui, |ui| {
        ui.label(RichText::new("About").strong().color(theme::TEXT()));
        ui.add_space(6.0);
        ui.label(RichText::new("aiem -- AI Extension Manager").color(theme::TEXT()));
        ui.label(RichText::new(format!("version {}", env!("CARGO_PKG_VERSION"))).small().color(theme::MUTED()));
        ui.label(RichText::new("Pure-Rust desktop app · eframe + egui").small().color(theme::MUTED()));
    });

    }); // ScrollArea
}

// ─── Backup & Restore card ────────────────────────────────────────────────────

fn render_backup_card(ui: &mut egui::Ui, app: &mut App) {
    // Lazy-load config from disk into state on first render.
    if app.settings_state.backup_cfg.is_none() {
        let cfg = BackupConfig::load().unwrap_or_default();
        // Pre-fill repo input from saved config.
        if app.settings_state.backup_repo_input.is_empty() {
            if let Some(repo) = &cfg.github_repo {
                app.settings_state.backup_repo_input = repo.clone();
            }
        }
        // Pre-fill proxy input from saved config.
        if app.settings_state.backup_proxy_input.is_empty() {
            if let Some(proxy) = &cfg.http_proxy {
                app.settings_state.backup_proxy_input = proxy.clone();
            }
        }
        app.settings_state.backup_cfg = Some(cfg);
    }
    let cfg = app.settings_state.backup_cfg.as_ref().unwrap();
    let last_backup_label = cfg.last_backup_ts
        .map(aiem_core::backup::time_ago)
        .unwrap_or_else(|| "never".into());
    let cur_interval = cfg.auto_interval;

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Backup & Restore").strong().color(theme::TEXT()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("last backup: {last_backup_label}"))
                        .small()
                        .color(theme::MUTED()),
                );
            });
        });
        ui.add_space(8.0);

        // ── Auto-backup interval ──────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(RichText::new("Auto-backup").color(theme::MUTED()).small());
            for variant in [AutoInterval::Never, AutoInterval::Daily, AutoInterval::Weekly] {
                let selected = cur_interval == variant;
                if ui.selectable_label(selected, variant.label()).clicked() && !selected {
                    if let Some(cfg) = app.settings_state.backup_cfg.as_mut() {
                        cfg.auto_interval = variant;
                        let _ = cfg.save();
                    }
                }
            }
        });
        ui.add_space(8.0);

        // ── Local snapshot ────────────────────────────────────────────────
        ui.label(RichText::new("Local snapshot").strong().small().color(theme::TEXT()));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Snapshot now").on_hover_text(
                "Saves skills_index.json, mcp_servers.json, projects.json into ~/.aiem/snapshots/<ts>/",
            ).clicked() {
                app.bus.backup_snapshot();
            }
        });
        ui.add_space(2.0);
        ui.label(RichText::new("Export to directory").small().color(theme::MUTED()));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.backup_export_path)
                    .hint_text("/path/to/export/dir")
                    .desired_width(300.0),
            );
            let can_export = !app.settings_state.backup_export_path.trim().is_empty();
            if ui.add_enabled(can_export, egui::Button::new("Export")).clicked() {
                let dest = std::path::PathBuf::from(
                    app.settings_state.backup_export_path.trim().to_owned()
                );
                app.bus.backup_export_dir(dest);
            }
        });
        ui.add_space(2.0);
        ui.label(RichText::new("Restore from directory").small().color(theme::MUTED()));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.backup_import_path)
                    .hint_text("/path/to/snapshot/dir")
                    .desired_width(300.0),
            );
            let can_import = !app.settings_state.backup_import_path.trim().is_empty();
            if ui.add_enabled(can_import, egui::Button::new("Restore")).on_hover_text(
                "Overwrites current config with the snapshot. Reloads skills & MCP list.",
            ).clicked() {
                let src = std::path::PathBuf::from(
                    app.settings_state.backup_import_path.trim().to_owned()
                );
                app.bus.backup_import_dir(src);
            }
        });

        ui.add_space(10.0);

        // ── GitHub backup ─────────────────────────────────────────────────
        ui.label(RichText::new("GitHub backup").strong().small().color(theme::TEXT()));
        ui.add_space(4.0);
        ui.label(RichText::new("Repo URL (HTTPS)").small().color(theme::MUTED()));
        ui.add(
            egui::TextEdit::singleline(&mut app.settings_state.backup_repo_input)
                .hint_text("https://github.com/you/my-aiem-backup")
                .desired_width(360.0),
        );
        ui.add_space(2.0);
        ui.label(
            RichText::new("Token (leave empty to use saved GITHUB_TOKEN)")
                .small()
                .color(theme::MUTED()),
        );
        ui.add(
            egui::TextEdit::singleline(&mut app.settings_state.backup_token_input)
                .password(true)
                .hint_text("ghp_... (optional override)")
                .desired_width(320.0),
        );
        ui.add_space(2.0);
        ui.label(
            RichText::new("HTTP Proxy (optional, e.g. http://127.0.0.1:7890)")
                .small()
                .color(theme::MUTED()),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.settings_state.backup_proxy_input)
                    .hint_text("http://127.0.0.1:7890  or  socks5://127.0.0.1:1080")
                    .desired_width(300.0),
            );
            if ui.button("💾  Save settings").on_hover_text(
                "Persist Repo URL and proxy to ~/.aiem/backup.json; token to GITHUB_TOKEN env var"
            ).clicked() {
                let repo  = app.settings_state.backup_repo_input.trim().to_owned();
                let proxy = app.settings_state.backup_proxy_input.trim().to_owned();
                let tok   = app.settings_state.backup_token_input.trim().to_owned();
                if let Some(cfg) = app.settings_state.backup_cfg.as_mut() {
                    if !repo.is_empty() { cfg.github_repo = Some(repo); }
                    cfg.http_proxy = if proxy.is_empty() { None } else { Some(proxy) };
                    let _ = cfg.save();
                }
                if !tok.is_empty() {
                    std::env::set_var("GITHUB_TOKEN", &tok);
                }
                app.toast_info("Backup settings saved");
            }
        });
        ui.add_space(4.0);
        let repo_ok = !app.settings_state.backup_repo_input.trim().is_empty();
        ui.horizontal(|ui| {
            if ui.add_enabled(repo_ok, egui::Button::new("🔌  Test connection"))
                .on_hover_text("Run git ls-remote to verify repo URL, token and proxy")
                .clicked()
            {
                let repo  = app.settings_state.backup_repo_input.trim().to_owned();
                let token = {
                    let t = app.settings_state.backup_token_input.trim().to_owned();
                    if t.is_empty() { None } else { Some(t) }
                };
                app.bus.backup_test_connection(repo, token);
            }
            if ui.add_enabled(repo_ok, egui::Button::new("Push to GitHub"))
                .on_hover_text("Commit and push skills_index + mcp_servers to your repo")
                .clicked()
            {
                let repo  = app.settings_state.backup_repo_input.trim().to_owned();
                let token = {
                    let t = app.settings_state.backup_token_input.trim().to_owned();
                    if t.is_empty() { None } else { Some(t) }
                };
                app.bus.backup_push_github(repo, token);
            }
            if ui.add_enabled(repo_ok, egui::Button::new("Pull from GitHub"))
                .on_hover_text("Restore config files from your backup repo. Reloads all data.")
                .clicked()
            {
                let repo  = app.settings_state.backup_repo_input.trim().to_owned();
                let token = {
                    let t = app.settings_state.backup_token_input.trim().to_owned();
                    if t.is_empty() { None } else { Some(t) }
                };
                app.bus.backup_pull_github(repo, token);
            }
        });
        ui.add_space(2.0);
        ui.label(
            RichText::new(
                "Uses ~/.aiem/backup-git/ as git working tree. Requires git in PATH.",
            )
            .small()
            .color(theme::MUTED()),
        );
    });
}

// ─── Trash card ────────────────────────────────────────────────────────

fn render_trash_card(ui: &mut egui::Ui, app: &mut App) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Trash").strong().color(theme::TEXT()));
            ui.add_space(6.0);
            ui.label(
                RichText::new("scanned / removed content is moved here instead of being hard-deleted")
                    .small()
                    .color(theme::MUTED()),
            );
        });
        ui.add_space(6.0);

        let trash_dir = match paths::trash_dir() {
            Ok(p) => p,
            Err(e) => {
                ui.label(RichText::new(format!("trash dir error: {e}")).color(theme::DANGER()));
                return;
            }
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("location:").small().color(theme::MUTED()));
            ui.label(
                RichText::new(trash_dir.to_string_lossy().into_owned())
                    .monospace()
                    .small()
                    .color(theme::TEXT()),
            );
        });
        ui.add_space(6.0);

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
            ui.label(RichText::new("Trash is empty.").color(theme::MUTED()));
            return;
        }

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} entries", entries.len())).color(theme::MUTED()),
            );
            if ui
                .button(RichText::new("Empty trash").color(theme::DANGER()))
                .on_hover_text("permanently delete every entry in trash")
                .clicked()
            {
                let mut removed = 0usize;
                for (_, p) in &entries {
                    if aiem_core::fs_util::remove_path(p).is_ok() {
                        removed += 1;
                    }
                }
                app.toast_info(format!("deleted {removed} trash entries"));
            }
        });
        ui.add_space(4.0);

        egui::Grid::new("trash-grid")
            .num_columns(3)
            .spacing([12.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for (name, path) in &entries {
                    ui.label(RichText::new(name).monospace().small().color(theme::TEXT()));
                    ui.label(
                        RichText::new(path.to_string_lossy().into_owned())
                            .small()
                            .color(theme::MUTED()),
                    );
                    if ui
                        .small_button(RichText::new("delete").color(theme::DANGER()))
                        .clicked()
                    {
                        match aiem_core::fs_util::remove_path(path) {
                            Ok(_) => app.toast_info(format!("deleted `{name}`")),
                            Err(e) => app.toast_error(format!("{e}")),
                        }
                    }
                    ui.end_row();
                }
            });
    });
}

