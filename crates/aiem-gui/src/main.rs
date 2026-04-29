#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

mod app;
mod i18n;
mod tasks;
mod theme;
mod ui;
mod views;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("aiem=info,aiem_gui=info,aiem_core=info")
            }),
        )
        .with_target(false)
        .compact()
        .init();

    let icon = load_app_icon();
    let mut vp = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([1100.0, 600.0])
        .with_title("aiem - AI Extension Manager");
    if let Some(icon) = icon {
        vp = vp.with_icon(std::sync::Arc::new(icon));
    }
    let native = eframe::NativeOptions {
        viewport: vp,
        persist_window: true,
        follow_system_theme: false,
        ..Default::default()
    };

    eframe::run_native(
        "aiem",
        native,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "inter".to_owned(),
                egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf"))
                    .into(),
            );
            fonts.font_data.insert(
                "jetbrains-mono".to_owned(),
                egui::FontData::from_static(include_bytes!(
                    "../assets/fonts/JetBrainsMono-Regular.ttf"
                ))
                .into(),
            );
            if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                list.insert(0, "inter".to_owned());
            }
            if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                list.insert(0, "jetbrains-mono".to_owned());
            }

            #[cfg(target_os = "windows")]
            let cjk_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\NotoSansSC-VF.ttf",
                "C:\\Windows\\Fonts\\MiSans-Regular.otf",
                "C:\\Windows\\Fonts\\msyh.ttc",
                "C:\\Windows\\Fonts\\Deng.ttf",
                "C:\\Windows\\Fonts\\simhei.ttf",
                "C:\\Windows\\Fonts\\simsun.ttc",
                "C:\\Windows\\Fonts\\msjh.ttc",
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
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
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
                    fonts
                        .font_data
                        .insert("cjk".to_owned(), egui::FontData::from_owned(data).into());
                    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                        let cjk_index = if list.iter().any(|name| name == "inter") {
                            1
                        } else {
                            0
                        };
                        list.insert(cjk_index, "cjk".to_owned());
                    }
                    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                        let cjk_index = if list.iter().any(|name| name == "jetbrains-mono") {
                            1
                        } else {
                            0
                        };
                        list.insert(cjk_index, "cjk".to_owned());
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

fn load_app_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../../../pic/aiem.png");
    let img = image::load_from_memory_with_format(icon_bytes, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}
