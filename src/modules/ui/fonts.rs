use std::sync::Arc;
use three_d::egui::{Context, FontData, FontDefinitions, FontFamily};

const NOTO_SANS_SC: &[u8] =
    include_bytes!("../../../assets/fonts/NotoSansSC-Regular.ttf");

pub fn configure(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto_sans_sc".to_owned(),
        Arc::new(FontData::from_static(NOTO_SANS_SC)),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let list = fonts.families.get_mut(&family).unwrap();
        list.clear();
        list.push("noto_sans_sc".to_owned());
    }

    // Register Phosphor icon font (appended after CJK text font)
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    ctx.set_fonts(fonts);
}
