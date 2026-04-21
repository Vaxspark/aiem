//! Theme -- white minimalist with dark-mode toggle.

use eframe::egui::{self, Color32, FontFamily, FontId, Rounding, Shadow, Stroke, Visuals};

// ─── Mode ───────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode { Light, Dark }

/// Global palette resolved from the current mode.
pub struct Palette {
    pub bg:          Color32,
    pub surface:     Color32,
    pub surface_hi:  Color32,
    pub surface_hov: Color32,
    pub stroke:      Color32,
    pub text:        Color32,
    pub text_sec:    Color32,   // secondary / muted text
    pub accent:      Color32,   // primary action
    pub accent_fg:   Color32,   // text on accent bg
    pub danger:      Color32,
    pub tag_bg:      Color32,
    pub tag_fg:      Color32,
}

pub fn palette(mode: Mode) -> Palette {
    match mode {
        Mode::Light => Palette {
            bg:          Color32::from_rgb(0xF7, 0xF7, 0xF7),
            surface:     Color32::WHITE,
            surface_hi:  Color32::from_rgb(0xF0, 0xF0, 0xF0),
            surface_hov: Color32::from_rgb(0xEA, 0xEA, 0xEA),
            stroke:      Color32::from_rgb(0xDF, 0xDF, 0xDF),
            text:        Color32::from_rgb(0x1A, 0x1A, 0x1A),
            text_sec:    Color32::from_rgb(0x88, 0x88, 0x88),
            accent:      Color32::from_rgb(0x1A, 0x1A, 0x1A),
            accent_fg:   Color32::WHITE,
            danger:      Color32::from_rgb(0xD0, 0x44, 0x44),
            tag_bg:      Color32::from_rgb(0xEE, 0xEE, 0xEE),
            tag_fg:      Color32::from_rgb(0x55, 0x55, 0x55),
        },
        Mode::Dark => Palette {
            bg:          Color32::from_rgb(0x0E, 0x0E, 0x0E),
            surface:     Color32::from_rgb(0x18, 0x18, 0x18),
            surface_hi:  Color32::from_rgb(0x1E, 0x1E, 0x1E),
            surface_hov: Color32::from_rgb(0x24, 0x24, 0x24),
            stroke:      Color32::from_rgb(0x2E, 0x2E, 0x2E),
            text:        Color32::from_rgb(0xE8, 0xE8, 0xE8),
            text_sec:    Color32::from_rgb(0x6E, 0x6E, 0x6E),
            accent:      Color32::WHITE,
            accent_fg:   Color32::BLACK,
            danger:      Color32::from_rgb(0xCC, 0x55, 0x55),
            tag_bg:      Color32::from_rgb(0x24, 0x24, 0x24),
            tag_fg:      Color32::from_rgb(0x99, 0x99, 0x99),
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

pub fn p() -> Palette { palette(current_mode()) }

// Legacy compat: used by views as theme::TEXT(), theme::MUTED(), etc.
// These are functions (not const) so they track the current mode.
#[allow(non_snake_case)] pub fn TEXT()    -> Color32 { p().text }
#[allow(non_snake_case)] pub fn MUTED()   -> Color32 { p().text_sec }
#[allow(non_snake_case)] pub fn ACCENT()  -> Color32 { p().accent }
#[allow(non_snake_case)] pub fn DANGER()  -> Color32 { p().danger }
#[allow(non_snake_case)] pub fn SUCCESS() -> Color32 {
    match current_mode() {
        Mode::Light => Color32::from_rgb(0x1E, 0x7A, 0x3A), // dark green
        Mode::Dark  => Color32::from_rgb(0x4A, 0xBB, 0x6A), // bright green
    }
}
#[allow(non_snake_case)] pub fn SURFACE() -> Color32 { p().surface }

/// Subtle card shadow for elevation.
pub fn card_shadow() -> Shadow {
    match current_mode() {
        Mode::Light => Shadow {
            offset: egui::vec2(0.0, 1.0),
            blur: 4.0,
            spread: 0.0,
            color: Color32::from_black_alpha(10),
        },
        Mode::Dark => Shadow {
            offset: egui::vec2(0.0, 1.0),
            blur: 6.0,
            spread: 0.0,
            color: Color32::from_black_alpha(40),
        },
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
        Mode::Dark  => Visuals::dark(),
    };

    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.stroke);
    v.window_rounding = Rounding::same(10.0);
    v.extreme_bg_color = p.surface_hi;
    v.faint_bg_color = p.surface_hi;
    v.selection.bg_fill = match mode {
        Mode::Light => Color32::from_rgb(0xD8, 0xD8, 0xD8),
        Mode::Dark  => Color32::from_rgb(0x38, 0x38, 0x38),
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
    style.text_styles.insert(Heading, FontId::new(20.0, FontFamily::Proportional));
    style.text_styles.insert(Body,    FontId::new(13.5, FontFamily::Proportional));
    style.text_styles.insert(Monospace, FontId::new(12.5, FontFamily::Monospace));
    style.text_styles.insert(Button,  FontId::new(13.5, FontFamily::Proportional));
    style.text_styles.insert(Small,   FontId::new(11.5, FontFamily::Proportional));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    style.spacing.window_margin = egui::Margin::same(14.0);
    style.spacing.interact_size.y = 30.0;
    ctx.set_style(style);
}

// ─── Tag ────────────────────────────────────────────────────────────────────

/// A minimal pill tag with tinted background from `color`.
pub fn tag(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let pal = p();
    // Mix the accent color into the bg at ~15% opacity for a subtle tint
    let bg = Color32::from_rgba_premultiplied(
        ((pal.tag_bg.r() as u16 * 220 + color.r() as u16 * 35) / 255) as u8,
        ((pal.tag_bg.g() as u16 * 220 + color.g() as u16 * 35) / 255) as u8,
        ((pal.tag_bg.b() as u16 * 220 + color.b() as u16 * 35) / 255) as u8,
        255,
    );
    let fg = color;
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::proportional(11.0),
        fg,
    );
    let padding = egui::vec2(8.0, 3.0);
    let size = galley.size() + padding * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
    ui.painter().rect_stroke(rect, Rounding::same(10.0), Stroke::new(0.5, pal.stroke));
    ui.painter().galley(rect.min + padding, galley, fg);
    resp
}
