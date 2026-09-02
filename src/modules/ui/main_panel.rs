use crate::app::App;
use crate::modules::ui::{
    about_dialog, bottom_panel, bvh_inspector, fbx_converter_panel, fonts,
    glb_inspector,
    menu_bar::{self, Page},
};

pub fn render_ui(
    app: &mut App,
    ui: &mut three_d::egui::Ui,
    _window_width: u32,
) -> three_d::egui::Rect {
    use three_d::egui::*;

    if !app.fonts_configured {
        fonts::configure(ui.ctx());
        app.fonts_configured = true;
    }

    match app.page {
        Page::GlbEditor => app.file_tree.handle_dropped_files(ui.ctx()),
        Page::BvhStudio => app.bvh_file_tree.handle_dropped_files(ui.ctx()),
        Page::FbxConverter => {
            app.converter_file_tree.handle_dropped_files(ui.ctx())
        }
    }
    app.poll_tasks();
    let actions = menu_bar::render(
        ui,
        &app.i18n,
        app.page,
        app.canvas.show_grid,
        app.canvas.show_axes,
        app.canvas.show_origin,
    );
    for action in actions {
        app.dispatch_action(&action);
    }
    render_page_tabs(app, ui);

    match app.page {
        Page::GlbEditor => {
            Panel::left("glb_files")
                .resizable(true)
                .default_size(250.0)
                .min_size(170.0)
                .show_inside(ui, |ui| {
                    let (changed, preview_path) =
                        app.file_tree.render(ui, &app.i18n);
                    if changed || app.file_tree.take_root_changed() {
                        app.needs_save = true;
                    }
                    if let Some(path) = preview_path {
                        app.preview_glb(&path);
                    }
                });
        }
        Page::BvhStudio => {
            Panel::left("bvh_files")
                .resizable(true)
                .default_size(250.0)
                .min_size(170.0)
                .show_inside(ui, |ui| {
                    if let Some(path) = app.bvh_file_tree.render(ui, &app.i18n)
                    {
                        app.load_bvh_path(&path);
                    }
                });
        }
        Page::FbxConverter => {
            Panel::left("converter_files")
                .resizable(true)
                .default_size(250.0)
                .min_size(170.0)
                .show_inside(ui, |ui| {
                    let (changed, _) =
                        app.converter_file_tree.render(ui, &app.i18n);
                    if changed || app.converter_file_tree.take_root_changed() {
                        app.needs_save = true;
                    }
                });
        }
    }

    // The FBX Converter page has no inspector and no 3D preview; its
    // controls live in the central area instead.
    if app.page != Page::FbxConverter {
        Panel::right("inspector")
            .resizable(true)
            .default_size(320.0)
            .min_size(250.0)
            .show_inside(ui, |ui| {
                ScrollArea::vertical().show(ui, |ui| match app.page {
                    Page::GlbEditor => glb_inspector::render(app, ui),
                    Page::BvhStudio => bvh_inspector::render(app, ui),
                    Page::FbxConverter => {}
                });
            });
    }

    bottom_panel::render(app, ui);

    let content_rect = CentralPanel::no_frame()
        .show_inside(ui, |ui| {
            if app.page == Page::FbxConverter {
                fbx_converter_panel::render(app, ui);
            }
            let (_, rect) = ui.allocate_space(ui.available_size());
            rect
        })
        .inner;
    about_dialog::render(app, ui.ctx());
    content_rect
}

fn render_page_tabs(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;
    Frame::NONE
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("AIO Asset Normalizer").strong());
                ui.separator();
                if ui
                    .selectable_label(
                        app.page == Page::GlbEditor,
                        app.i18n.tr("page.glb_editor"),
                    )
                    .clicked()
                {
                    app.page = Page::GlbEditor;
                    if app.glb_retarget_preview_active {
                        app.exit_glb_retarget_preview();
                    } else {
                        app.request_glb_reload(
                            crate::reload::GlbReloadKind::OpenModel,
                        );
                    }
                }
                if ui
                    .selectable_label(
                        app.page == Page::BvhStudio,
                        app.i18n.tr("page.bvh_studio"),
                    )
                    .clicked()
                {
                    app.page = Page::BvhStudio;
                    app.canvas.show_origin = false;
                    if app.glb_retarget_preview_active {
                        app.exit_glb_retarget_preview();
                    }
                    app.needs_bvh_target_reload = true;
                    app.refresh_v2_retarget_mapping();
                }
                if ui
                    .selectable_label(
                        app.page == Page::FbxConverter,
                        app.i18n.tr("page.fbx_converter"),
                    )
                    .clicked()
                {
                    app.page = Page::FbxConverter;
                    if app.glb_retarget_preview_active {
                        app.exit_glb_retarget_preview();
                    }
                }
            });
        });
    ui.separator();
}
