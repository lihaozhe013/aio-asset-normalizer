use std::sync::Arc;
use three_d::egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn configure(ctx: &Context) {
    if let Some(font_data) = load_system_font() {
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "system_cjk".to_owned(),
            Arc::new(FontData::from_owned(font_data)),
        );
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .push("system_cjk".to_owned());
        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .push("system_cjk".to_owned());
        ctx.set_fonts(fonts);
    }
}

fn load_system_font() -> Option<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        for path in [
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyhbd.ttf",
            "C:\\Windows\\Fonts\\simhei.ttf",
        ] {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        for path in [
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
        ] {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for path in [
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        ] {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }
    None
}
