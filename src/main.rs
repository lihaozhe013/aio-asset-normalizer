mod app;
mod modules;

use app::App;
use three_d::*;

fn main() {
    let window = Window::new(WindowSettings {
        title: "AIO Asset Normalizer".to_string(),
        min_size: (1024, 600),
        max_size: None,
        initial_size: Some((1200, 750)),
        ..Default::default()
    })
    .expect("Failed to create window");

    let context = window.gl();
    let mut gui = three_d::GUI::new(&context);
    let mut app = App::new(&context, window.viewport());

    window.render_loop(move |mut frame_input| {
        app.camera.handle_events(&frame_input.events);

        let mut content_rect: Option<egui::Rect> = None;

        gui.update(
            &mut frame_input.events,
            frame_input.accumulated_time,
            frame_input.viewport,
            frame_input.device_pixel_ratio,
            |gui_ctx| {
                content_rect = Some(app.render_ui(gui_ctx, frame_input.window_width));
            },
        );

        app.reload_model_if_needed(&context);

        let dpr = frame_input.device_pixel_ratio;
        let viewport = content_rect
            .map(|r| Viewport {
                x: (r.min.x * dpr) as i32,
                y: 0,
                width: ((r.width() * dpr) as u32).max(1),
                height: ((r.max.y * dpr) as u32).max(1),
            })
            .unwrap_or(Viewport {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            });

        app.camera.set_viewport(viewport);

        let screen = frame_input.screen();
        let mut clear_rt = screen.clear(ClearState::color_and_depth(0.12, 0.13, 0.17, 1.0, 1.0));

        if app.canvas.show_axes {
            clear_rt = clear_rt.render_partially(
                viewport.into(),
                &app.camera.camera,
                &app.canvas.axes,
                &[],
            );
        }
        if app.canvas.show_grid {
            clear_rt = clear_rt.render_partially(
                viewport.into(),
                &app.camera.camera,
                &app.canvas.grid,
                &[],
            );
        }
        if app.canvas.show_origin {
            clear_rt = clear_rt.render_partially(
                viewport.into(),
                &app.camera.camera,
                &app.canvas.origin_sphere,
                &[],
            );
        }

        if let Some(ref model) = app.canvas.model {
            let lights = app.canvas.model_lights();
            clear_rt = clear_rt.render_partially(
                viewport.into(),
                &app.camera.camera,
                model,
                &lights,
            );
        }

        if app.canvas.show_bones {
            if let Some(ref sticks) = app.canvas.bone_sticks {
                clear_rt = clear_rt.render_partially(
                    viewport.into(),
                    &app.camera.camera,
                    sticks,
                    &[],
                );
            }
            if let Some(ref joints) = app.canvas.bone_joints {
                clear_rt = clear_rt.render_partially(
                    viewport.into(),
                    &app.camera.camera,
                    joints,
                    &[],
                );
            }
        }

        clear_rt.write(|| gui.render()).unwrap();

        FrameOutput {
            exit: app.quit_requested(),
            ..Default::default()
        }
    });
}
