use aiem_core::ide;
use eframe::egui::{self, RichText};

use crate::app::{card, page_header, primary_button, App};
use crate::i18n;
use crate::tasks;
use crate::theme;

/// Extract a short display name from a skill id.
/// Examples:
///   "owner__repo__banner-design" -> "banner-design"
///   "owner__repo__.claude_skills_banner-design" -> "banner-design"
///   "wanshuiyin__Auto-.._skills_ablation-planner" -> "ablation-planner"
///   "owner/repo/banner-design" -> "banner-design"
pub fn short_id(id: &str) -> &str {
    // First strip any "owner__repo__" prefix if present.
    let tail = if let Some(pos) = id.rfind("__") {
        &id[pos + 2..]
    } else {
        id
    };
    // The tail may still be a flattened path like ".claude_skills_banner-design".
    // Take the last path-like segment.
    let leaf = tail
        .rsplit(|c: char| c == '/' || c == '\\' || c == '_')
        .find(|s| !s.is_empty())
        .unwrap_or(tail);
    leaf
}

#[derive(Default)]
pub struct State {
    pub add_source: String,
    pub add_ref: String,
    pub add_subdir: String,
    pub add_name: String,
    pub add_open: bool,
    pub filter: String,
    /// skill id -> chosen IDE for quick-deploy dropdown
    pub deploy_ide: std::collections::HashMap<String, String>,
    /// group -> deploy scope: "global" or a project path
    pub deploy_scope: std::collections::HashMap<String, String>,
    /// skill id for which "Link GitHub" form is open
    pub link_github_id: Option<String>,
    /// text input for the GitHub link
    pub link_github_input: String,
    /// group key pending batch-delete confirmation
    pub confirm_delete_group: Option<String>,
    /// skill id -> cached list of project names that reference this skill (chips row)
    pub deployed_projects_cache: std::collections::HashMap<String, Vec<String>>,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    page_header(
        ui,
        i18n::t("skills.title"),
        i18n::t("skills.subtitle"),
        |ui| {
            if ui.button(RichText::new(i18n::t("skills.clear_global")).color(theme::DANGER())).clicked() {
                match tasks::clear_all_global_skills() {
                    Ok(n) => {
                        app.toast_info(format!("cleared {n} global deployment(s)"));
                        app.reload_skills();
                    }
                    Err(e) => app.toast_error(format!("{e}")),
                }
            }
            if primary_button(ui, i18n::t("skills.add")).clicked() {
                app.skills_state.add_open = !app.skills_state.add_open;
            }
        },
    );

    if app.skills_state.add_open {
        render_add_form(ui, app);
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new(i18n::t("skills.filter")).color(theme::MUTED()));
        ui.add(egui::TextEdit::singleline(&mut app.skills_state.filter)
            .desired_width((ui.available_width() - 20.0).min(300.0).max(120.0))
            .hint_text("name / id"));
    });
    ui.add_space(8.0);

    let filter = app.skills_state.filter.to_ascii_lowercase();
    let items: Vec<_> = app.skills.list().cloned().collect();
    let total = items.len();

    // Group skills by GitHub owner/repo; local skills go under "(local)"
    let mut groups: std::collections::BTreeMap<String, Vec<aiem_core::skills::model::Skill>> =
        std::collections::BTreeMap::new();
    for skill in &items {
        if !filter.is_empty()
            && !skill.id.to_lowercase().contains(&filter)
            && !skill.name.to_lowercase().contains(&filter)
        {
            continue;
        }
        let group_key = match &skill.source {
            aiem_core::skills::model::SkillSource::GitHub { owner, repo, .. } => {
                format!("{}/{}", owner, repo)
            }
            _ => "(local)".to_string(),
        };
        groups.entry(group_key).or_default().push(skill.clone());
    }

    let shown: usize = groups.values().map(|v| v.len()).sum();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if total == 0 {
            empty_state(ui, i18n::t("skills.empty"), i18n::t("skills.empty_sub"));
        } else if shown == 0 {
            empty_state(ui, i18n::t("skills.no_match"), i18n::t("skills.no_match_sub"));
        } else {
            for (group_name, skills) in &groups {
                // Group header - show only repo name (last path segment)
                let display_name = group_name
                    .rsplit('/')
                    .next()
                    .unwrap_or(group_name.as_str());
                let group_label = if skills.len() > 1 {
                    format!("\u{1F4C1} {}  ({})", display_name, skills.len())
                } else {
                    format!("\u{1F4C1} {}", display_name)
                };
                let id = ui.make_persistent_id(format!("grp-{}", group_name));
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(), id, true,
                )
                .show_header(ui, |ui| {
                    ui.label(RichText::new(&group_label).strong().color(theme::TEXT()));
                })
                .body(|ui| {
                    // Batch actions bar for multi-skill groups
                    if skills.len() > 1 {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            // IDE picker for group
                            let group_ide_key = format!("grp-ide-{}", group_name);
                            let chosen = app.skills_state.deploy_ide
                                .entry(group_ide_key.clone())
                                .or_insert_with(|| "claude-code".to_string());
                            egui::ComboBox::from_id_source(format!("gide-{}", group_name))
                                .selected_text(chosen.as_str())
                                .width(130.0)
                                .show_ui(ui, |ui| {
                                    for i in ide::IDES {
                                        ui.selectable_value(chosen, i.id.to_string(), i.display_name);
                                    }
                                });
                            let ide_id = app.skills_state.deploy_ide
                                .get(&group_ide_key).cloned().unwrap_or_else(|| "claude-code".into());

                            // Scope picker: Global or Project
                            let scope_key = format!("grp-scope-{}", group_name);
                            let scope = app.skills_state.deploy_scope
                                .entry(scope_key.clone())
                                .or_insert_with(|| "global".to_string());
                            let scope_label = if scope == "global" {
                                i18n::t("skills.scope_global").to_string()
                            } else {
                                // Show last path component
                                std::path::Path::new(scope.as_str())
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| scope.clone())
                            };
                            egui::ComboBox::from_id_source(format!("gscope-{}", group_name))
                                .selected_text(&scope_label)
                                .width(160.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(scope, "global".to_string(), i18n::t("skills.scope_global"));
                                    // List registered projects
                                    if let Ok(store) = aiem_core::projects::ProjectStore::load() {
                                        for proj in store.list() {
                                            let p = proj.path.clone();
                                            let label = std::path::Path::new(&p)
                                                .file_name()
                                                .map(|n| n.to_string_lossy().to_string())
                                                .unwrap_or_else(|| p.clone());
                                            ui.selectable_value(scope, p, label);
                                        }
                                    }
                                });
                            let project_path: Option<std::path::PathBuf> = {
                                let s = app.skills_state.deploy_scope
                                    .get(&scope_key).cloned().unwrap_or_else(|| "global".into());
                                if s == "global" { None } else { Some(std::path::PathBuf::from(s)) }
                            };

                            // Deploy All
                            {
                                let pal = theme::p();
                                let btn = egui::Button::new(
                                    RichText::new(i18n::t("skills.deploy_all")).small().color(pal.accent_fg),
                                ).fill(pal.accent).rounding(egui::Rounding::same(6.0))
                                    .min_size(egui::vec2(0.0, 26.0));
                                if ui.add(btn).clicked() {
                                    let mut ok = 0;
                                    for s in skills.iter() {
                                        let res = match project_path.as_deref() {
                                            Some(p) => tasks::skill_deploy_to_project(&s.id, &ide_id, p).map(|_| ()),
                                            None => tasks::deploy_skill(&s.id, &ide_id, None).map(|_| ()),
                                        };
                                        match res {
                                            Ok(_) => ok += 1,
                                            Err(e) => app.toast_error(format!("{}: {e}", s.name)),
                                        }
                                    }
                                    if ok > 0 {
                                        app.toast_info(format!("deployed {ok} skills to {ide_id}"));
                                        app.skills_state.deployed_projects_cache.clear();
                                        app.reload_skills();
                                    }
                                }
                            }
                            // Undeploy All
                            {
                                let btn = egui::Button::new(
                                    RichText::new(i18n::t("skills.undeploy_all")).small().color(theme::DANGER()),
                                ).rounding(egui::Rounding::same(6.0))
                                    .min_size(egui::vec2(0.0, 26.0));
                                if ui.add(btn).clicked() {
                                    let mut ok = 0;
                                    for s in skills.iter() {
                                        if s.deployments.contains_key(ide_id.as_str()) {
                                            let res = match project_path.as_deref() {
                                                Some(p) => tasks::skill_undeploy_from_project(&s.id, &ide_id, p),
                                                None => tasks::undeploy_skill(&s.id, &ide_id, None),
                                            };
                                            if res.is_ok() { ok += 1; }
                                        }
                                    }
                                    if ok > 0 {
                                        app.toast_info(format!("undeployed {ok} skills from {ide_id}"));
                                        app.skills_state.deployed_projects_cache.clear();
                                        app.reload_skills();
                                    }
                                }
                            }
                            // Update All
                            {
                                let btn = egui::Button::new(
                                    RichText::new(i18n::t("skills.update")).small(),
                                ).rounding(egui::Rounding::same(6.0))
                                    .min_size(egui::vec2(0.0, 26.0));
                                if ui.add(btn).clicked() {
                                    // Use sync_group so newly added upstream skills are also installed
                                    if let Some(first) = skills.iter().find(|s| {
                                        !matches!(&s.source, aiem_core::skills::model::SkillSource::Local { .. })
                                    }) {
                                        if let aiem_core::skills::model::SkillSource::GitHub { owner, repo, .. } = &first.source {
                                            let gh_skills: Vec<_> = skills.iter()
                                                .filter(|s| !matches!(&s.source, aiem_core::skills::model::SkillSource::Local { .. }))
                                                .cloned()
                                                .collect();
                                            app.bus.sync_group(owner.clone(), repo.clone(), gh_skills);
                                        }
                                    }
                                }
                            }
                            // Remove All (with inline confirmation)
                            {
                                let pending = app.skills_state.confirm_delete_group.as_deref()
                                    == Some(group_name.as_str());
                                let btn = egui::Button::new(
                                    RichText::new(i18n::t("skills.remove_all")).small().color(theme::DANGER()),
                                ).rounding(egui::Rounding::same(6.0))
                                    .min_size(egui::vec2(0.0, 26.0));
                                if ui.add(btn).clicked() {
                                    if pending {
                                        app.skills_state.confirm_delete_group = None;
                                    } else {
                                        app.skills_state.confirm_delete_group = Some(group_name.clone());
                                    }
                                }
                            }
                        });
                        // Inline confirmation row
                        if app.skills_state.confirm_delete_group.as_deref() == Some(group_name.as_str()) {
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let msg = format!(
                                    "{} {} {}",
                                    i18n::t("skills.remove_all_confirm_pre"),
                                    skills.len(),
                                    i18n::t("skills.remove_all_confirm_post"),
                                );
                                ui.label(RichText::new(msg).color(theme::DANGER()));
                                let confirm_btn = egui::Button::new(
                                    RichText::new(i18n::t("skills.remove_all_ok")).color(theme::DANGER()),
                                ).rounding(egui::Rounding::same(6.0))
                                    .min_size(egui::vec2(0.0, 26.0));
                                if ui.add(confirm_btn).clicked() {
                                    let ids: Vec<String> = skills.iter().map(|s| s.id.clone()).collect();
                                    let mut ok = 0usize;
                                    for id in &ids {
                                        match tasks::remove_skill(id) {
                                            Ok(_) => ok += 1,
                                            Err(e) => app.toast_error(format!("{}: {e}", short_id(id))),
                                        }
                                    }
                                    if ok > 0 {
                                        app.toast_info(format!(
                                            "{} {}",
                                            ok,
                                            i18n::t("skills.remove_all_done")
                                        ));
                                        app.reload_skills();
                                    }
                                    app.skills_state.confirm_delete_group = None;
                                }
                                if ui.button(i18n::t("skills.remove_all_cancel")).clicked() {
                                    app.skills_state.confirm_delete_group = None;
                                }
                            });
                        }
                        ui.add_space(4.0);
                    }
                    for skill in skills {
                        render_skill_card(ui, app, skill);
                    }
                });
                ui.add_space(4.0);
            }
        }
    });
}

fn render_add_form(ui: &mut egui::Ui, app: &mut App) {
    card(ui, |ui| {
        ui.label(RichText::new("Add skill from GitHub").strong().color(theme::TEXT()));
        ui.add_space(6.0);
        ui.label(RichText::new("Paste a GitHub URL or shorthand: owner/repo · owner/repo//subdir · owner/repo@v1.2")
            .color(theme::MUTED()).small());
        ui.add_space(10.0);

        let field_w = (ui.available_width() - 100.0).max(200.0);
        egui::Grid::new("skill-add-grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Source").color(theme::TEXT()));
                ui.add(egui::TextEdit::singleline(&mut app.skills_state.add_source)
                    .desired_width(field_w)
                    .hint_text("https://github.com/owner/repo or owner/repo"));
                ui.end_row();

                ui.label(RichText::new("Subdir (opt)").color(theme::TEXT()));
                ui.add(egui::TextEdit::singleline(&mut app.skills_state.add_subdir)
                    .desired_width(field_w)
                    .hint_text("path/inside/repo"));
                ui.end_row();

                ui.label(RichText::new("Ref (opt)").color(theme::TEXT()));
                ui.add(egui::TextEdit::singleline(&mut app.skills_state.add_ref)
                    .desired_width(field_w)
                    .hint_text("branch / tag / commit"));
                ui.end_row();

                ui.label(RichText::new("Name (opt)").color(theme::TEXT()));
                ui.add(egui::TextEdit::singleline(&mut app.skills_state.add_name)
                    .desired_width(field_w)
                    .hint_text("display name"));
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if primary_button(ui, "Download & install").clicked() {
                let mut src = app.skills_state.add_source.trim().to_string();
                if src.is_empty() {
                    app.toast_error("source is required");
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
                    if n.is_empty() { None } else { Some(n.to_string()) }
                };
                app.bus.add_skill_from_github(src, name);
                app.skills_state.add_open = false;
                app.skills_state.add_source.clear();
                app.skills_state.add_ref.clear();
                app.skills_state.add_subdir.clear();
                app.skills_state.add_name.clear();
            }
            if ui.button("Cancel").clicked() {
                app.skills_state.add_open = false;
            }
        });
    });
    ui.add_space(14.0);
}

fn render_skill_card(ui: &mut egui::Ui, app: &mut App, skill: &aiem_core::skills::model::Skill) {
    let is_local = matches!(&skill.source, aiem_core::skills::model::SkillSource::Local { .. });
    let skill_id = skill.id.clone();

    card(ui, |ui| {
        // Info row: title + id + version + deployments | action buttons (bottom-aligned)
        ui.horizontal(|ui| {
            // Left info section (flexible width)
            ui.vertical(|ui| {
                // Display a cleaner name: last segment of the id (e.g. "banner-design")
                let short_name = short_id(&skill.id);
                ui.label(RichText::new(short_name).strong().size(16.0).color(theme::TEXT()));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new(short_ver(&skill.version)).small().color(theme::ACCENT()));
                    if is_local {
                        theme::tag(ui, "local", theme::MUTED());
                    }
                });
                if let Some(d) = &skill.description {
                    ui.add_space(2.0);
                    let first = d.lines().next().unwrap_or("");
                    ui.label(RichText::new(first).color(theme::MUTED()).small());
                }
                if !skill.deployments.is_empty() {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for ide in skill.deployments.keys() {
                            theme::tag(ui, ide, theme::SUCCESS());
                        }
                    });
                }
                // Project-membership chips (left-aligned, no label).
                let sk_id_chips = skill.id.clone();
                let proj_names = app.skills_state.deployed_projects_cache
                    .entry(sk_id_chips.clone())
                    .or_insert_with(|| tasks::skill_projects_with(&sk_id_chips).unwrap_or_default())
                    .clone();
                if !proj_names.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for n in &proj_names { theme::tag(ui, n, theme::MUTED()); }
                    });
                }
            });

            // Right action buttons (bottom-aligned with left content)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                let btn = egui::Button::new(RichText::new("Remove").color(theme::DANGER()))
                    .rounding(egui::Rounding::same(6.0))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(btn).clicked() {
                    match tasks::remove_skill(&skill_id) {
                        Ok(_) => { app.toast_info(format!("removed {}", skill_id)); app.reload_skills(); }
                        Err(e) => app.toast_error(format!("remove failed: {e}")),
                    }
                }

                if is_local {
                    let btn = egui::Button::new(RichText::new("Link GitHub"))
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(0.0, 26.0));
                    if ui.add(btn).clicked() {
                        app.skills_state.link_github_id = Some(skill_id.clone());
                        app.skills_state.link_github_input.clear();
                    }
                } else {
                    let btn = egui::Button::new(RichText::new("Update"))
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(0.0, 26.0));
                    if ui.add(btn).clicked() {
                        app.bus.update_skill(skill_id.clone());
                    }
                }

                // Deploy/Undeploy + ComboBox
                // ── Deploy/Undeploy toggle + IDE picker + Project picker ──
                // Visual order (left → right): [Project ▾] [IDE ▾] [Deploy/Undeploy]
                // (code order is reverse because this is a RTL layout).
                let ide_selected = app.skills_state.deploy_ide
                    .entry(skill.id.clone())
                    .or_insert_with(|| "claude-code".to_string())
                    .clone();

                // Project scope: "global" | <project-path>
                let scope_key = format!("card-scope-{}", skill.id);
                let scope_val = app.skills_state.deploy_scope
                    .get(&scope_key).cloned().unwrap_or_else(|| "global".into());
                let is_global = scope_val == "global";

                // Load projects once per card draw (list + "does this project already have this skill?").
                let projects: Vec<(String, String, bool)> = aiem_core::projects::ProjectStore::load()
                    .map(|store| store.list()
                        .map(|p| (p.path.clone(), p.name.clone(),
                                  p.skills.iter().any(|s| s == &skill.id)))
                        .collect())
                    .unwrap_or_default();

                let is_deployed_here = if is_global {
                    // Global: per-IDE deployment list contains "~"
                    skill.deployments.get(ide_selected.as_str())
                        .map(|roots| roots.iter().any(|r| r == "~"))
                        .unwrap_or(false)
                } else {
                    // Project: project.skills contains this id
                    projects.iter().any(|(p, _, has)| *has && p == &scope_val)
                };
                let (label, danger) = if is_deployed_here { ("Undeploy", true) } else { ("Deploy", false) };
                let resp = if danger {
                    let btn = egui::Button::new(RichText::new(label).color(theme::DANGER()))
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(0.0, 26.0));
                    ui.add(btn)
                } else {
                    let pal = theme::p();
                    let btn = egui::Button::new(RichText::new(label).color(pal.accent_fg))
                        .fill(pal.accent)
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(0.0, 26.0));
                    ui.add(btn)
                };
                if resp.clicked() {
                    let id = skill.id.clone();
                    // Invalidate chips cache — membership may change.
                    app.skills_state.deployed_projects_cache.remove(&id);
                    if is_global {
                        if is_deployed_here {
                            match tasks::undeploy_skill(&id, &ide_selected, None) {
                                Ok(_) => { app.toast_info(format!("undeployed {id} from {ide_selected}")); app.reload_skills(); }
                                Err(e) => app.toast_error(format!("{e}")),
                            }
                        } else {
                            match tasks::deploy_skill(&id, &ide_selected, None) {
                                Ok(p) => { app.toast_info(format!("deployed -> {}", p.display())); app.reload_skills(); }
                                Err(e) => app.toast_error(format!("{e}")),
                            }
                        }
                    } else {
                        let project = std::path::PathBuf::from(&scope_val);
                        if is_deployed_here {
                            match tasks::skill_undeploy_from_project(&id, &ide_selected, &project) {
                                Ok(_) => { app.toast_info(format!("undeployed {id} from project")); app.reload_skills(); }
                                Err(e) => app.toast_error(format!("{e}")),
                            }
                        } else {
                            match tasks::skill_deploy_to_project(&id, &ide_selected, &project) {
                                Ok(p) => { app.toast_info(format!("deployed -> {}", p.display())); app.reload_skills(); }
                                Err(e) => app.toast_error(format!("{e}")),
                            }
                        }
                    }
                }

                // IDE ComboBox (appears to the LEFT of the button in RTL).
                let chosen = app.skills_state.deploy_ide
                    .entry(skill.id.clone())
                    .or_insert_with(|| "claude-code".to_string());
                egui::ComboBox::from_id_source(format!("ide-{}", skill.id))
                    .selected_text(chosen.as_str())
                    .width(130.0)
                    .show_ui(ui, |ui| {
                        for i in ide::IDES {
                            ui.selectable_value(chosen, i.id.to_string(), i.display_name);
                        }
                    });

                // Project scope ComboBox (leftmost in RTL).
                let mut scope_val = scope_val;
                let scope_label = if is_global {
                    "Global".to_string()
                } else {
                    projects.iter().find(|(p, _, _)| p == &scope_val)
                        .map(|(_, n, _)| n.clone())
                        .unwrap_or_else(|| std::path::Path::new(scope_val.as_str())
                            .file_name().map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| scope_val.clone()))
                };
                egui::ComboBox::from_id_source(format!("proj-{}", skill.id))
                    .selected_text(&scope_label)
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut scope_val, "global".to_string(), "Global");
                        if projects.is_empty() {
                            ui.label(RichText::new("(no projects registered)").small().color(theme::MUTED()));
                        } else {
                            for (path, name, has) in &projects {
                                let label = if *has { format!("{name}  \u{2713}") } else { name.clone() };
                                ui.selectable_value(&mut scope_val, path.clone(), label);
                            }
                        }
                    });
                app.skills_state.deploy_scope.insert(scope_key, scope_val);
            });
        });

        // "Link GitHub" inline form
        if app.skills_state.link_github_id.as_deref() == Some(&skill_id) {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("GitHub:").color(theme::TEXT()));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut app.skills_state.link_github_input)
                        .desired_width((ui.available_width() - 200.0).max(180.0))
                        .hint_text("owner/repo or https://github.com/owner/repo"),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if primary_button(ui, "Save").clicked() || enter {
                    let input = app.skills_state.link_github_input.trim().to_string();
                    if let Some(new_source) = aiem_core::skills::model::SkillSource::parse_github(&input) {
                        match aiem_core::skills::SkillRegistry::load() {
                            Ok(mut reg) => {
                                if let Some(mut sk) = reg.get(&skill_id).cloned() {
                                    sk.source = new_source;
                                    reg.upsert(sk);
                                    if let Err(e) = reg.save() {
                                        app.toast_error(format!("save: {e}"));
                                    } else {
                                        app.toast_info(format!("linked {} to GitHub", skill_id));
                                        app.reload_skills();
                                    }
                                }
                            }
                            Err(e) => app.toast_error(format!("{e}")),
                        }
                        app.skills_state.link_github_id = None;
                    } else {
                        app.toast_error("Invalid GitHub format. Use owner/repo");
                    }
                }
                if ui.button("Cancel").clicked() {
                    app.skills_state.link_github_id = None;
                }
            });
        }

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

fn short_ver(v: &str) -> String { if v.len() > 10 { v[..10].to_string() } else { v.to_string() } }
