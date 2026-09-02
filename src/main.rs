#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod app_bvh;
mod app_export;
mod app_fbx_converter;
mod app_preview;
mod app_retarget;
mod app_retarget_prompt;
mod app_ui;
mod modules;
mod reload;
mod window;

use app::App;
use three_d::*;

fn main() {
    let prefs = modules::preferences::load();

    let window = window::create().expect("Failed to create window");

    let context = window.gl();
    let mut gui = three_d::GUI::new(&context);
    let mut app = App::new(&context, window.viewport(), &prefs);

    window.render_loop(move |mut frame_input| {
        let mut content_rect: Option<egui::Rect> = None;

        gui.update(
            &mut frame_input.events,
            frame_input.accumulated_time,
            frame_input.viewport,
            frame_input.device_pixel_ratio,
            |gui_ctx| {
                content_rect =
                    Some(app.render_ui(gui_ctx, frame_input.window_width));
            },
        );

        let dpr = frame_input.device_pixel_ratio;
        let viewport = content_rect
            .map(|rect| {
                viewport_from_content_rect(rect, frame_input.viewport, dpr)
            })
            .unwrap_or(Viewport {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            });

        // The FBX Converter page has no 3D canvas; skip viewport work
        // entirely while it is active.
        let show_scene = app.page != modules::ui::menu_bar::Page::FbxConverter;
        if show_scene {
            app.camera.handle_events(&frame_input.events, viewport);
        }

        app.reload_model_if_needed(&context);

        app.camera.set_viewport(viewport);

        let screen = frame_input.screen();
        let mut clear_rt = screen
            .clear(ClearState::color_and_depth(0.12, 0.13, 0.17, 1.0, 1.0));

        if show_scene && app.canvas.show_axes {
            for axis in &app.canvas.axes {
                clear_rt = clear_rt.render_partially(
                    viewport.into(),
                    &app.camera.camera,
                    axis,
                    &[],
                );
            }
        }
        if show_scene && app.canvas.show_grid {
            for line in &app.canvas.grid {
                clear_rt = clear_rt.render_partially(
                    viewport.into(),
                    &app.camera.camera,
                    line,
                    &[],
                );
            }
        }
        if show_scene && app.canvas.show_origin {
            clear_rt = clear_rt.render_partially(
                viewport.into(),
                &app.camera.camera,
                &app.canvas.origin_sphere,
                &[],
            );
        }

        if let Some(model) = app.canvas.model.as_ref().filter(|_| show_scene) {
            let lights = app.canvas.model_lights();
            clear_rt = clear_rt.render_partially(
                viewport.into(),
                &app.camera.camera,
                model,
                &lights,
            );
        }

        if show_scene
            && app.page == modules::ui::menu_bar::Page::GlbEditor
            && !app.glb_retarget_preview_active
            && app.canvas.show_glb_skeleton
        {
            if let Some(skeleton) = app.canvas.glb_skeleton.as_ref() {
                clear_rt = clear_rt.render_partially(
                    viewport.into(),
                    &app.camera.camera,
                    skeleton.bone_object(),
                    &[],
                );
                if skeleton.joints_visible() {
                    clear_rt = clear_rt.render_partially(
                        viewport.into(),
                        &app.camera.camera,
                        skeleton.joints_object(),
                        &[],
                    );
                }
                if skeleton.end_sites_visible() {
                    clear_rt = clear_rt.render_partially(
                        viewport.into(),
                        &app.camera.camera,
                        skeleton.end_sites_object(),
                        &[],
                    );
                }
            }
        }

        if show_scene
            && (app.page == modules::ui::menu_bar::Page::BvhStudio
                || app.glb_retarget_preview_active)
        {
            if app.page == modules::ui::menu_bar::Page::BvhStudio
                && app.canvas.show_source_skeleton
            {
                if let Some(skeleton) = app.canvas.skeleton.as_ref() {
                    clear_rt = clear_rt.render_partially(
                        viewport.into(),
                        &app.camera.camera,
                        skeleton.bone_object(),
                        &[],
                    );
                    if skeleton.joints_visible() {
                        clear_rt = clear_rt.render_partially(
                            viewport.into(),
                            &app.camera.camera,
                            skeleton.joints_object(),
                            &[],
                        );
                    }
                    if skeleton.end_sites_visible() {
                        clear_rt = clear_rt.render_partially(
                            viewport.into(),
                            &app.camera.camera,
                            skeleton.end_sites_object(),
                            &[],
                        );
                    }
                }
            }
            if app.canvas.show_target_skeleton {
                if let Some(skeleton) = app.canvas.target_skeleton.as_ref() {
                    clear_rt = clear_rt.render_partially(
                        viewport.into(),
                        &app.camera.camera,
                        skeleton.bone_object(),
                        &[],
                    );
                    if skeleton.joints_visible() {
                        clear_rt = clear_rt.render_partially(
                            viewport.into(),
                            &app.camera.camera,
                            skeleton.joints_object(),
                            &[],
                        );
                    }
                    if skeleton.end_sites_visible() {
                        clear_rt = clear_rt.render_partially(
                            viewport.into(),
                            &app.camera.camera,
                            skeleton.end_sites_object(),
                            &[],
                        );
                    }
                }
            }
        }

        clear_rt.write(|| gui.render()).unwrap();

        FrameOutput {
            exit: app.quit_requested(),
            ..Default::default()
        }
    });
}

fn viewport_from_content_rect(
    rect: three_d::egui::Rect,
    screen: Viewport,
    device_pixel_ratio: f32,
) -> Viewport {
    if screen.width == 0 || screen.height == 0 {
        return Viewport::new_at_origo(1, 1);
    }

    let scale = device_pixel_ratio.max(f32::EPSILON);
    let screen_width = screen.width as i32;
    let screen_height = screen.height as i32;
    let left = (rect.min.x * scale).round() as i32;
    let right = (rect.max.x * scale).round() as i32;
    let top = (rect.min.y * scale).round() as i32;
    let bottom = (rect.max.y * scale).round() as i32;

    let x = left.clamp(0, screen_width.saturating_sub(1));
    let y = (screen_height - bottom).clamp(0, screen_height.saturating_sub(1));
    let width = (right - left).max(1).min((screen_width - x).max(1)) as u32;
    let height = (bottom - top).max(1).min((screen_height - y).max(1)) as u32;

    Viewport {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_reserves_all_ui_regions() {
        let rect = three_d::egui::Rect::from_min_max(
            three_d::egui::pos2(250.0, 32.0),
            three_d::egui::pos2(900.0, 650.0),
        );
        let viewport = viewport_from_content_rect(
            rect,
            Viewport::new_at_origo(1200, 800),
            1.0,
        );

        assert_eq!(
            viewport,
            Viewport {
                x: 250,
                y: 150,
                width: 650,
                height: 618,
            }
        );
        assert_eq!(viewport.x + viewport.width as i32, 900);
    }

    #[test]
    fn viewport_conversion_scales_logical_coordinates_for_high_dpi() {
        let rect = three_d::egui::Rect::from_min_max(
            three_d::egui::pos2(100.0, 24.0),
            three_d::egui::pos2(700.0, 600.0),
        );
        let viewport = viewport_from_content_rect(
            rect,
            Viewport::new_at_origo(1400, 1200),
            2.0,
        );

        assert_eq!(
            viewport,
            Viewport {
                x: 200,
                y: 0,
                width: 1200,
                height: 1152,
            }
        );
    }

    #[test]
    fn viewport_conversion_clamps_content_to_the_screen() {
        let rect = three_d::egui::Rect::from_min_max(
            three_d::egui::pos2(-10.0, -20.0),
            three_d::egui::pos2(1300.0, 900.0),
        );
        let viewport = viewport_from_content_rect(
            rect,
            Viewport::new_at_origo(1200, 800),
            1.0,
        );

        assert_eq!(viewport, Viewport::new_at_origo(1200, 800));
    }
}
