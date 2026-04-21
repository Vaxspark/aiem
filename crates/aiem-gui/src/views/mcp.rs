use std::collections::BTreeMap;

use aiem_core::mcp::adapters;
use aiem_core::mcp::model::{McpServer, McpTransport};
use aiem_core::mcp::McpRegistry;
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, primary_button, App};
use crate::i18n;
use crate::tasks;
use crate::theme;

#[derive(Default)]
pub struct State {
    pub add_open: bool,
    /// JSON/code block input for adding servers
    pub json_input: String,
    pub filter: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    page_header(ui, i18n::t("mcp.title"), i18n::t("mcp.subtitle"), |ui| {
        if primary_button(ui, i18n::t("mcp.sync")).clicked() {
            match tasks::mcp_sync_all(None) {
                Ok(touched) => {
                    for (ide, path) in &touched {
                        app.toast_info(format!("{ide}: {}", path.display()));
                    }
                    if touched.is_empty() { app.toast_info("nothing to sync"); }
                    app.reload_mcp();
                }
                Err(e) => app.toast_error(format!("sync: {e}")),
            }
        }
        if ui.button(i18n::t("mcp.new")).clicked() {
            app.mcp_state.add_open = !app.mcp_state.add_open;
        }
    });

    if app.mcp_state.add_open {
        render_add(ui, app);
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new(i18n::t("mcp.filter")).color(theme::MUTED()));
        ui.add(egui::TextEdit::singleline(&mut app.mcp_state.filter)
            .desired_width((ui.available_width() - 20.0).min(300.0).max(120.0))
            .hint_text("name"));
    });
    ui.add_space(8.0);

    let filter = app.mcp_state.filter.to_ascii_lowercase();
    let servers: Vec<_> = app.mcp.list().cloned().collect();
    let total = servers.len();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let mut shown = 0;
        for s in &servers {
            if !filter.is_empty() && !s.name.to_lowercase().contains(&filter) { continue; }
            shown += 1;
            render_server_card(ui, app, s);
        }
        if total == 0 {
            empty_state(ui, "No MCP servers yet", "Click \"New server\" to register one.");
        } else if shown == 0 {
            empty_state(ui, "No matches", "Try a different filter.");
        }
    });

    ui.add_space(10.0);
    config_paths_summary(ui);
}

fn render_add(ui: &mut egui::Ui, app: &mut App) {
    card(ui, |ui| {
        ui.label(RichText::new(i18n::t("mcp.register")).strong().color(theme::TEXT()));
        ui.add_space(4.0);
        ui.label(RichText::new("Paste a JSON block — same format as Claude/Codex config:")
            .small().color(theme::MUTED()));
        ui.add_space(4.0);

        if app.mcp_state.json_input.is_empty() {
            app.mcp_state.json_input = TEMPLATE.to_string();
        }

        ui.add(
            egui::TextEdit::multiline(&mut app.mcp_state.json_input)
                .desired_width(ui.available_width() - 10.0)
                .desired_rows(14)
                .code_editor()
                .hint_text("{ ... }"),
        );

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if primary_button(ui, i18n::t("mcp.save")).clicked() {
                match parse_json_servers(&app.mcp_state.json_input) {
                    Ok(servers) => {
                        let count = servers.len();
                        let mut reg = match McpRegistry::load() {
                            Ok(r) => r,
                            Err(e) => { app.toast_error(format!("{e}")); return; }
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
            if ui.button(i18n::t("common.cancel")).clicked() {
                app.mcp_state.add_open = false;
                app.mcp_state.json_input.clear();
            }
        });
    });
    ui.add_space(14.0);
}

const TEMPLATE: &str = r#"{
  "server-name": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\"],
    "env": {}
  }
}"#;

/// Parse JSON input into McpServer(s). Supports:
/// 1. Single object with "command"/"url" -> one server named from the key or "name" field
/// 2. Map of name -> config (like Claude/Codex format)
fn parse_json_servers(input: &str) -> Result<Vec<McpServer>, String> {
    let val: serde_json::Value = serde_json::from_str(input.trim())
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let obj = val.as_object().ok_or("Expected a JSON object")?;

    let mut servers = Vec::new();

    // Check if this is a single server (has "command" or "url" at top level)
    if obj.contains_key("command") || obj.contains_key("url") {
        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        servers.push(json_to_server(&name, &val)?);
    } else {
        // Map of name -> server config
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
        let args: Vec<String> = obj.get("args")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let env: BTreeMap<String, String> = obj.get("env")
            .and_then(|v| v.as_object())
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default();
        let cwd = obj.get("cwd").and_then(|v| v.as_str()).map(String::from);
        McpTransport::Stdio { command: cmd.to_string(), args, env, cwd }
    } else if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
        let headers: BTreeMap<String, String> = obj.get("headers")
            .and_then(|v| v.as_object())
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default();
        // Guess SSE vs HTTP from URL or type field
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("sse");
        if kind == "http" {
            McpTransport::Http { url: url.to_string(), headers }
        } else {
            McpTransport::Sse { url: url.to_string(), headers }
        }
    } else {
        return Err(format!("{name}: need 'command' (stdio) or 'url' (http/sse)"));
    };

    let targets: Vec<String> = obj.get("targets")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["codex".into(), "claude-code".into(), "copilot".into()]);

    let description = obj.get("description").and_then(|v| v.as_str()).map(String::from);

    Ok(McpServer {
        name: name.to_string(),
        transport,
        targets,
        description,
        tags: vec![],
        disabled: false,
    })
}

fn render_server_card(ui: &mut egui::Ui, app: &mut App, s: &McpServer) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&s.name).strong().size(16.0).color(theme::TEXT()));
                    let (kind, color) = match &s.transport {
                        McpTransport::Stdio { .. } => ("stdio", theme::ACCENT()),
                        McpTransport::Http { .. }  => ("http",  theme::SUCCESS()),
                        McpTransport::Sse { .. }   => ("sse",   theme::SUCCESS()),
                    };
                    theme::tag(ui, kind, color);
                    if s.disabled {
                        theme::tag(ui, "disabled", theme::DANGER());
                    }
                });
                let detail = match &s.transport {
                    McpTransport::Stdio { command, args, .. } => {
                        format!("{} {}", command, args.join(" "))
                    }
                    McpTransport::Http { url, .. } | McpTransport::Sse { url, .. } => url.clone(),
                };
                ui.label(RichText::new(detail).monospace().small().color(theme::MUTED()));
                if let Some(d) = &s.description {
                    ui.label(RichText::new(d).color(theme::MUTED()).small());
                }
                if !s.targets.is_empty() {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        for t in &s.targets { theme::tag(ui, t, theme::SUCCESS()); }
                    });
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let btn = egui::Button::new(RichText::new("Remove").color(theme::DANGER()))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).on_hover_text("remove").clicked() {
                    match tasks::mcp_remove(&s.name) {
                        Ok(_) => { app.toast_info("removed (run Sync to retract)"); app.reload_mcp(); }
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                }
                let (label, hint) = if s.disabled { ("Enable", "enable") } else { ("Disable", "disable") };
                let btn = egui::Button::new(RichText::new(label))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).on_hover_text(hint).clicked() {
                    match tasks::mcp_toggle(&s.name, !s.disabled) {
                        Ok(_) => { app.reload_mcp(); }
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                }
            });
        });
    });
    ui.add_space(10.0);
}

fn config_paths_summary(ui: &mut egui::Ui) {
    ui.collapsing(RichText::new("IDE config paths").color(theme::MUTED()), |ui| {
        for ide in adapters::SUPPORTED {
            match adapters::config_path(ide, None) {
                Ok(p) => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(*ide).strong().color(theme::TEXT()));
                        ui.label(RichText::new("->").color(theme::MUTED()));
                        ui.label(RichText::new(p.to_string_lossy()).monospace().small().color(theme::MUTED()));
                    });
                }
                Err(_) => {}
            }
        }
    });
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
