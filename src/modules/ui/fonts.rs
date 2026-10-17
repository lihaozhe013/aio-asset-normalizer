use std::sync::Arc;
use three_d::egui::{Context, FontData, FontDefinitions, FontFamily};

const NOTO_SANS_SC: &[u8] = include_bytes!("../../../assets/fonts/NotoSansSC-Regular.ttf");

pub fn configure(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto_sans_sc".to_owned(),
        Arc::new(FontData::from_static(NOTO_SANS_SC)),
    );

    // Noto Sans SC is the primary UI face so Latin and CJK glyphs share one
    // style; the built-in fonts stay behind it as symbol fallbacks.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "noto_sans_sc".to_owned());

    // The built-in monospace fonts must stay first so logs and time codes
    // render in a true monospaced face; Noto covers CJK inside monospace
    // output only.
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("noto_sans_sc".to_owned());

    // Register Phosphor icon font; egui-phosphor inserts it at index 1 so
    // icons resolve right after the primary face.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    ctx.set_fonts(fonts);
}
