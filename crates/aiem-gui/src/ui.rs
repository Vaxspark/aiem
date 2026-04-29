use eframe::egui::{self, Color32, FontId, RichText, Rounding, Stroke};

use crate::i18n;
use crate::theme;

/// Centered max-width content container.
#[allow(dead_code)]
pub fn page_content(ui: &mut egui::Ui, max_width: f32, content: impl FnOnce(&mut egui::Ui)) {
    let avail = ui.available_width();
    if avail > max_width {
        let pad = (avail - max_width) / 2.0;
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.set_max_width(max_width);
                content(ui);
            });
        });
    } else {
        content(ui);
    }
}

pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let pal = theme::p();
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .size(13.0)
                .color(pal.accent_fg)
                .strong(),
        )
        .fill(pal.accent)
        .rounding(Rounding::same(7.0))
        .min_size(egui::vec2(0.0, 28.0)),
    )
}

pub fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let pal = theme::p();
    ui.add(
        egui::Button::new(RichText::new(label).size(13.0).color(pal.text))
            .fill(pal.surface_hi)
            .stroke(Stroke::new(1.0, pal.stroke))
            .rounding(Rounding::same(7.0))
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

pub fn danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let pal = theme::p();
    ui.add(
        egui::Button::new(RichText::new(label).size(13.0).color(pal.danger))
            .fill(pal.surface)
            .stroke(Stroke::new(1.0, pal.danger.gamma_multiply(0.85)))
            .rounding(Rounding::same(7.0))
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

#[allow(dead_code)]
pub fn fixed_primary_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    let pal = theme::p();
    ui.add_sized(
        egui::vec2(width, 30.0),
        egui::Button::new(
            RichText::new(label)
                .size(13.0)
                .strong()
                .color(pal.accent_fg),
        )
        .fill(pal.accent)
        .rounding(Rounding::same(7.0)),
    )
}

#[allow(dead_code)]
pub fn fixed_secondary_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    let pal = theme::p();
    ui.add_sized(
        egui::vec2(width, 30.0),
        egui::Button::new(RichText::new(label).size(13.0).color(pal.text))
            .fill(pal.surface_hi)
            .stroke(Stroke::new(1.0, pal.stroke))
            .rounding(Rounding::same(7.0)),
    )
}

pub fn fixed_danger_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    let pal = theme::p();
    ui.add_sized(
        egui::vec2(width, 30.0),
        egui::Button::new(RichText::new(label).size(13.0).color(pal.danger))
            .fill(pal.surface)
            .stroke(Stroke::new(1.0, pal.danger.gamma_multiply(0.85)))
            .rounding(Rounding::same(7.0)),
    )
}

pub fn compact_primary_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    let pal = theme::p();
    ui.add_sized(
        egui::vec2(width, COMPACT_CONTROL_H),
        egui::Button::new(
            RichText::new(label)
                .size(12.0)
                .strong()
                .color(pal.accent_fg),
        )
        .fill(pal.accent)
        .rounding(Rounding::same(6.0)),
    )
}

pub fn compact_secondary_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    compact_secondary_button_enabled(ui, label, width, true)
}

pub fn compact_secondary_button_enabled(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    enabled: bool,
) -> egui::Response {
    let pal = theme::p();
    let text_color = if enabled { pal.text } else { pal.text_sec };
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).size(12.0).color(text_color))
            .fill(pal.surface_hi)
            .stroke(Stroke::new(1.0, pal.stroke))
            .rounding(Rounding::same(6.0))
            .min_size(egui::vec2(width, COMPACT_CONTROL_H)),
    )
}

pub fn compact_danger_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    let pal = theme::p();
    ui.add_sized(
        egui::vec2(width, COMPACT_CONTROL_H),
        egui::Button::new(RichText::new(label).size(12.0).color(pal.danger))
            .fill(pal.surface)
            .stroke(Stroke::new(1.0, pal.danger.gamma_multiply(0.85)))
            .rounding(Rounding::same(6.0)),
    )
}

pub fn small_action(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let pal = theme::p();
    ui.add(
        egui::Button::new(RichText::new(label).size(12.0).color(pal.text))
            .fill(Color32::TRANSPARENT)
            .rounding(Rounding::same(6.0))
            .min_size(egui::vec2(0.0, 24.0)),
    )
}

/// Responsive toolbar: single row if wide enough, two rows if narrow (<620).
pub fn page_toolbar(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    right: impl FnOnce(&mut egui::Ui),
) {
    let pal = theme::p();
    let wide = ui.available_width() >= 620.0;

    if wide {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(title).size(24.0).strong().color(pal.text));
                if !subtitle.is_empty() {
                    ui.label(RichText::new(subtitle).size(13.0).color(pal.text_sec));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
        });
    } else {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(22.0).strong().color(pal.text));
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).size(12.0).color(pal.text_sec));
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
            });
        });
    }
    ui.add_space(10.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.min.y;
    ui.painter().line_segment(
        [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
        Stroke::new(1.0, pal.stroke),
    );
    ui.add_space(14.0);
}

/// Responsive resource row with click selection.
/// The entire card (frame + content) avoids the scrollbar gutter area.
pub fn resource_row(
    ui: &mut egui::Ui,
    id_source: &str,
    selected: bool,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let pal = theme::p();
    let id = ui.make_persistent_id(id_source);
    let bg = if selected { pal.selected } else { pal.surface };
    let stroke = if selected {
        Stroke::new(1.0, pal.accent.gamma_multiply(0.28))
    } else {
        Stroke::new(1.0, pal.stroke.gamma_multiply(0.72))
    };

    let card_w = (ui.available_width() - LIST_GUTTER).max(0.0);
    let resp = ui.allocate_ui_with_layout(
        egui::vec2(card_w, 0.0),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            egui::Frame::none()
                .fill(bg)
                .stroke(stroke)
                .rounding(Rounding::same(10.0))
                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                .show(ui, |ui| {
                    ui.set_min_width((card_w - 28.0).max(0.0));
                    content(ui);
                });
        },
    );

    let outer_resp = ui.interact(resp.response.rect, id, egui::Sense::click());
    ui.add_space(5.0);
    outer_resp
}

pub fn settings_group(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    let pal = theme::p();
    if !title.is_empty() {
        ui.label(
            RichText::new(title.to_uppercase())
                .size(11.0)
                .color(pal.text_sec),
        );
        ui.add_space(4.0);
    }
    egui::Frame::none()
        .fill(pal.surface)
        .stroke(Stroke::new(1.0, pal.stroke))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 28.0).max(0.0));
            content(ui);
        });
    ui.add_space(12.0);
}

pub fn settings_row(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    let pal = theme::p();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(label).color(pal.text).size(14.0));
            if !description.is_empty() {
                ui.label(RichText::new(description).size(12.0).color(pal.text_sec));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let pal = theme::p();
    let bg = Color32::from_rgba_premultiplied(
        ((pal.tag_bg.r() as u16 * 220 + color.r() as u16 * 35) / 255) as u8,
        ((pal.tag_bg.g() as u16 * 220 + color.g() as u16 * 35) / 255) as u8,
        ((pal.tag_bg.b() as u16 * 220 + color.b() as u16 * 35) / 255) as u8,
        255,
    );
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(11.0), color);
    let padding = egui::vec2(8.0, 3.0);
    let size = galley.size() + padding * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
    ui.painter().galley(rect.min + padding, galley, color);
    resp
}

#[allow(dead_code)]
pub fn status_badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.label(RichText::new(text).size(12.0).color(color));
    });
}

#[allow(dead_code)]
pub fn danger_zone(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    let pal = theme::p();
    ui.add_space(8.0);
    egui::Frame::none()
        .fill(Color32::from_rgba_premultiplied(
            pal.danger.r(),
            pal.danger.g(),
            pal.danger.b(),
            8,
        ))
        .stroke(Stroke::new(1.0, pal.danger.gamma_multiply(0.3)))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 28.0).max(0.0));
            content(ui);
        });
}

pub const DETAIL_GUTTER: f32 = 22.0;
pub const LIST_GUTTER: f32 = 18.0;
pub const SCROLLBAR_WIDTH: f32 = 3.0;
pub const COMPACT_CONTROL_H: f32 = 28.0;
pub const DETAIL_EMBEDDED_CARD_MAX_W: f32 = 560.0;
pub const DETAIL_DIVIDER_LEFT_INSET: f32 = 0.0;
pub const DETAIL_DIVIDER_RIGHT_INSET: f32 = 0.0;
pub const DANGER_BUTTON_RIGHT_INSET: f32 = 8.5;
const DETAIL_INSET: f32 = 14.0;

/// Scroll area for detail pages with a fixed right gutter to avoid scrollbar overlap.
pub fn detail_scroll_body(ui: &mut egui::Ui, id: &str, content: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical()
        .id_source(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let content_w = (ui.available_width() - DETAIL_GUTTER).max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(content_w, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_min_width(content_w);
                    ui.set_max_width(content_w);
                    content(ui);
                    ui.add_space(16.0);
                },
            );
        });
}

/// Lightweight horizontal divider aligned to content width.
pub fn detail_divider(ui: &mut egui::Ui) {
    let pal = theme::p();
    ui.add_space(6.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.min.y;
    let min_x = rect.min.x + DETAIL_DIVIDER_LEFT_INSET;
    let max_x = (rect.max.x - DETAIL_DIVIDER_RIGHT_INSET).max(min_x);
    ui.painter().line_segment(
        [egui::pos2(min_x, y), egui::pos2(max_x, y)],
        Stroke::new(1.0, pal.stroke),
    );
    ui.add_space(6.0);
}

const KV_LABEL_W: f32 = 32.0;
const KV_VALUE_GAP: f32 = 4.0;

/// Shared centered width for inset detail cards such as deployment tables and previews.
pub fn detail_embedded_card_metrics(ui: &egui::Ui) -> (f32, f32) {
    let available = ui.available_width();
    let width = (available - DETAIL_INSET * 2.0).clamp(120.0, DETAIL_EMBEDDED_CARD_MAX_W);
    let side_gap = ((available - width) / 2.0).max(0.0);
    (width, side_gap)
}

/// Key-value row with a fixed-width label and flexible value area.
pub fn detail_kv_row(ui: &mut egui::Ui, label: &str, value: impl FnOnce(&mut egui::Ui)) {
    let pal = theme::p();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = KV_VALUE_GAP;
        ui.allocate_ui_with_layout(
            egui::vec2(KV_LABEL_W, 20.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_min_width(KV_LABEL_W);
                ui.set_max_width(KV_LABEL_W);
                ui.label(RichText::new(label).size(13.0).color(pal.text_sec));
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width().max(0.0), 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_max_width(ui.available_width().max(0.0));
                value(ui);
            },
        );
    });
    ui.add_space(5.0);
}

pub fn deployment_records_table(
    ui: &mut egui::Ui,
    _id_source: &str,
    rows: &[(String, String, String, Color32)],
) {
    let pal = theme::p();
    if rows.is_empty() {
        ui.label(
            RichText::new(i18n::t("deployment.no_records"))
                .size(12.0)
                .color(pal.text_sec),
        );
        return;
    }

    let (table_w, side_gap) = detail_embedded_card_metrics(ui);
    let header_h = 30.0;
    let row_h = 34.0;
    let total_h = header_h + row_h * rows.len() as f32;
    let col_project = table_w * 0.36;
    let col_ide = table_w * 0.34;

    ui.horizontal(|ui| {
        ui.add_space(side_gap);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(table_w, total_h), egui::Sense::hover());
        let painter = ui.painter();
        let rounding = Rounding::same(8.0);
        painter.rect_filled(rect, rounding, pal.surface_hi);
        painter.rect_stroke(rect, rounding, Stroke::new(1.0, pal.stroke));

        let x1 = rect.left() + col_project;
        let x2 = x1 + col_ide;
        let line = Stroke::new(1.0, pal.stroke.gamma_multiply(0.82));
        for x in [x1, x2] {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                line,
            );
        }
        for idx in 0..=rows.len() {
            let y = rect.top() + header_h + row_h * idx as f32;
            if y < rect.bottom() {
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    line,
                );
            }
        }

        let header_y = rect.top() + header_h * 0.5;
        paint_table_text(
            painter,
            i18n::t("deployment.project"),
            egui::Rect::from_min_max(rect.min, egui::pos2(x1, rect.top() + header_h)),
            header_y,
            pal.text_sec,
            11.0,
            true,
        );
        paint_table_text(
            painter,
            i18n::t("deployment.ide"),
            egui::Rect::from_min_max(
                egui::pos2(x1, rect.top()),
                egui::pos2(x2, rect.top() + header_h),
            ),
            header_y,
            pal.text_sec,
            11.0,
            true,
        );
        paint_table_text(
            painter,
            i18n::t("deployment.status"),
            egui::Rect::from_min_max(
                egui::pos2(x2, rect.top()),
                egui::pos2(rect.right(), rect.top() + header_h),
            ),
            header_y,
            pal.text_sec,
            11.0,
            true,
        );

        for (idx, (project, ide, status, status_color)) in rows.iter().enumerate() {
            let top = rect.top() + header_h + row_h * idx as f32;
            let bottom = top + row_h;
            let row_y = (top + bottom) * 0.5;
            paint_table_text(
                painter,
                &truncate_str(project, 28),
                egui::Rect::from_min_max(egui::pos2(rect.left(), top), egui::pos2(x1, bottom)),
                row_y,
                pal.text,
                12.0,
                false,
            );
            paint_table_text(
                painter,
                &truncate_str(ide, 20),
                egui::Rect::from_min_max(egui::pos2(x1, top), egui::pos2(x2, bottom)),
                row_y,
                pal.text,
                12.0,
                false,
            );
            paint_table_pill(
                painter,
                status,
                *status_color,
                egui::Rect::from_min_max(egui::pos2(x2, top), egui::pos2(rect.right(), bottom)),
                row_y,
            );
        }
    });
}

fn paint_table_text(
    painter: &egui::Painter,
    text: &str,
    cell: egui::Rect,
    center_y: f32,
    color: Color32,
    size: f32,
    strong: bool,
) {
    let _ = strong;
    let font_id = FontId::proportional(size);
    let galley = painter.layout_no_wrap(text.to_string(), font_id, color);
    painter.galley(
        egui::pos2(cell.left() + 12.0, center_y - galley.size().y * 0.5),
        galley,
        color,
    );
}

fn paint_table_pill(
    painter: &egui::Painter,
    text: &str,
    color: Color32,
    cell: egui::Rect,
    center_y: f32,
) {
    let pal = theme::p();
    let bg = Color32::from_rgba_premultiplied(
        ((pal.tag_bg.r() as u16 * 220 + color.r() as u16 * 35) / 255) as u8,
        ((pal.tag_bg.g() as u16 * 220 + color.g() as u16 * 35) / 255) as u8,
        ((pal.tag_bg.b() as u16 * 220 + color.b() as u16 * 35) / 255) as u8,
        255,
    );
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(11.0), color);
    let padding = egui::vec2(8.0, 3.0);
    let size = galley.size() + padding * 2.0;
    let rect = egui::Rect::from_center_size(egui::pos2(cell.center().x, center_y), size);
    painter.rect_filled(rect, Rounding::same(10.0), bg);
    painter.galley(rect.min + padding, galley, color);
}

/// Action footer row: left-aligned buttons with consistent spacing.
#[allow(dead_code)]
pub fn detail_action_footer(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    detail_divider(ui);
    let pal = theme::p();
    ui.label(
        RichText::new(i18n::t("detail.actions").to_uppercase())
            .size(11.0)
            .color(pal.text_sec),
    );
    ui.add_space(4.0);
    content(ui);
    ui.add_space(4.0);
}

/// Compact single-line action toolbar for detail pages.
pub fn detail_action_panel(
    ui: &mut egui::Ui,
    selectors: impl FnOnce(&mut egui::Ui),
    buttons: impl FnOnce(&mut egui::Ui),
) {
    detail_divider(ui);
    let pal = theme::p();
    ui.label(
        RichText::new(i18n::t("detail.actions").to_uppercase())
            .size(11.0)
            .color(pal.text_sec),
    );
    ui.add_space(4.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), COMPACT_CONTROL_H),
        egui::Layout::left_to_right(egui::Align::BOTTOM),
        |ui| {
            ui.set_min_height(COMPACT_CONTROL_H);
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.spacing_mut().interact_size.y = COMPACT_CONTROL_H;
            selectors(ui);
            buttons(ui);
        },
    );
    ui.add_space(4.0);
}

/// Danger footer: divider, left description text, right danger button(s).
pub fn detail_danger_footer(
    ui: &mut egui::Ui,
    description: &str,
    buttons: impl FnOnce(&mut egui::Ui),
) {
    detail_divider(ui);
    let pal = theme::p();
    let row_w = ui.available_width();
    let button_area_w = 120.0;
    ui.allocate_ui_with_layout(
        egui::vec2(row_w, 36.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let left_w = (row_w - button_area_w).max(92.0);
            ui.allocate_ui_with_layout(
                egui::vec2(left_w, 36.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_min_width(left_w);
                    ui.set_max_width(left_w);
                    ui.add(
                        egui::Label::new(
                            RichText::new(i18n::t("detail.danger_zone").to_uppercase())
                                .size(11.0)
                                .color(pal.text_sec),
                        )
                        .truncate(),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(description).size(12.0).color(pal.text_sec))
                            .truncate(),
                    );
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2((row_w - left_w).max(0.0), 36.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add_space(DANGER_BUTTON_RIGHT_INSET);
                    buttons(ui);
                },
            );
        },
    );
    ui.add_space(8.0);
}

pub fn empty_state(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let pal = theme::p();
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).strong().size(16.0).color(pal.text));
        if !subtitle.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(subtitle).size(13.0).color(pal.text_sec));
        }
    });
}

/// Detail header with truncation for long titles. Returns true when close is clicked.
pub fn detail_header(ui: &mut egui::Ui, title: &str, subtitle: &str) -> bool {
    let pal = theme::p();
    let mut closed = false;
    ui.horizontal(|ui| {
        let max_title_w = (ui.available_width() - 70.0).max(60.0);
        ui.vertical(|ui| {
            ui.set_max_width(max_title_w);
            ui.label(
                RichText::new(truncate_str(title, 48))
                    .size(18.0)
                    .strong()
                    .color(pal.text),
            );
            if !subtitle.is_empty() {
                ui.label(
                    RichText::new(truncate_str(subtitle, 64))
                        .size(12.0)
                        .color(pal.text_sec)
                        .monospace(),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(i18n::t("common.close"))
                            .size(12.0)
                            .color(pal.text_sec),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, pal.stroke))
                    .rounding(Rounding::same(6.0))
                    .min_size(egui::vec2(48.0, 24.0)),
                )
                .clicked()
            {
                closed = true;
            }
        });
    });
    ui.add_space(4.0);
    let rect = ui.available_rect_before_wrap();
    let min_x = rect.min.x + DETAIL_DIVIDER_LEFT_INSET;
    let max_x = (rect.max.x - DETAIL_GUTTER - DETAIL_DIVIDER_RIGHT_INSET).max(min_x);
    ui.painter().line_segment(
        [egui::pos2(min_x, rect.min.y), egui::pos2(max_x, rect.min.y)],
        Stroke::new(1.0, pal.stroke),
    );
    ui.add_space(8.0);
    closed
}

pub fn detail_section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    let pal = theme::p();
    ui.add_space(2.0);
    ui.label(
        RichText::new(title.to_uppercase())
            .size(11.0)
            .color(pal.text_sec),
    );
    ui.add_space(4.0);
    content(ui);
    ui.add_space(10.0);
}

pub fn search_bar(ui: &mut egui::Ui, filter: &mut String, hint: &str) {
    search_bar_with_right_gutter(ui, filter, hint, 0.0);
}

pub fn search_bar_with_right_gutter(
    ui: &mut egui::Ui,
    filter: &mut String,
    hint: &str,
    right_gutter: f32,
) {
    let width = (ui.available_width() - right_gutter).max(120.0);
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| search_bar_inner(ui, filter, hint),
    );
}

fn search_bar_inner(ui: &mut egui::Ui, filter: &mut String, hint: &str) {
    let pal = theme::p();
    egui::Frame::none()
        .fill(pal.surface_hi)
        .stroke(Stroke::new(1.0, pal.stroke))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(filter)
                    .desired_width(ui.available_width())
                    .hint_text(hint)
                    .frame(false),
            );
        });
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_chars - 1).collect();
        result.push('\u{2026}');
        result
    }
}
