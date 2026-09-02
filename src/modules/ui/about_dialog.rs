use crate::app::App;

pub fn render(app: &mut App, ctx: &three_d::egui::Context) {
    use three_d::egui::*;
    if !app.show_about {
        return;
    }
    const ICON_PNG: &[u8] = include_bytes!(
        "../../../assets/icon/aio-asset-normalizer-transparent.png"
    );
    if app.about_icon.is_none() {
        let decoded = image::load_from_memory(ICON_PNG)
            .expect("failed to decode the About dialog icon")
            .to_rgba8();
        let image = ColorImage::from_rgba_unmultiplied(
            [decoded.width() as usize, decoded.height() as usize],
            decoded.as_raw(),
        );
        app.about_icon = Some(ctx.load_texture(
            "aio-asset-normalizer-about-icon",
            image,
            TextureOptions::LINEAR,
        ));
    }
    let icon = app.about_icon.as_ref().unwrap().clone();
    Window::new(app.i18n.tr("about.title"))
        .open(&mut app.show_about)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let frame_size = vec2(88.0, 88.0);
                let available = ui.available_width();
                let left_pad = ((available - frame_size.x) / 2.0).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(left_pad);
                    Frame::NONE
                        .fill(Color32::from_rgb(248, 240, 230))
                        .stroke(Stroke::new(
                            1.0_f32,
                            Color32::from_rgb(224, 198, 176),
                        ))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.add(
                                Image::from_texture(&icon)
                                    .fit_to_exact_size(vec2(72.0, 72.0)),
                            );
                        });
                });
                ui.heading("AIO Asset Normalizer");
                ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.label(app.i18n.tr("label.description"));
            });
        });
}
