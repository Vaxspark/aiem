use std::collections::{BTreeMap, BTreeSet};

use aiem_core::ide;
use aiem_core::mcp::model::{McpAuthMode, McpRuntime, McpServer, McpTransport};
use aiem_core::mcp::McpRegistry;
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::i18n;
use crate::tasks;
use crate::theme;
use crate::ui;

/// State for the GitHub MCP import dialog.
#[derive(Default)]
pub struct GithubImportState {
    pub open: bool,
    pub repo_input: String,
    pub ref_input: String,
    pub preview: Option<aiem_core::mcp::github::McpPreview>,
    pub selected: BTreeSet<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub imported: Option<Vec<String>>,
}

#[derive(Default)]
pub struct State {
    pub add_open: bool,
    pub json_input: String,
    pub filter: String,
    pub deploy_ide: std::collections::HashMap<String, String>,
    pub deploy_scope: std::collections::HashMap<String, String>,
    pub deployed_cache: std::collections::HashMap<String, Vec<String>>,
    pub bundles_open: bool,
    pub bundle_name_input: String,
    pub bundle_src_input: String,
    pub github_import: GithubImportState,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_toolbar(ui, i18n::t("mcp.title"), i18n::t("mcp.subtitle"), |ui| {
        if ui::primary_button(ui, i18n::t("mcp.sync_all")).clicked() {
            match tasks::mcp_sync_all(None) {
                Ok(touched) => {
                    for (ide, path) in &touched {
                        app.toast_info(format!("{ide}: {}", path.display()));
                    }
                    if touched.is_empty() {
                        app.toast_info("nothing to sync");
                    }
                    app.reload_mcp();
                }
                Err(e) => app.toast_error(format!("sync: {e}")),
            }
        }
        if ui::secondary_button(ui, i18n::t("mcp.new")).clicked() {
            app.mcp_state.add_open = !app.mcp_state.add_open;
        }
        if ui::secondary_button(ui, i18n::t("mcp.bundles")).clicked() {
            app.mcp_state.bundles_open = !app.mcp_state.bundles_open;
        }
        if ui::secondary_button(ui, i18n::t("mcp.import_github")).clicked() {
            app.mcp_state.github_import.open = !app.mcp_state.github_import.open;
        }
    });

    if app.mcp_state.add_open {
        render_add(ui, app);
    }
    if app.mcp_state.bundles_open {
        render_bundles(ui, app);
    }
    if app.mcp_state.github_import.open {
        render_github_import(ui, app);
    }

    ui::search_bar_with_right_gutter(
        ui,
        &mut app.mcp_state.filter,
        i18n::t("mcp.search_hint"),
        ui::LIST_GUTTER,
    );
    ui.add_space(8.0);

    let filter = app.mcp_state.filter.to_ascii_lowercase();
    let servers: Vec<_> = app.mcp.list().cloned().collect();
    let total = servers.len();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut shown = 0;
            for s in &servers {
                if !filter.is_empty() && !s.name.to_lowercase().contains(&filter) {
                    continue;
                }
                shown += 1;
                let is_selected = app.selected_mcp.as_deref() == Some(&s.name);
                let name = s.name.clone();
                let resp = ui::resource_row(ui, &format!("mcp-{}", s.name), is_selected, |ui| {
                    render_server_row(ui, s);
                });
                if resp.clicked() {
                    if is_selected {
                        app.selected_mcp = None;
                    } else {
                        app.selected_mcp = Some(name);
                    }
                }
            }
            if total == 0 {
                ui::empty_state(ui, i18n::t("mcp.empty"), i18n::t("mcp.empty_sub"));
            } else if shown == 0 {
                ui::empty_state(ui, i18n::t("mcp.no_match"), i18n::t("mcp.no_match_sub"));
            }
        });
}

fn render_server_row(ui: &mut egui::Ui, s: &McpServer) {
    let pal = theme::p();
    let (kind, color, subtitle) = match &s.transport {
        McpTransport::Stdio { command, .. } => ("stdio", theme::ACCENT(), command.clone()),
        McpTransport::Http { url, .. } => ("http", theme::SUCCESS(), url.clone()),
        McpTransport::Sse { url, .. } => ("sse", theme::SUCCESS(), url.clone()),
    };
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(&s.name).strong().size(14.0).color(pal.text));
                ui::pill(ui, kind, color);
                if s.disabled {
                    ui::pill(ui, i18n::t("common.disable"), theme::DANGER());
                }
            });
            ui.label(
                RichText::new(subtitle)
                    .size(11.0)
                    .monospace()
                    .color(pal.text_sec),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !s.targets.is_empty() {
                ui::pill(ui, &format!("{} IDE", s.targets.len()), pal.text_sec);
            }
        });
    });
}

pub fn detail(ui: &mut egui::Ui, app: &mut App) {
    let server_name = match &app.selected_mcp {
        Some(n) => n.clone(),
        None => return,
    };
    let server = match app.mcp.get(&server_name) {
        Some(s) => s.clone(),
        None => {
            app.selected_mcp = None;
            return;
        }
    };

    if ui::detail_header(ui, &server.name, "") {
        app.selected_mcp = None;
        return;
    }

    ui::detail_scroll_body(ui, "mcp-detail", |ui| {
        let pal = theme::p();

        // ── Transport ──
        ui::detail_section(ui, i18n::t("detail.transport"), |ui| {
            render_transport_kv(ui, &server, &pal);
        });

        // ── Description ──
        if let Some(d) = &server.description {
            ui::detail_section(ui, i18n::t("detail.description"), |ui| {
                ui.label(RichText::new(d).size(13.0).color(pal.text));
            });
        }

        // ── Runtime / Bundle / Auth / Source ──
        render_runtime_kv(ui, &server, &pal);

        // ── Deployment records ──
        ui::detail_section(ui, i18n::t("deployment.records"), |ui| {
            let rows = mcp_deployment_records(&server);
            ui::deployment_records_table(ui, &format!("mcp-deployments-{}", server.name), &rows);
        });

        // ── Actions (unified: IDE + scope + deploy/sync + enable/disable) ──
        render_unified_actions(ui, app, &server);

        // ── Danger Zone ──
        let sname = server.name.clone();
        ui::detail_danger_footer(ui, i18n::t("mcp.danger_desc"), |ui| {
            if ui::fixed_danger_button(ui, i18n::t("mcp.remove_server"), 112.0).clicked() {
                match tasks::mcp_remove(&sname) {
                    Ok(_) => {
                        app.toast_info("removed");
                        app.selected_mcp = None;
                        app.reload_mcp();
                    }
                    Err(e) => app.toast_error(format!("{e}")),
                }
            }
        });
    });
}

fn normalized_targets(targets: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for target in targets {
        let canonical = aiem_core::mcp::adapters::canonical_id(target).to_string();
        if !out.iter().any(|x| x == &canonical) {
            out.push(canonical);
        }
    }
    out
}

fn selected_mcp_ide(app: &mut App, server: &McpServer) -> String {
    let default = normalized_targets(&server.targets)
        .into_iter()
        .find(|target| ide::find(target).is_some())
        .unwrap_or_else(|| "claude-code".to_string());
    let key = format!("mcp-ide-{}", server.name);
    let selected = app.mcp_state.deploy_ide.entry(key).or_insert(default);
    let canonical = aiem_core::mcp::adapters::canonical_id(selected).to_string();
    if ide::find(&canonical).is_some() {
        *selected = canonical.clone();
        canonical
    } else {
        *selected = "claude-code".to_string();
        "claude-code".to_string()
    }
}

fn mcp_ide_combo(ui: &mut egui::Ui, id: &str, selected: &mut String, width: f32) {
    let selected_label = ide::find(selected)
        .map(|i| i.display_name)
        .unwrap_or(selected.as_str());
    egui::ComboBox::from_id_source(id)
        .selected_text(RichText::new(selected_label).size(12.0))
        .width(width)
        .show_ui(ui, |ui| {
            for ide_def in ide::IDES {
                ui.selectable_value(selected, ide_def.id.to_string(), ide_def.display_name);
            }
        });
}

fn mcp_is_synced(name: &str, ide_id: &str, project: Option<&std::path::Path>) -> bool {
    aiem_core::mcp::adapters::read(ide_id, project)
        .map(|servers| servers.iter().any(|s| s.name == name))
        .unwrap_or(false)
}

fn mcp_deployment_records(s: &McpServer) -> Vec<(String, String, String, egui::Color32)> {
    let mut rows = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let global_targets = normalized_targets(&s.targets);

    for ide_id in &global_targets {
        let ide_label = ide::find(ide_id)
            .map(|i| i.display_name.to_string())
            .unwrap_or_else(|| ide_id.clone());
        let synced = mcp_is_synced(&s.name, ide_id, None);
        let (status, color) = if s.disabled {
            (i18n::t("common.disable").to_string(), theme::DANGER())
        } else if synced {
            (i18n::t("deployment.synced").to_string(), theme::SUCCESS())
        } else {
            (
                i18n::t("deployment.not_synced").to_string(),
                theme::WARNING(),
            )
        };
        if seen.insert((i18n::t("common.global").to_string(), ide_label.clone())) {
            rows.push((
                i18n::t("common.global").to_string(),
                ide_label,
                status,
                color,
            ));
        }
    }

    if let Ok(store) = aiem_core::projects::ProjectStore::load() {
        for project in store
            .list()
            .filter(|p| p.mcp_servers.iter().any(|name| name == &s.name))
        {
            let project_path = std::path::Path::new(&project.path);
            let mut found_synced = false;
            for ide_def in ide::IDES {
                if mcp_is_synced(&s.name, ide_def.id, Some(project_path)) {
                    found_synced = true;
                    if seen.insert((project.name.clone(), ide_def.display_name.to_string())) {
                        rows.push((
                            project.name.clone(),
                            ide_def.display_name.to_string(),
                            i18n::t("deployment.deployed").to_string(),
                            theme::SUCCESS(),
                        ));
                    }
                }
            }

            if !found_synced {
                let ides: Vec<String> = if project.ides.is_empty() {
                    if global_targets.is_empty() {
                        vec![i18n::t("common.none").to_string()]
                    } else {
                        global_targets.clone()
                    }
                } else {
                    project.ides.clone()
                };
                for ide_id in ides {
                    let ide_label = ide::find(&ide_id)
                        .map(|i| i.display_name.to_string())
                        .unwrap_or(ide_id);
                    if seen.insert((project.name.clone(), ide_label.clone())) {
                        rows.push((
                            project.name.clone(),
                            ide_label,
                            i18n::t("deployment.not_synced").to_string(),
                            theme::WARNING(),
                        ));
                    }
                }
            }
        }
    }

    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    rows
}

fn code_value(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(RichText::new(text).size(12.0).monospace().color(color));
}

// ── Transport key-value section ──

fn render_transport_kv(ui: &mut egui::Ui, server: &McpServer, pal: &theme::Palette) {
    match &server.transport {
        McpTransport::Stdio {
            command,
            args,
            env,
            cwd,
            bundle,
        } => {
            ui::detail_kv_row(ui, i18n::t("detail.type"), |ui| {
                ui::pill(ui, "stdio", theme::ACCENT());
            });
            ui::detail_kv_row(ui, i18n::t("detail.command"), |ui| {
                code_value(ui, &format!("{} {}", command, args.join(" ")), pal.text);
            });
            if let Some(c) = cwd {
                ui::detail_kv_row(ui, "CWD", |ui| {
                    code_value(ui, c, pal.text);
                });
            }
            if !env.is_empty() {
                ui::detail_kv_row(ui, "Env", |ui| {
                    for (k, v) in env {
                        ui.label(
                            RichText::new(format!("{k}={v}"))
                                .size(11.0)
                                .monospace()
                                .color(pal.text_sec),
                        );
                    }
                });
            }
            if let Some(b) = bundle {
                ui::detail_kv_row(ui, "Bundle", |ui| {
                    code_value(ui, b, pal.text);
                });
            }
        }
        McpTransport::Http { url, headers } | McpTransport::Sse { url, headers } => {
            let kind = if matches!(&server.transport, McpTransport::Http { .. }) {
                "http"
            } else {
                "sse"
            };
            ui::detail_kv_row(ui, i18n::t("detail.type"), |ui| {
                ui::pill(ui, kind, theme::SUCCESS());
            });
            ui::detail_kv_row(ui, "URL", |ui| {
                code_value(ui, url, pal.text);
            });
            if !headers.is_empty() {
                ui::detail_kv_row(ui, "Headers", |ui| {
                    for (k, v) in headers {
                        let masked = if v.len() > 8 {
                            format!("{}...", &v[..8])
                        } else {
                            v.clone()
                        };
                        ui.label(
                            RichText::new(format!("{k}: {masked}"))
                                .size(11.0)
                                .monospace()
                                .color(pal.text_sec),
                        );
                    }
                });
            }
        }
    }
}

// ── Target IDE two-column grid ──

// ── Unified actions (single IDE combo + scope + deploy/sync + disable) ──

fn render_unified_actions(ui: &mut egui::Ui, app: &mut App, s: &McpServer) {
    use aiem_core::projects::ProjectStore;

    let store = ProjectStore::load().ok();
    let projects: Vec<(String, String)> = store
        .as_ref()
        .map(|st| {
            st.list()
                .map(|p| (p.path.clone(), p.name.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut selected_ide = selected_mcp_ide(app, s);
    let scope_key = s.name.clone();
    let mut scope_val = app
        .mcp_state
        .deploy_scope
        .get(&scope_key)
        .cloned()
        .unwrap_or_else(|| "global".into());
    let is_global = scope_val == "global";
    let project_path = (!is_global).then(|| std::path::PathBuf::from(&scope_val));
    let is_deployed = if let Some(path) = project_path.as_deref() {
        mcp_is_synced(&s.name, &selected_ide, Some(path))
    } else {
        mcp_is_synced(&s.name, &selected_ide, None)
    };

    let ide_snap = selected_ide.clone();
    let scope_snap = scope_val.clone();
    ui::detail_action_panel(
        ui,
        |ui| {
            let widths = mcp_detail_action_widths(ui.available_width());
            mcp_ide_combo(
                ui,
                &format!("d-mcp-ide-{}", s.name),
                &mut selected_ide,
                widths.ide,
            );
            let scope_label = if is_global {
                i18n::t("common.global").to_string()
            } else {
                projects
                    .iter()
                    .find(|(p, _)| p == &scope_val)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_else(|| scope_val.clone())
            };
            egui::ComboBox::from_id_source(format!("d-mcp-scope-{}", s.name))
                .selected_text(RichText::new(&scope_label).size(12.0))
                .width(widths.scope)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut scope_val,
                        "global".to_string(),
                        i18n::t("common.global"),
                    );
                    for (path, name) in &projects {
                        let mark = if mcp_is_synced(
                            &s.name,
                            &selected_ide,
                            Some(std::path::Path::new(path)),
                        ) {
                            "  \u{2713}"
                        } else {
                            ""
                        };
                        ui.selectable_value(&mut scope_val, path.clone(), format!("{name}{mark}"));
                    }
                });

            if is_global {
                if is_deployed {
                    if ui::compact_danger_button(ui, i18n::t("common.remove"), widths.deploy)
                        .clicked()
                    {
                        match tasks::mcp_retract_one_global_from_ide(&s.name, &ide_snap) {
                            Ok(_) => {
                                app.toast_info("retracted");
                                app.reload_mcp();
                            }
                            Err(e) => app.toast_error(format!("{e}")),
                        }
                    }
                } else if ui::compact_primary_button(ui, i18n::t("common.deploy"), widths.deploy)
                    .clicked()
                {
                    match tasks::mcp_sync_one_global(&s.name, &[ide_snap.clone()]) {
                        Ok(_) => {
                            app.toast_info(format!("synced -> {}", ide_snap));
                            app.reload_mcp();
                        }
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                }
            } else if !scope_snap.is_empty() && scope_snap != "global" {
                let path = std::path::PathBuf::from(&scope_snap);
                if is_deployed {
                    if ui::compact_danger_button(ui, i18n::t("common.remove"), widths.deploy)
                        .clicked()
                    {
                        app.mcp_state.deployed_cache.remove(&s.name);
                        match tasks::mcp_undeploy_from_project_for_ide(&s.name, &ide_snap, &path) {
                            Ok(_) => {
                                app.toast_info("undeployed");
                                app.reload_mcp();
                            }
                            Err(e) => app.toast_error(format!("{e}")),
                        }
                    }
                } else if ui::compact_primary_button(ui, i18n::t("common.deploy"), widths.deploy)
                    .clicked()
                {
                    app.mcp_state.deployed_cache.remove(&s.name);
                    match tasks::mcp_deploy_to_project_for_ide(&s.name, &ide_snap, &path) {
                        Ok(touched) => {
                            app.toast_info(format!("deployed ({} files)", touched.len()));
                            app.reload_mcp();
                        }
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                }
            }

            let disabled_label = if s.disabled {
                i18n::t("common.enable")
            } else {
                i18n::t("common.disable")
            };
            if ui::compact_secondary_button_enabled(ui, disabled_label, widths.toggle, is_deployed)
                .clicked()
            {
                match tasks::mcp_toggle(&s.name, !s.disabled) {
                    Ok(_) => app.reload_mcp(),
                    Err(e) => app.toast_error(format!("{e}")),
                }
            }
        },
        |_| {},
    );

    app.mcp_state.deploy_ide.insert(
        format!("mcp-ide-{}", s.name),
        aiem_core::mcp::adapters::canonical_id(&selected_ide).to_string(),
    );
    app.mcp_state.deploy_scope.insert(scope_key, scope_val);
}

// ── Runtime / Bundle / Auth / Source key-value section ──

struct McpDetailActionWidths {
    ide: f32,
    scope: f32,
    deploy: f32,
    toggle: f32,
}

fn mcp_detail_action_widths(available_width: f32) -> McpDetailActionWidths {
    let gap = 6.0;
    let usable = (available_width - gap * 3.0).max(0.0);
    let ide = usable * 0.35;
    let scope = usable * 0.25;
    let deploy = usable * 0.20;
    let toggle = (usable - ide - scope - deploy).max(0.0);
    McpDetailActionWidths {
        ide,
        scope,
        deploy,
        toggle,
    }
}

fn render_runtime_kv(ui: &mut egui::Ui, server: &McpServer, pal: &theme::Palette) {
    let has_bundle = matches!(
        &server.transport,
        McpTransport::Stdio {
            bundle: Some(_),
            ..
        }
    );
    let has_any = has_bundle
        || server.source.is_some()
        || server.runtime.is_some()
        || server.auth_mode != McpAuthMode::None;
    if !has_any {
        return;
    }

    ui::detail_section(ui, i18n::t("mcp.detail_runtime"), |ui| {
        if let McpTransport::Stdio {
            bundle: Some(b), ..
        } = &server.transport
        {
            let exists = aiem_core::mcp::bundles::bundle_path(b)
                .map(|p| p.exists())
                .unwrap_or(false);
            let (label, color) = if exists {
                (
                    format!("{}: {b}", i18n::t("mcp.bundle_imported")),
                    theme::SUCCESS(),
                )
            } else {
                (
                    format!("{}: {b}", i18n::t("mcp.bundle_missing")),
                    theme::DANGER(),
                )
            };
            ui::detail_kv_row(ui, i18n::t("mcp.detail_bundle"), |ui| {
                ui::pill(ui, &label, color);
            });
        }

        if let Some(rt) = server.runtime {
            let rt_str = match rt {
                McpRuntime::Python => "Python",
                McpRuntime::Node => "Node.js",
                McpRuntime::Other => "Other",
            };
            ui::detail_kv_row(ui, i18n::t("mcp.detail_runtime"), |ui| {
                ui.label(RichText::new(rt_str).size(12.0).color(pal.text));
            });
        }

        let auth_label = match &server.auth_mode {
            McpAuthMode::None => i18n::t("mcp.auth_none"),
            McpAuthMode::SecretRef => i18n::t("mcp.auth_secret"),
            McpAuthMode::External => i18n::t("mcp.auth_external"),
            McpAuthMode::MissingSecret => i18n::t("mcp.auth_missing"),
        };
        let auth_color = match &server.auth_mode {
            McpAuthMode::None => pal.text_sec,
            McpAuthMode::SecretRef => theme::SUCCESS(),
            McpAuthMode::External => theme::WARNING(),
            McpAuthMode::MissingSecret => theme::DANGER(),
        };
        ui::detail_kv_row(ui, i18n::t("mcp.detail_auth"), |ui| {
            ui.label(RichText::new(auth_label).size(12.0).color(auth_color));
        });

        if let Some(src) = &server.source {
            ui::detail_kv_row(ui, i18n::t("mcp.detail_source"), |ui| {
                ui.label(
                    RichText::new(format!("{}/{}", src.owner, src.repo))
                        .size(12.0)
                        .monospace()
                        .color(pal.text_sec),
                );
                if let Some(commit) = &src.commit {
                    ui.label(
                        RichText::new(format!("@ {}", &commit[..commit.len().min(12)]))
                            .size(11.0)
                            .monospace()
                            .color(pal.text_sec),
                    );
                }
            });
        }
    });
}

fn render_add(ui: &mut egui::Ui, app: &mut App) {
    ui::settings_group(ui, i18n::t("mcp.register"), |ui| {
        let pal = theme::p();
        ui.label(
            RichText::new(i18n::t("mcp.json_hint"))
                .size(12.0)
                .color(pal.text_sec),
        );
        ui.add_space(4.0);

        if app.mcp_state.json_input.is_empty() {
            app.mcp_state.json_input = TEMPLATE.to_string();
        }

        ui.add(
            egui::TextEdit::multiline(&mut app.mcp_state.json_input)
                .desired_width(ui.available_width() - 10.0)
                .desired_rows(12)
                .code_editor()
                .hint_text("{ ... }"),
        );

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui::primary_button(ui, i18n::t("common.save")).clicked() {
                match parse_json_servers(&app.mcp_state.json_input) {
                    Ok(servers) => {
                        let count = servers.len();
                        let mut reg = match McpRegistry::load() {
                            Ok(r) => r,
                            Err(e) => {
                                app.toast_error(format!("{e}"));
                                return;
                            }
                        };
                        for s in servers {
                            reg.upsert(s);
                        }
                        if let Err(e) = reg.save() {
                            app.toast_error(format!("{e}"));
                        } else {
                            app.toast_info(format!("saved {count} server(s)"));
                            app.reload_mcp();
                            app.mcp_state.add_open = false;
                            app.mcp_state.json_input.clear();
                        }
                    }
                    Err(e) => app.toast_error(e),
                }
            }
            if ui::secondary_button(ui, i18n::t("common.cancel")).clicked() {
                app.mcp_state.add_open = false;
                app.mcp_state.json_input.clear();
            }
        });
    });
}

const TEMPLATE: &str = r#"{
  "server-name": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\"],
    "env": {}
  }
}"#;

fn parse_json_servers(input: &str) -> Result<Vec<McpServer>, String> {
    let val: serde_json::Value =
        serde_json::from_str(input.trim()).map_err(|e| format!("JSON parse error: {e}"))?;
    let obj = val.as_object().ok_or("Expected a JSON object")?;
    let mut servers = Vec::new();
    if obj.contains_key("command") || obj.contains_key("url") {
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        servers.push(json_to_server(&name, &val)?);
    } else {
        for (name, config) in obj {
            servers.push(json_to_server(name, config)?);
        }
    }
    if servers.is_empty() {
        return Err("No servers found in JSON".into());
    }
    Ok(servers)
}

fn json_to_server(name: &str, val: &serde_json::Value) -> Result<McpServer, String> {
    let obj = val.as_object().ok_or(format!("{name}: expected object"))?;
    let transport = if let Some(cmd) = obj.get("command").and_then(|v| v.as_str()) {
        let args: Vec<String> = obj
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let env: BTreeMap<String, String> = obj
            .get("env")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let cwd = obj.get("cwd").and_then(|v| v.as_str()).map(String::from);
        let bundle = obj.get("bundle").and_then(|v| v.as_str()).map(String::from);
        McpTransport::Stdio {
            command: cmd.to_string(),
            args,
            env,
            cwd,
            bundle,
        }
    } else if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
        let headers: BTreeMap<String, String> = obj
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("sse");
        if kind == "http" {
            McpTransport::Http {
                url: url.to_string(),
                headers,
            }
        } else {
            McpTransport::Sse {
                url: url.to_string(),
                headers,
            }
        }
    } else {
        return Err(format!("{name}: need 'command' or 'url'"));
    };
    let targets: Vec<String> = obj
        .get("targets")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(default_mcp_targets);
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);
    Ok(McpServer {
        name: name.to_string(),
        transport,
        targets,
        description,
        tags: vec![],
        disabled: false,
        source: None,
        runtime: None,
        auth_mode: Default::default(),
    })
}

fn default_mcp_targets() -> Vec<String> {
    aiem_core::mcp::adapters::SUPPORTED
        .iter()
        .map(|ide| ide.to_string())
        .collect()
}

fn render_github_import(ui: &mut egui::Ui, app: &mut App) {
    ui::settings_group(ui, i18n::t("mcp.gh_title"), |ui| {
        let pal = theme::p();

        // --- Input row (borrows app.mcp_state.github_import) ---
        ui.horizontal(|ui| {
            let gi = &mut app.mcp_state.github_import;
            ui.label(
                RichText::new(i18n::t("mcp.gh_repo"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            ui.add(
                egui::TextEdit::singleline(&mut gi.repo_input)
                    .desired_width(220.0)
                    .hint_text("modelcontextprotocol/servers"),
            );
            ui.label(
                RichText::new(i18n::t("mcp.gh_ref"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            ui.add(
                egui::TextEdit::singleline(&mut gi.ref_input)
                    .desired_width(100.0)
                    .hint_text("main"),
            );

            let can_analyze = !gi.repo_input.trim().is_empty() && !gi.loading;
            if ui
                .add_enabled(can_analyze, egui::Button::new(i18n::t("mcp.gh_analyze")))
                .clicked()
            {
                let repo = gi.repo_input.trim().to_string();
                let r#ref = if gi.ref_input.trim().is_empty() {
                    "main".to_string()
                } else {
                    gi.ref_input.trim().to_string()
                };
                gi.error = None;
                gi.preview = None;
                gi.imported = None;

                let (owner, name) = match repo.split_once('/') {
                    Some((o, n)) => (o.to_string(), n.to_string()),
                    None => {
                        gi.error = Some("Expected owner/repo format".into());
                        return;
                    }
                };

                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        gi.error = Some(format!("Runtime error: {e}"));
                        return;
                    }
                };
                match rt.block_on(aiem_core::mcp::github::preview_github_mcp(
                    &owner,
                    &name,
                    Some(&r#ref),
                    None,
                )) {
                    Ok(preview) => {
                        let names: BTreeSet<String> = preview
                            .servers
                            .iter()
                            .map(|ps| ps.server.name.clone())
                            .collect();
                        gi.selected = names;
                        gi.preview = Some(preview);
                    }
                    Err(e) => gi.error = Some(format!("{e}")),
                }
            }
        });

        // --- Status messages ---
        if let Some(err) = app.mcp_state.github_import.error.clone() {
            ui.add_space(4.0);
            ui.label(RichText::new(&err).size(12.0).color(theme::DANGER()));
        }

        // --- Clone preview data so we can render without holding a borrow on app ---
        let preview = app.mcp_state.github_import.preview.clone();
        let imported_result = app.mcp_state.github_import.imported.clone();

        if let Some(ref preview) = preview {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "{}/{}  {} {}",
                    preview.owner,
                    preview.repo,
                    i18n::t("mcp.gh_commit"),
                    &preview.commit[..preview.commit.len().min(12)]
                ))
                .size(12.0)
                .monospace()
                .color(pal.text_sec),
            );
            for w in &preview.warnings {
                ui.label(
                    RichText::new(format!("\u{26a0} {w}"))
                        .size(12.0)
                        .color(theme::WARNING()),
                );
            }

            if preview.servers.is_empty() {
                ui.label(
                    RichText::new(i18n::t("mcp.gh_no_servers"))
                        .size(12.0)
                        .color(pal.text_sec),
                );
            } else {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(i18n::t("mcp.gh_servers"))
                        .size(13.0)
                        .strong()
                        .color(pal.text),
                );

                for ps in &preview.servers {
                    ui.add_space(4.0);
                    let name = ps.server.name.clone();
                    let mut checked = app.mcp_state.github_import.selected.contains(&name);
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut checked, "");
                        ui.vertical(|ui| {
                            let rt_str = ps
                                .server
                                .runtime
                                .map(|r| format!("{r:?}"))
                                .unwrap_or_else(|| "unknown".into());
                            ui.label(
                                RichText::new(&ps.server.name)
                                    .size(13.0)
                                    .strong()
                                    .color(pal.text),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{}: {rt_str}",
                                        i18n::t("mcp.gh_runtime")
                                    ))
                                    .size(11.0)
                                    .color(pal.text_sec),
                                );
                                if let Some(ep) = &ps.entrypoint {
                                    ui.label(
                                        RichText::new(format!(
                                            "  {}: {ep}",
                                            i18n::t("mcp.gh_entry")
                                        ))
                                        .size(11.0)
                                        .monospace()
                                        .color(pal.text_sec),
                                    );
                                }
                            });
                            ui.label(
                                RichText::new(format!(
                                    "{}: {}  |  {}: {}",
                                    i18n::t("mcp.gh_kept"),
                                    ps.kept_files.len(),
                                    i18n::t("mcp.gh_dropped"),
                                    ps.dropped_files.len(),
                                ))
                                .size(11.0)
                                .color(pal.text_sec),
                            );
                            if !ps.detected_secrets.is_empty() {
                                ui.label(
                                    RichText::new(format!(
                                        "{}: {}",
                                        i18n::t("mcp.gh_secrets"),
                                        ps.detected_secrets.len()
                                    ))
                                    .size(11.0)
                                    .color(theme::WARNING()),
                                );
                            }
                            for w in &ps.warnings {
                                ui.label(
                                    RichText::new(format!("\u{26a0} {w}"))
                                        .size(11.0)
                                        .color(theme::WARNING()),
                                );
                            }
                        });
                    });
                    if checked {
                        app.mcp_state.github_import.selected.insert(name);
                    } else {
                        app.mcp_state.github_import.selected.remove(&name);
                    }
                }

                // --- Import / Cancel buttons ---
                ui.add_space(8.0);
                let selected = app.mcp_state.github_import.selected.clone();
                let can_import = !selected.is_empty();
                let mut do_import = false;
                let mut do_cancel = false;

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_import, egui::Button::new(i18n::t("mcp.gh_import")))
                        .clicked()
                    {
                        do_import = true;
                    }
                    if ui::secondary_button(ui, i18n::t("common.cancel")).clicked() {
                        do_cancel = true;
                    }
                });

                if do_import {
                    let rt = tokio::runtime::Runtime::new().ok();
                    if let Some(rt) = rt {
                        match rt.block_on(aiem_core::mcp::github::import_github_mcp(
                            preview,
                            Some(&selected),
                        )) {
                            Ok(imported) => {
                                app.mcp_state.github_import.imported = Some(imported);
                                app.reload_mcp();
                            }
                            Err(e) => app.mcp_state.github_import.error = Some(format!("{e}")),
                        }
                    }
                }
                if do_cancel {
                    app.mcp_state.github_import = GithubImportState::default();
                }
            }

            if let Some(ref imported) = imported_result {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "{}: {}",
                        i18n::t("mcp.gh_imported"),
                        imported.len()
                    ))
                    .size(13.0)
                    .strong()
                    .color(theme::SUCCESS()),
                );
                for name in imported {
                    ui.label(
                        RichText::new(format!("  \u{2713} {name}"))
                            .size(12.0)
                            .color(pal.text),
                    );
                }
            }
        }
    });
}

fn render_bundles(ui: &mut egui::Ui, app: &mut App) {
    ui::settings_group(ui, i18n::t("mcp.bundles_title"), |ui| {
        let pal = theme::p();
        ui.label(
            RichText::new(i18n::t("mcp.bundles_hint"))
                .size(12.0)
                .color(pal.text_sec),
        );
        ui.add_space(6.0);

        let bundles_dir = match aiem_core::paths::mcp_bundles_dir() {
            Ok(p) => p,
            Err(e) => {
                ui.label(RichText::new(format!("error: {e}")).color(theme::DANGER()));
                return;
            }
        };
        ui.label(
            RichText::new(bundles_dir.to_string_lossy().into_owned())
                .size(11.0)
                .monospace()
                .color(pal.text_sec),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(i18n::t("skills.name"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.mcp_state.bundle_name_input)
                    .desired_width(120.0)
                    .hint_text("my-mcp"),
            );
            ui.label(
                RichText::new(i18n::t("detail.source"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.mcp_state.bundle_src_input)
                    .desired_width(220.0)
                    .hint_text("C:\\path\\to\\local\\mcp"),
            );
            if ui::secondary_button(ui, i18n::t("common.pick")).clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    app.mcp_state.bundle_src_input = p.to_string_lossy().into_owned();
                }
            }
            let can_import = !app.mcp_state.bundle_name_input.trim().is_empty()
                && !app.mcp_state.bundle_src_input.trim().is_empty();
            if ui
                .add_enabled(can_import, egui::Button::new(i18n::t("common.import")))
                .clicked()
            {
                let name = app.mcp_state.bundle_name_input.trim().to_string();
                let src = std::path::PathBuf::from(app.mcp_state.bundle_src_input.trim());
                match aiem_core::mcp::bundles::import_bundle(&name, &src) {
                    Ok(p) => {
                        app.toast_info(format!("bundle imported to {}", p.display()));
                        app.mcp_state.bundle_name_input.clear();
                        app.mcp_state.bundle_src_input.clear();
                    }
                    Err(e) => app.toast_error(format!("{e}")),
                }
            }
        });
        ui.add_space(6.0);

        let bundles = aiem_core::mcp::bundles::list_bundles().unwrap_or_default();
        if bundles.is_empty() {
            ui.label(
                RichText::new(i18n::t("mcp.no_bundles"))
                    .size(12.0)
                    .color(pal.text_sec),
            );
        } else {
            for b in &bundles {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(b).monospace().size(12.0).color(pal.text));
                    if ui
                        .small_button(
                            RichText::new(i18n::t("common.delete")).color(theme::DANGER()),
                        )
                        .clicked()
                    {
                        match aiem_core::mcp::bundles::remove_bundle(b) {
                            Ok(_) => app.toast_info(format!("bundle `{b}` removed")),
                            Err(e) => app.toast_error(format!("{e}")),
                        }
                    }
                });
            }
        }
    });
}
