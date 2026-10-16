use crate::app::App;
use crate::modules::ui::{
    about_dialog, bottom_panel, bvh_inspector, empty_state, fbx_converter_panel, fonts,
    glb_inspector,
    menu_bar::{MenuAction, Page},
    tabs, theme,
};
use egui_phosphor::regular::{ARROWS_CLOCKWISE, CUBE, PERSON_SIMPLE_RUN};
#[cfg(not(target_os = "macos"))]
use three_d::egui::Key;
use three_d::egui::{self, Panel, ScrollArea};

pub fn render_ui(
    app: &mut App,
    ui: &mut three_d::egui::Ui,
    _window_width: u32,
) -> three_d::egui::Rect {
    if !app.ui_configured {
        fonts::configure(ui.ctx());
        theme::apply(ui.ctx());
        app.ui_configured = true;
    }

    match app.page {
        Page::GlbEditor => app.file_tree.handle_dropped_files(ui.ctx()),
        Page::BvhStudio => app.bvh_file_tree.handle_dropped_files(ui.ctx()),
        Page::FbxConverter => app.converter_file_tree.handle_dropped_files(ui.ctx()),
    }
    app.poll_tasks();
    #[cfg(not(target_os = "macos"))]
    for action in keyboard_shortcuts(ui, app.page) {
        app.dispatch_action(&action);
    }
    // macOS hosts its menu in the system bar; other platforms share this panel
    // with the tabs to keep a continuous frame around the workspace.
    #[cfg(not(target_os = "macos"))]
    let mut actions = Vec::new();
    let mut tab_clicked: Option<usize> = None;
    Panel::top("top_bar")
        .frame(
            egui::Frame::default()
                .fill(theme::BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::symmetric(6, 3)),
        )
        .show_inside(ui, |ui| {
            #[cfg(not(target_os = "macos"))]
            {
                actions = crate::modules::ui::menu_bar::render(
                    ui,
                    &app.i18n,
                    app.canvas.show_grid,
                    app.canvas.show_axes,
                    app.canvas.show_origin,
                );
            }
            tab_clicked = render_page_tabs(app, ui);
        });
    #[cfg(not(target_os = "macos"))]
    for action in actions {
        app.dispatch_action(&action);
    }
    if let Some(index) = tab_clicked {
        let order = [Page::GlbEditor, Page::BvhStudio, Page::FbxConverter];
        switch_page(app, order[index]);
    }

    match app.page {
        Page::GlbEditor => {
            Panel::left("glb_files")
                .resizable(true)
                .default_size(250.0)
                .min_size(170.0)
                .show_inside(ui, |ui| {
                    let (changed, preview_path) =
                        app.file_tree
                            .render_with_active(ui, &app.i18n, app.glb_path.as_deref());
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
                    if let Some(path) = app.bvh_file_tree.render(ui, &app.i18n) {
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
                    let (changed, _) = app.converter_file_tree.render(ui, &app.i18n);
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

    let content_rect = egui::CentralPanel::no_frame()
        .show_inside(ui, |ui| {
            if app.page == Page::FbxConverter {
                fbx_converter_panel::render(app, ui);
            } else if page_is_empty(app) {
                render_empty_state(app, ui);
            }
            let (_, rect) = ui.allocate_space(ui.available_size());
            rect
        })
        .inner;
    about_dialog::render(app, ui.ctx());
    crate::app_mcp_setup::render(app, ui.ctx());
    content_rect
}

/// Whether the active page has no content to preview in the 3D viewport.
fn page_is_empty(app: &App) -> bool {
    match app.page {
        Page::GlbEditor => app.glb.is_none() && !app.canvas.has_glb_skeleton(),
        Page::BvhStudio => app.canvas.skeleton.is_none() && app.canvas.target_skeleton.is_none(),
        Page::FbxConverter => false,
    }
}

fn render_empty_state(app: &mut App, ui: &mut three_d::egui::Ui) {
    let (icon, title_key, hint_key, action) = match app.page {
        Page::GlbEditor => (
            CUBE,
            "empty.glb_title",
            "empty.glb_hint",
            MenuAction::ImportGlb,
        ),
        Page::BvhStudio => (
            PERSON_SIMPLE_RUN,
            "empty.bvh_title",
            "empty.bvh_hint",
            MenuAction::ImportBvh,
        ),
        Page::FbxConverter => return,
    };
    let action_label = match action {
        MenuAction::ImportGlb => app.i18n.tr("menu.import_glb"),
        _ => app.i18n.tr("menu.import_bvh"),
    };
    let clicked = empty_state::render(
        ui,
        icon,
        app.i18n.tr(title_key),
        app.i18n.tr(hint_key),
        Some(action_label),
    );
    if clicked {
        app.dispatch_action(&action);
    }
}

/// Global keyboard shortcuts mapped onto existing menu actions.
///
/// The command modifier maps to Cmd on macOS and Ctrl elsewhere.
#[cfg(not(target_os = "macos"))]
fn keyboard_shortcuts(ui: &three_d::egui::Ui, page: Page) -> Vec<MenuAction> {
    ui.input(|input| {
        let mut actions = Vec::new();
        if !input.modifiers.command {
            return actions;
        }
        if input.key_pressed(Key::O) {
            actions.push(match page {
                Page::BvhStudio => MenuAction::ImportBvh,
                _ => MenuAction::ImportGlb,
            });
        }
        if input.key_pressed(Key::S) {
            actions.push(MenuAction::Save);
        }
        if input.key_pressed(Key::E) {
            actions.push(MenuAction::Export);
        }
        actions
    })
}

fn render_page_tabs(app: &mut App, ui: &mut three_d::egui::Ui) -> Option<usize> {
    let entries = [
        tabs::TabEntry {
            icon: CUBE,
            label: app.i18n.tr("page.glb_editor").to_owned(),
        },
        tabs::TabEntry {
            icon: PERSON_SIMPLE_RUN,
            label: app.i18n.tr("page.bvh_studio").to_owned(),
        },
        tabs::TabEntry {
            icon: ARROWS_CLOCKWISE,
            label: app.i18n.tr("page.fbx_converter").to_owned(),
        },
    ];
    let selected = [Page::GlbEditor, Page::BvhStudio, Page::FbxConverter]
        .iter()
        .position(|page| *page == app.page)
        .unwrap_or(0);
    ui.horizontal(|ui| tabs::segmented(ui, &entries, selected))
        .inner
}

/// Apply a page switch with the per-page reset and reload side effects.
///
/// Re-selecting the active page keeps its original refresh semantics, so
/// there is deliberately no same-page guard here.
fn switch_page(app: &mut App, page: Page) {
    app.page = page;
    app.canvas.clear_glb_skeleton();
    match page {
        Page::GlbEditor => {
            if app.glb_retarget_preview_active {
                app.exit_glb_retarget_preview();
            } else {
                app.request_glb_reload(crate::reload::GlbReloadKind::OpenModel);
            }
        }
        Page::BvhStudio => {
            app.canvas.show_origin = false;
            if app.glb_retarget_preview_active {
                app.exit_glb_retarget_preview();
            }
            app.needs_bvh_target_reload = true;
            app.refresh_v2_retarget_mapping();
        }
        Page::FbxConverter => {
            if app.glb_retarget_preview_active {
                app.exit_glb_retarget_preview();
            }
        }
    }
}
