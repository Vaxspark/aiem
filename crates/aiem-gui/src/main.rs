#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

mod app;
mod theme;
mod i18n;
mod views;
mod tasks;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("aiem=info,aiem_gui=info,aiem_core=info")),
        )
        .with_target(false)
        .compact()
        .init();

    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([1000.0, 600.0])
            .with_title("aiem — AI Extension Manager"),
        persist_window: true,        follow_system_theme: false,        ..Default::default()
    };

    eframe::run_native(
        "aiem",
        native,
        Box::new(|cc| {
            // Load a CJK font as fallback for Chinese/Japanese/Korean characters.
            // Searches common system font locations across Windows/macOS/Linux.
            let mut fonts = egui::FontDefinitions::default();
            #[cfg(target_os = "windows")]
            let cjk_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\msyh.ttc",      // Microsoft YaHei
                "C:\\Windows\\Fonts\\simhei.ttf",    // SimHei
                "C:\\Windows\\Fonts\\simsun.ttc",    // SimSun
                "C:\\Windows\\Fonts\\msjh.ttc",      // Microsoft JhengHei (TW)
            ];
            #[cfg(target_os = "macos")]
            let cjk_paths: &[&str] = &[
                "/System/Library/Fonts/PingFang.ttc",
                "/System/Library/Fonts/STHeiti Light.ttc",
                "/System/Library/Fonts/STHeiti Medium.ttc",
                "/Library/Fonts/Arial Unicode.ttf",
                "/System/Library/Fonts/Hiragino Sans GB.ttc",
            ];
            #[cfg(target_os = "linux")]
            let cjk_paths: &[&str] = &[
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
                "/usr/share/fonts/wqy-zenhei/wqy-zenhei.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
                "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto/NotoSansCJKsc-Regular.otf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ];
            for path in cjk_paths {
                if let Ok(data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "cjk".to_owned(),
                        egui::FontData::from_owned(data).into(),
                    );
                    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                        list.push("cjk".to_owned());
                    }
                    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                        list.push("cjk".to_owned());
                    }
                    tracing::debug!("loaded CJK font from {path}");
                    break;
                }
            }
            cc.egui_ctx.set_fonts(fonts);

            theme::install(&cc.egui_ctx, theme::Mode::Light);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc)))
        }),
    )
}
