use aiem_core::ide;
use aiem_core::skills::model::{Skill, SkillSource};
use eframe::egui::{self, FontId, RichText};

use crate::app::App;
use crate::i18n;
use crate::tasks;
use crate::theme;
use crate::ui;

const GROUP_ACTIONS_W: f32 = 382.0;
const GROUP_ACTION_RIGHT_NUDGE: f32 = 14.0;
const SKILL_MD_PREVIEW_H: f32 = 350.0;

pub fn short_id(id: &str) -> &str {
    let tail = if let Some(pos) = id.rfind("__") {
        &id[pos + 2..]
    } else {
        id
    };
    tail.rsplit(|c: char| c == '/' || c == '\\' || c == '_')
        .find(|s| !s.is_empty())
        .unwrap_or(tail)
}

#[derive(Default)]
pub struct State {
    pub add_source: String,
    pub add_ref: String,
    pub add_subdir: String,
    pub add_name: String,
    pub add_open: bool,
    pub create_open: bool,
    pub create_name: String,
    pub create_content: String,
    pub filter: String,
    pub deploy_ide: std::collections::HashMap<String, String>,
    pub deploy_scope: std::collections::HashMap<String, String>,
    pub link_github_input: String,
    pub deployed_projects_cache: std::collections::HashMap<String, Vec<String>>,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_toolbar(
        ui,
        i18n::t("skills.title"),
        i18n::t("skills.subtitle"),
        |ui| {
            if ui::primary_button(ui, i18n::t("skills.add")).clicked() {
                app.skills_state.add_open = !app.skills_state.add_open;
                app.skills_state.create_open = false;
            }
            if ui::secondary_button(ui, i18n::t("skills.new")).clicked() {
                app.skills_state.create_open = !app.skills_state.create_open;
                app.skills_state.add_open = false;
            }
            if ui::danger_button(ui, i18n::t("skills.clear_global")).clicked() {
                match tasks::clear_all_global_skills() {
                    Ok(n) => {
                        app.toast_info(format!("cleared {n} global deployment(s)"));
                        app.reload_skills();
                    }
                    Err(e) => app.toast_error(format!("{e}")),
                }
            }
        },
    );

    if app.skills_state.add_open {
        render_add_form(ui, app);
    }
    if app.skills_state.create_open {
        render_create_form(ui, app);
    }

    ui::search_bar(
        ui,
        &mut app.skills_state.filter,
        i18n::t("skills.search_hint"),
    );
    ui.add_space(8.0);

    let filter = app.skills_state.filter.to_ascii_lowercase();
    let items: Vec<_> = app.skills.list().cloned().collect();
    let total = items.len();

    let mut groups: std::collections::BTreeMap<String, Vec<Skill>> =
        std::collections::BTreeMap::new();
    for skill in &items {
        if !filter.is_empty()
            && !skill.id.to_lowercase().contains(&filter)
            && !skill.name.to_lowercase().contains(&filter)
        {
            continue;
        }
        let group_key = match &skill.source {
            SkillSource::GitHub { owner, repo, .. } => format!("{}/{}", owner, repo),
            _ => format!("({})", i18n::t("skills.local")),
        };
        groups.entry(group_key).or_default().push(skill.clone());
    }
    let shown: usize = groups.values().map(|v| v.len()).sum();

    egui::ScrollArea::vertical()
        .id_source("skills-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if total == 0 {
                ui::empty_state(ui, i18n::t("skills.empty"), i18n::t("skills.empty_sub"));
            } else if shown == 0 {
                ui::empty_state(
                    ui,
                    i18n::t("skills.no_match"),
                    i18n::t("skills.no_match_sub"),
                );
            } else {
                for (group_name, skills) in &groups {
                    render_group(ui, app, group_name, skills);
                }
            }
        });
}

fn render_group(ui: &mut egui::Ui, app: &mut App, group_name: &str, skills: &[Skill]) {
    let pal = theme::p();
    let display_name = group_name.rsplit('/').next().unwrap_or(group_name);
    let header = if skills.len() > 1 {
        format!("{}  ({})", display_name, skills.len())
    } else {
        display_name.to_string()
    };

    let id = ui.make_persistent_id(format!("grp-{}", group_name));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
        .show_header(ui, |ui| {
            let row_w = (ui.available_width()
                - ui::LIST_GUTTER
                - ui.spacing().indent
                - GROUP_ACTION_RIGHT_NUDGE)
                .max(160.0);
            ui.allocate_ui_with_layout(
                egui::vec2(row_w, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let actions_w = if skills.len() > 1 {
                        GROUP_ACTIONS_W
                    } else {
                        0.0
                    };
                    let title_w = (row_w - actions_w - 8.0).max(44.0);
                    paint_group_title(ui, &header, title_w, pal.text);
                    if skills.len() > 1 {
                        ui.add_space((row_w - title_w - actions_w).max(0.0));
                        ui.allocate_ui_with_layout(
                            egui::vec2(actions_w, 28.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| render_group_actions(ui, app, skills),
                        );
                    }
                },
            );
        })
        .body(|ui| {
            for skill in skills {
                let is_selected = app.selected_skill.as_deref() == Some(&skill.id);
                let skill_id = skill.id.clone();
                let resp = ui::resource_row(ui, &format!("sk-{}", skill.id), is_selected, |ui| {
                    render_skill_row(ui, app, skill);
                });
                if resp.clicked() {
                    if is_selected {
                        app.selected_skill = None;
                        app.detail_skill_content = None;
                        app.detail_skill_files = None;
                    } else {
                        app.select_skill(&skill_id);
                    }
                }
            }
        });
    ui.add_space(4.0);
}

fn paint_group_title(ui: &mut egui::Ui, text: &str, width: f32, color: egui::Color32) {
    let font_id = FontId::proportional(13.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 22.0), egui::Sense::hover());
    let fitted = fit_text_to_width(ui, text, width, font_id.clone(), color);
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        fitted,
        font_id,
        color,
    );
}

fn fit_text_to_width(
    ui: &egui::Ui,
    text: &str,
    width: f32,
    font_id: FontId,
    color: egui::Color32,
) -> String {
    if width <= 8.0 {
        return String::new();
    }
    let fits = |candidate: &str| {
        ui.painter()
            .layout_no_wrap(candidate.to_string(), font_id.clone(), color)
            .size()
            .x
            <= width
    };
    if fits(text) {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let mut candidate: String = chars.iter().take(mid).collect();
        candidate.push('\u{2026}');
        if fits(&candidate) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let mut result: String = chars.iter().take(lo).collect();
    result.push('\u{2026}');
    result
}

fn render_skill_row(ui: &mut egui::Ui, app: &mut App, skill: &Skill) {
    let pal = theme::p();
    let is_local = matches!(&skill.source, SkillSource::Local { .. });
    let short = short_id(&skill.id);
    let deploy_count = skill.deployments.len();
    let project_count = app
        .skills_state
        .deployed_projects_cache
        .entry(skill.id.clone())
        .or_insert_with(|| tasks::skill_projects_with(&skill.id).unwrap_or_default())
        .len();
    let source_label = match &skill.source {
        SkillSource::GitHub { owner, repo, .. } => format!("{}/{}", owner, repo),
        SkillSource::Local { .. } => i18n::t("skills.local").to_string(),
    };

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(short).size(15.0).color(pal.text));
                ui.label(
                    RichText::new(short_ver(&skill.version))
                        .size(11.0)
                        .color(pal.accent),
                );
                if is_local {
                    ui::pill(ui, i18n::t("skills.local"), pal.text_sec);
                }
            });
            ui.label(
                RichText::new(source_label)
                    .size(11.0)
                    .monospace()
                    .color(pal.text_sec),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            if project_count > 0 {
                ui::pill(ui, &format!("{} project", project_count), pal.text_sec);
            }
            if deploy_count > 0 {
                ui::pill(ui, &format!("{} IDE", deploy_count), theme::SUCCESS());
            }
        });
    });
}

fn render_group_actions(ui: &mut egui::Ui, app: &mut App, skills: &[Skill]) {
    ui.spacing_mut().item_spacing.x = 4.0;
    ui.spacing_mut().interact_size.y = ui::COMPACT_CONTROL_H;
    let group_key = skills
        .first()
        .map(|s| match &s.source {
            SkillSource::GitHub { owner, repo, .. } => format!("group:{owner}/{repo}"),
            SkillSource::Local { .. } => "group:local".to_string(),
        })
        .unwrap_or_else(|| "group".to_string());
    let mut ide_selected = app
        .skills_state
        .deploy_ide
        .entry(group_key.clone())
        .or_insert_with(|| "claude-code".to_string())
        .clone();
    if ide::find(&ide_selected).is_none() {
        ide_selected = "claude-code".to_string();
    }
    let mut scope_val = app
        .skills_state
        .deploy_scope
        .get(&group_key)
        .cloned()
        .unwrap_or_else(|| "global".to_string());
    let projects: Vec<(String, String)> = aiem_core::projects::ProjectStore::load()
        .map(|store| {
            store
                .list()
                .map(|p| (p.path.clone(), p.name.clone()))
                .collect()
        })
        .unwrap_or_default();

    let ids: Vec<String> = skills.iter().map(|s| s.id.clone()).collect();
    let project = (scope_val != "global").then(|| std::path::PathBuf::from(&scope_val));

    if ui::compact_secondary_button(ui, i18n::t("common.update"), 46.0).clicked() {
        if let Some(first) = skills
            .iter()
            .find(|s| !matches!(&s.source, SkillSource::Local { .. }))
        {
            if let SkillSource::GitHub { owner, repo, .. } = &first.source {
                app.bus.sync_group(owner.clone(), repo.clone());
            }
        }
    }
    if ui::compact_secondary_button(ui, i18n::t("skills.undeploy_all"), 66.0).clicked() {
        match tasks::undeploy_skill_group(&ids, &ide_selected, project.as_deref()) {
            Ok(n) => {
                app.toast_info(format!("undeployed {n}/{}", ids.len()));
                app.skills_state.deployed_projects_cache.clear();
                app.reload_skills();
            }
            Err(e) => app.toast_error(format!("{e}")),
        }
    }
    if ui::compact_secondary_button(ui, i18n::t("skills.deploy_all"), 66.0).clicked() {
        match tasks::deploy_skill_group(&ids, &ide_selected, project.as_deref()) {
            Ok(n) => {
                app.toast_info(format!("deployed {n}/{}", ids.len()));
                app.skills_state.deployed_projects_cache.clear();
                app.reload_skills();
            }
            Err(e) => app.toast_error(format!("{e}")),
        }
    }

    let scope_label = if scope_val == "global" {
        i18n::t("common.global").to_string()
    } else {
        projects
            .iter()
            .find(|(p, _)| p == &scope_val)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| scope_val.clone())
    };
    egui::ComboBox::from_id_source(format!("group-scope-{group_key}"))
        .selected_text(RichText::new(scope_label).size(12.0))
        .width(78.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut scope_val,
                "global".to_string(),
                i18n::t("common.global"),
            );
            for (path, name) in &projects {
                ui.selectable_value(&mut scope_val, path.clone(), name);
            }
        });

    let ide_label = ide::find(&ide_selected)
        .map(|i| i.display_name)
        .unwrap_or(ide_selected.as_str());
    egui::ComboBox::from_id_source(format!("group-ide-{group_key}"))
        .selected_text(RichText::new(ide_label).size(12.0))
        .width(110.0)
        .show_ui(ui, |ui| {
            for i in ide::IDES {
                ui.selectable_value(&mut ide_selected, i.id.to_string(), i.display_name);
            }
        });

    app.skills_state
        .deploy_ide
        .insert(group_key.clone(), ide_selected);
    app.skills_state.deploy_scope.insert(group_key, scope_val);
}

// ─── Detail panel ───────────────────────────────────────────────────────────

pub fn detail(ui: &mut egui::Ui, app: &mut App) {
    let skill_id = match &app.selected_skill {
        Some(id) => id.clone(),
        None => return,
    };
    let skill = match app.skills.get(&skill_id) {
        Some(s) => s.clone(),
        None => {
            app.selected_skill = None;
            return;
        }
    };

    let short = short_id(&skill.id);
    let is_local = matches!(&skill.source, SkillSource::Local { .. });

    if ui::detail_header(ui, short, &skill.id) {
        app.selected_skill = None;
        app.detail_skill_content = None;
        app.detail_skill_files = None;
        return;
    }

    ui::detail_scroll_body(ui, "skill-detail", |ui| {
        let pal = theme::p();

        // ── Overview ──
        ui::detail_section(ui, i18n::t("detail.overview"), |ui| {
            if let Some(d) = &skill.description {
                ui.label(RichText::new(d).size(13.0).color(pal.text));
                ui.add_space(4.0);
            }
            ui::detail_kv_row(ui, i18n::t("detail.version"), |ui| {
                ui.label(
                    RichText::new(&skill.version)
                        .size(12.0)
                        .monospace()
                        .color(pal.text),
                );
            });
            match &skill.source {
                SkillSource::GitHub {
                    owner, repo, r#ref, ..
                } => {
                    ui::detail_kv_row(ui, i18n::t("detail.source"), |ui| {
                        ui.horizontal_top(|ui| {
                            ui.label(
                                RichText::new(format!("{}/{}", owner, repo))
                                    .size(12.0)
                                    .color(pal.accent),
                            );
                            if let Some(r) = r#ref {
                                ui.label(
                                    RichText::new(format!("@{}", r))
                                        .size(11.0)
                                        .color(pal.text_sec),
                                );
                            }
                        });
                    });
                }
                SkillSource::Local { path } => {
                    ui::detail_kv_row(ui, i18n::t("detail.path"), |ui| {
                        ui.label(
                            RichText::new(path.to_string_lossy().to_string())
                                .size(12.0)
                                .monospace()
                                .color(pal.text),
                        );
                    });
                }
            }
        });

        // ── Deployments ──
        ui::detail_section(ui, i18n::t("deployment.records"), |ui| {
            let rows = skill_deployment_records(&skill);
            ui::deployment_records_table(ui, &format!("skill-deployments-{}", skill.id), &rows);
        });

        // ── SKILL.md preview ──
        if let Some(content) = &app.detail_skill_content {
            ui::detail_section(ui, "SKILL.md", |ui| {
                let (preview_w, side_gap) = ui::detail_embedded_card_metrics(ui);
                ui.horizontal(|ui| {
                    ui.add_space(side_gap);
                    ui.allocate_ui_with_layout(
                        egui::vec2(preview_w, SKILL_MD_PREVIEW_H + 20.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::Frame::none()
                                .fill(pal.surface_hi)
                                .stroke(egui::Stroke::new(1.0, pal.stroke))
                                .rounding(egui::Rounding::same(8.0))
                                .inner_margin(egui::Margin::same(10.0))
                                .show(ui, |ui| {
                                    ui.set_min_width((preview_w - 20.0).max(0.0));
                                    ui.set_min_height(SKILL_MD_PREVIEW_H);
                                    egui::ScrollArea::vertical()
                                        .max_height(SKILL_MD_PREVIEW_H)
                                        .min_scrolled_height(SKILL_MD_PREVIEW_H)
                                        .show(ui, |ui| {
                                            ui.set_max_width((preview_w - 20.0).max(0.0));
                                            ui.label(
                                                RichText::new(content.as_str())
                                                    .size(13.0)
                                                    .monospace()
                                                    .color(pal.text),
                                            );
                                        });
                                });
                        },
                    );
                });
            });
        }

        // ── Files ──
        if let Some(files) = &app.detail_skill_files {
            if !files.is_empty() {
                ui::detail_section(
                    ui,
                    &format!("{} ({})", i18n::t("detail.files"), files.len()),
                    |ui| {
                        for (path, size) in files.iter().take(30) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(path).size(12.0).monospace().color(pal.text),
                                );
                                ui.label(
                                    RichText::new(format_size(*size))
                                        .size(11.0)
                                        .color(pal.text_sec),
                                );
                            });
                        }
                        if files.len() > 30 {
                            ui.label(
                                RichText::new(format!("... +{}", files.len() - 30))
                                    .size(11.0)
                                    .color(pal.text_sec),
                            );
                        }
                    },
                );
            }
        }

        // ── Link to GitHub (local skills only) ──
        if is_local {
            ui::detail_section(ui, i18n::t("skills.link_github"), |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.skills_state.link_github_input)
                            .desired_width(ui.available_width() - 80.0)
                            .hint_text("owner/repo"),
                    );
                    if ui::primary_button(ui, i18n::t("common.link")).clicked() {
                        let input = app.skills_state.link_github_input.trim().to_string();
                        let n = aiem_core::skills::apply_github_proxy_env(&input);
                        if let Some(new_source) = SkillSource::parse_github(n) {
                            match aiem_core::skills::SkillRegistry::load() {
                                Ok(mut reg) => {
                                    if let Some(mut sk) = reg.get(&skill_id).cloned() {
                                        sk.source = new_source;
                                        reg.upsert(sk);
                                        if let Err(e) = reg.save() {
                                            app.toast_error(format!("save: {e}"));
                                        } else {
                                            app.toast_info(format!(
                                                "linked {} to GitHub",
                                                skill_id
                                            ));
                                            app.reload_skills();
                                        }
                                    }
                                }
                                Err(e) => app.toast_error(format!("{e}")),
                            }
                            app.skills_state.link_github_input.clear();
                        } else {
                            app.toast_error(i18n::t("skills.invalid_github"));
                        }
                    }
                });
            });
        }

        // ── Actions ──
        render_skill_actions(ui, app, &skill, short, is_local);

        // ── Danger Zone ──
        let sid = skill_id.clone();
        let sshort = short.to_string();
        ui::detail_danger_footer(ui, i18n::t("skills.danger_desc"), |ui| {
            if ui::fixed_danger_button(ui, i18n::t("skills.remove_skill"), 104.0).clicked() {
                match tasks::remove_skill(&sid) {
                    Ok(_) => {
                        app.toast_info(format!("removed {}", sshort));
                        app.selected_skill = None;
                        app.detail_skill_content = None;
                        app.detail_skill_files = None;
                        app.reload_skills();
                    }
                    Err(e) => app.toast_error(format!("remove: {e}")),
                }
            }
        });
    });
}

fn render_skill_actions(
    ui: &mut egui::Ui,
    app: &mut App,
    skill: &Skill,
    short: &str,
    is_local: bool,
) {
    let projects: Vec<(String, String, bool)> = aiem_core::projects::ProjectStore::load()
        .map(|store| {
            store
                .list()
                .map(|p| {
                    (
                        p.path.clone(),
                        p.name.clone(),
                        p.skills.iter().any(|s| s == &skill.id),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let mut ide_selected = app
        .skills_state
        .deploy_ide
        .entry(skill.id.clone())
        .or_insert_with(|| "claude-code".to_string())
        .clone();
    if ide::find(&ide_selected).is_none() {
        ide_selected = "claude-code".to_string();
    }

    let scope_key = format!("detail-scope-{}", skill.id);
    let mut scope_val = app
        .skills_state
        .deploy_scope
        .get(&scope_key)
        .cloned()
        .unwrap_or_else(|| "global".into());
    let is_global = scope_val == "global";
    let is_deployed = if is_global {
        skill
            .deployments
            .get(ide_selected.as_str())
            .map(|roots| roots.iter().any(|r| r == "~"))
            .unwrap_or(false)
    } else {
        projects.iter().any(|(p, _, has)| *has && p == &scope_val)
    };

    let ide_snap = ide_selected.clone();
    let scope_snap = scope_val.clone();
    ui::detail_action_panel(
        ui,
        |ui| {
            let widths = skill_detail_action_widths(ui.available_width(), !is_local);
            let ide_label = ide::find(&ide_selected)
                .map(|i| i.display_name)
                .unwrap_or(ide_selected.as_str());
            egui::ComboBox::from_id_source(format!("d-ide-{}", skill.id))
                .selected_text(RichText::new(ide_label).size(12.0))
                .width(widths.ide)
                .show_ui(ui, |ui| {
                    for i in ide::IDES {
                        ui.selectable_value(&mut ide_selected, i.id.to_string(), i.display_name);
                    }
                });

            let scope_label = if scope_val == "global" {
                i18n::t("common.global").to_string()
            } else {
                projects
                    .iter()
                    .find(|(p, _, _)| p == &scope_val)
                    .map(|(_, n, _)| n.clone())
                    .unwrap_or_else(|| {
                        std::path::Path::new(scope_val.as_str())
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| scope_val.clone())
                    })
            };
            egui::ComboBox::from_id_source(format!("d-scope-{}", skill.id))
                .selected_text(RichText::new(&scope_label).size(12.0))
                .width(widths.scope)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut scope_val,
                        "global".to_string(),
                        i18n::t("common.global"),
                    );
                    for (path, name, has) in &projects {
                        let mark = if *has { "  \u{2713}" } else { "" };
                        ui.selectable_value(&mut scope_val, path.clone(), format!("{name}{mark}"));
                    }
                });

            if is_deployed {
                if ui::compact_danger_button(ui, i18n::t("common.undeploy"), widths.deploy)
                    .clicked()
                {
                    let id = skill.id.clone();
                    app.skills_state.deployed_projects_cache.remove(&id);
                    if is_global {
                        match tasks::undeploy_skill(&id, &ide_snap, None) {
                            Ok(_) => {
                                app.toast_info(format!("undeployed {} from {}", short, ide_snap));
                                app.reload_skills();
                            }
                            Err(e) => app.toast_error(format!("{e}")),
                        }
                    } else {
                        let path = std::path::PathBuf::from(&scope_snap);
                        match tasks::skill_undeploy_from_project(&id, &ide_snap, &path) {
                            Ok(_) => {
                                app.toast_info("undeployed");
                                app.reload_skills();
                            }
                            Err(e) => app.toast_error(format!("{e}")),
                        }
                    }
                }
            } else if ui::compact_primary_button(ui, i18n::t("common.deploy"), widths.deploy)
                .clicked()
            {
                let id = skill.id.clone();
                app.skills_state.deployed_projects_cache.remove(&id);
                if is_global {
                    match tasks::deploy_skill(&id, &ide_snap, None) {
                        Ok(p) => {
                            app.toast_info(format!("deployed -> {}", p.display()));
                            app.reload_skills();
                        }
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                } else {
                    let path = std::path::PathBuf::from(&scope_snap);
                    match tasks::skill_deploy_to_project(&id, &ide_snap, &path) {
                        Ok(p) => {
                            app.toast_info(format!("deployed -> {}", p.display()));
                            app.reload_skills();
                        }
                        Err(e) => app.toast_error(format!("{e}")),
                    }
                }
            }

            if !is_local
                && ui::compact_secondary_button(ui, i18n::t("common.update"), widths.update)
                    .clicked()
            {
                app.bus.update_skill(skill.id.clone());
            }
        },
        |_| {},
    );

    app.skills_state
        .deploy_ide
        .insert(skill.id.clone(), ide_selected);
    app.skills_state.deploy_scope.insert(scope_key, scope_val);
}

struct SkillDetailActionWidths {
    ide: f32,
    scope: f32,
    deploy: f32,
    update: f32,
}

fn skill_detail_action_widths(available_width: f32, has_update: bool) -> SkillDetailActionWidths {
    let gap = 6.0;
    let count = if has_update { 4.0 } else { 3.0 };
    let usable = (available_width - gap * (count - 1.0)).max(0.0);
    if has_update {
        let ide = usable * 0.34;
        let scope = usable * 0.25;
        let deploy = usable * 0.21;
        let update = (usable - ide - scope - deploy).max(0.0);
        SkillDetailActionWidths {
            ide,
            scope,
            deploy,
            update,
        }
    } else {
        let ide = usable * 0.42;
        let scope = usable * 0.34;
        let deploy = (usable - ide - scope).max(0.0);
        SkillDetailActionWidths {
            ide,
            scope,
            deploy,
            update: 0.0,
        }
    }
}

fn skill_deployment_records(skill: &Skill) -> Vec<(String, String, String, egui::Color32)> {
    let store = aiem_core::projects::ProjectStore::load().ok();
    let mut rows = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for (ide_id, roots) in &skill.deployments {
        let ide_label = ide::find(ide_id)
            .map(|i| i.display_name.to_string())
            .unwrap_or_else(|| ide_id.clone());
        for root in roots {
            let project = deployment_project_label(root, store.as_ref());
            if seen.insert((project.clone(), ide_label.clone())) {
                rows.push((
                    project,
                    ide_label.clone(),
                    i18n::t("deployment.deployed").to_string(),
                    theme::SUCCESS(),
                ));
            }
        }
    }

    if let Some(store) = store.as_ref() {
        for project in store
            .list()
            .filter(|p| p.skills.iter().any(|s| s == &skill.id))
        {
            let ides: Vec<String> = if project.ides.is_empty() {
                vec![i18n::t("common.none").to_string()]
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
                        i18n::t("deployment.deployed").to_string(),
                        theme::SUCCESS(),
                    ));
                }
            }
        }
    }

    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    rows
}

fn deployment_project_label(
    root: &str,
    store: Option<&aiem_core::projects::ProjectStore>,
) -> String {
    if root == "~" {
        return i18n::t("common.global").to_string();
    }
    if let Some(project) = store.and_then(|s| s.get(root)) {
        return project.name.clone();
    }
    std::path::Path::new(root)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string())
}

fn render_add_form(ui: &mut egui::Ui, app: &mut App) {
    ui::settings_group(ui, i18n::t("skills.add_title"), |ui| {
        let pal = theme::p();
        ui.label(
            RichText::new(i18n::t("skills.add_hint"))
                .size(12.0)
                .color(pal.text_sec),
        );
        ui.add_space(6.0);

        let field_w = (ui.available_width() - 100.0).max(200.0);
        egui::Grid::new("skill-add-grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(i18n::t("skills.source"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.skills_state.add_source)
                        .desired_width(field_w)
                        .hint_text("owner/repo"),
                );
                ui.end_row();
                ui.label(
                    RichText::new(i18n::t("skills.subdir"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.skills_state.add_subdir)
                        .desired_width(field_w)
                        .hint_text("optional"),
                );
                ui.end_row();
                ui.label(
                    RichText::new(i18n::t("skills.ref"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.skills_state.add_ref)
                        .desired_width(field_w)
                        .hint_text("branch / tag"),
                );
                ui.end_row();
                ui.label(
                    RichText::new(i18n::t("skills.name"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.skills_state.add_name)
                        .desired_width(field_w)
                        .hint_text("optional"),
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui::primary_button(ui, i18n::t("skills.download")).clicked() {
                let mut src = app.skills_state.add_source.trim().to_string();
                if src.is_empty() {
                    app.toast_error(i18n::t("skills.source_required"));
                    return;
                }
                if !app.skills_state.add_subdir.trim().is_empty() && !src.contains("//") {
                    src.push_str(&format!("//{}", app.skills_state.add_subdir.trim()));
                }
                if !app.skills_state.add_ref.trim().is_empty() && !src.contains('@') {
                    src.push_str(&format!("@{}", app.skills_state.add_ref.trim()));
                }
                let name = {
                    let n = app.skills_state.add_name.trim();
                    if n.is_empty() {
                        None
                    } else {
                        Some(n.to_string())
                    }
                };
                app.bus.add_skill_from_github(src, name);
                app.skills_state.add_open = false;
                app.skills_state.add_source.clear();
                app.skills_state.add_ref.clear();
                app.skills_state.add_subdir.clear();
                app.skills_state.add_name.clear();
            }
            if ui::secondary_button(ui, i18n::t("common.cancel")).clicked() {
                app.skills_state.add_open = false;
            }
        });
    });
}

fn render_create_form(ui: &mut egui::Ui, app: &mut App) {
    ui::settings_group(ui, i18n::t("skills.create_title"), |ui| {
        let pal = theme::p();
        ui.label(
            RichText::new(i18n::t("skills.create_hint"))
                .size(12.0)
                .color(pal.text_sec),
        );
        ui.add_space(6.0);

        let field_w = (ui.available_width() - 100.0).max(200.0);
        egui::Grid::new("skill-create-grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(i18n::t("skills.name_req"))
                        .size(13.0)
                        .color(pal.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.skills_state.create_name)
                        .desired_width(field_w)
                        .hint_text("my-awesome-skill"),
                );
                ui.end_row();
                ui.label(RichText::new("SKILL.md *").size(13.0).color(pal.text));
                ui.add(
                    egui::TextEdit::multiline(&mut app.skills_state.create_content)
                        .desired_width(field_w)
                        .desired_rows(8)
                        .hint_text("# My Skill\n\nDescribe what this skill does..."),
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui::primary_button(ui, i18n::t("skills.create_btn")).clicked() {
                let name = app.skills_state.create_name.trim().to_string();
                let content = app.skills_state.create_content.clone();
                if name.is_empty() || content.trim().is_empty() {
                    app.toast_error(i18n::t("skills.name_content_required"));
                    return;
                }
                match aiem_core::skills::create_local_skill(&name, &content) {
                    Ok(s) => {
                        app.toast_info(format!("created skill: {}", s.name));
                        app.reload_skills();
                        app.skills_state.create_open = false;
                        app.skills_state.create_name.clear();
                        app.skills_state.create_content.clear();
                    }
                    Err(e) => app.toast_error(format!("create: {e}")),
                }
            }
            if ui::secondary_button(ui, i18n::t("common.cancel")).clicked() {
                app.skills_state.create_open = false;
            }
        });
    });
}

fn short_ver(v: &str) -> String {
    if v.len() > 10 {
        v[..10].to_string()
    } else {
        v.to_string()
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
