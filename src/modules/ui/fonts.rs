use std::sync::Arc;
use three_d::egui::{Context, FontData, FontDefinitions, FontFamily};

const NOTO_SANS_SC: &[u8] = include_bytes!("../../../assets/fonts/NotoSansSC-Regular.ttf");

pub fn configure(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto_sans_sc".to_owned(),
        Arc::new(FontData::from_static(NOTO_SANS_SC)),
    );

    // Keep the built-in proportional fonts first so Latin text renders with
    // egui's defaults; Noto Sans SC falls back for CJK glyphs.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("noto_sans_sc".to_owned());

    // The built-in monospace fonts must stay first so logs and time codes
    // render in a true monospaced face; Noto covers CJK inside monospace
    // output only.
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("noto_sans_sc".to_owned());

    // Register Phosphor icon font (appended after CJK text font)
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    ctx.set_fonts(fonts);
}
