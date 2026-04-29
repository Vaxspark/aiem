//! Theme -- white minimalist with dark-mode toggle.

use eframe::egui::{self, Color32, FontFamily, FontId, Rounding, Shadow, Stroke, Visuals};

// ─── Mode ───────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

/// Global palette resolved from the current mode.
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub surface_hi: Color32,
    pub surface_hov: Color32,
    pub stroke: Color32,
    pub text: Color32,
    pub text_sec: Color32,
    pub accent: Color32,
    pub accent_fg: Color32,
    pub danger: Color32,
    pub tag_bg: Color32,
    #[allow(dead_code)]
    pub tag_fg: Color32,
    pub sidebar_bg: Color32,
    pub selected: Color32,
    pub warning: Color32,
}

pub fn palette(mode: Mode) -> Palette {
    match mode {
        Mode::Light => Palette {
            bg: Color32::from_rgb(0xF5, 0xF5, 0xF7),
            surface: Color32::WHITE,
            surface_hi: Color32::from_rgb(0xEC, 0xEC, 0xEE),
            surface_hov: Color32::from_rgb(0xE4, 0xE4, 0xE8),
            stroke: Color32::from_rgb(0xD1, 0xD1, 0xD6),
            text: Color32::from_rgb(0x1D, 0x1D, 0x1F),
            text_sec: Color32::from_rgb(0x86, 0x86, 0x8B),
            accent: Color32::from_rgb(0x00, 0x7A, 0xFF),
            accent_fg: Color32::WHITE,
            danger: Color32::from_rgb(0xFF, 0x3B, 0x30),
            tag_bg: Color32::from_rgb(0xEC, 0xEC, 0xEE),
            tag_fg: Color32::from_rgb(0x63, 0x63, 0x66),
            sidebar_bg: Color32::from_rgb(0xEF, 0xEF, 0xF2),
            selected: Color32::from_rgb(0xE6, 0xF0, 0xFF),
            warning: Color32::from_rgb(0xFF, 0x9F, 0x0A),
        },
        Mode::Dark => Palette {
            bg: Color32::from_rgb(0x16, 0x16, 0x18),
            surface: Color32::from_rgb(0x2C, 0x2C, 0x2E),
            surface_hi: Color32::from_rgb(0x3A, 0x3A, 0x3C),
            surface_hov: Color32::from_rgb(0x48, 0x48, 0x4A),
            stroke: Color32::from_rgb(0x38, 0x38, 0x3A),
            text: Color32::from_rgb(0xF5, 0xF5, 0xF7),
            text_sec: Color32::from_rgb(0x8E, 0x8E, 0x93),
            accent: Color32::from_rgb(0x0A, 0x84, 0xFF),
            accent_fg: Color32::WHITE,
            danger: Color32::from_rgb(0xFF, 0x45, 0x3A),
            tag_bg: Color32::from_rgb(0x38, 0x38, 0x3A),
            tag_fg: Color32::from_rgb(0x8E, 0x8E, 0x93),
            sidebar_bg: Color32::from_rgb(0x1E, 0x1E, 0x20),
            selected: Color32::from_rgb(0x1A, 0x2A, 0x40),
            warning: Color32::from_rgb(0xFF, 0xA6, 0x1A),
        },
    }
}

// --- Legacy constants -- point to light-mode defaults for views that still
//     reference theme::TEXT() etc. We'll set these via a thread-local palette. ──

use std::cell::RefCell;
thread_local! {
    static CURRENT: RefCell<Mode> = RefCell::new(Mode::Light);
}

pub fn current_mode() -> Mode {
    CURRENT.with(|c| *c.borrow())
}

pub fn set_mode(m: Mode) {
    CURRENT.with(|c| *c.borrow_mut() = m);
}

pub fn p() -> Palette {
    palette(current_mode())
}

#[allow(non_snake_case)]
pub fn ACCENT() -> Color32 {
    p().accent
}
#[allow(non_snake_case)]
pub fn DANGER() -> Color32 {
    p().danger
}
#[allow(non_snake_case)]
#[allow(non_snake_case)]
pub fn WARNING() -> Color32 {
    p().warning
}
#[allow(non_snake_case)]
pub fn SUCCESS() -> Color32 {
    match current_mode() {
        Mode::Light => Color32::from_rgb(0x34, 0xC7, 0x59),
        Mode::Dark => Color32::from_rgb(0x30, 0xD1, 0x58),
    }
}

/// Toast shadow — slightly more prominent.
pub fn toast_shadow() -> Shadow {
    match current_mode() {
        Mode::Light => Shadow {
            offset: egui::vec2(0.0, 2.0),
            blur: 10.0,
            spread: 0.0,
            color: Color32::from_black_alpha(18),
        },
        Mode::Dark => Shadow {
            offset: egui::vec2(0.0, 2.0),
            blur: 10.0,
            spread: 0.0,
            color: Color32::from_black_alpha(60),
        },
    }
}

// --- Install ---

pub fn install(ctx: &egui::Context, mode: Mode) {
    set_mode(mode);
    let p = palette(mode);

    let mut v = match mode {
        Mode::Light => Visuals::light(),
        Mode::Dark => Visuals::dark(),
    };

    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.stroke);
    v.window_rounding = Rounding::same(10.0);
    v.extreme_bg_color = p.surface_hi;
    v.faint_bg_color = p.surface_hi;
    v.selection.bg_fill = match mode {
        Mode::Light => Color32::from_rgba_premultiplied(0x00, 0x7A, 0xFF, 20),
        Mode::Dark => Color32::from_rgba_premultiplied(0x0A, 0x84, 0xFF, 38),
    };
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.text_sec;

    let r = Rounding::same(6.0);
    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.stroke);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.noninteractive.rounding = r;

    v.widgets.inactive.bg_fill = p.surface_hi;
    v.widgets.inactive.weak_bg_fill = p.surface_hi;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.stroke);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.rounding = r;
    v.widgets.inactive.expansion = 0.0;

    v.widgets.hovered.bg_fill = p.surface_hov;
    v.widgets.hovered.weak_bg_fill = p.surface_hov;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.text_sec);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.rounding = r;

    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.weak_bg_fill = p.accent;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.5, p.accent_fg);
    v.widgets.active.rounding = r;

    v.widgets.open.bg_fill = p.surface_hi;
    v.widgets.open.weak_bg_fill = p.surface_hi;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.stroke);
    v.widgets.open.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.open.rounding = r;

    ctx.set_visuals(v);

    // Typography
    use egui::TextStyle::*;
    let mut style = (*ctx.style()).clone();
    style
        .text_styles
        .insert(Heading, FontId::new(21.0, FontFamily::Proportional));
    style
        .text_styles
        .insert(Body, FontId::new(14.0, FontFamily::Proportional));
    style
        .text_styles
        .insert(Monospace, FontId::new(14.0, FontFamily::Monospace));
    style
        .text_styles
        .insert(Button, FontId::new(14.0, FontFamily::Proportional));
    style
        .text_styles
        .insert(Small, FontId::new(12.0, FontFamily::Proportional));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    style.spacing.window_margin = egui::Margin::same(14.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.scroll.bar_width = crate::ui::SCROLLBAR_WIDTH;
    style.spacing.scroll.floating_width = crate::ui::SCROLLBAR_WIDTH;
    style.spacing.scroll.floating_allocated_width = 0.0;
    style.spacing.scroll.bar_inner_margin = 4.0;
    style.spacing.scroll.bar_outer_margin = 0.0;
    ctx.set_style(style);
}
