mod app;
mod modules;

use app::{App, PanelLayout};
use three_d::*;

fn main() {
    let window = Window::new(WindowSettings {
        title: "AIO Asset Normalizer".to_string(),
        min_size: (1024, 600),
        max_size: Some((2560, 1440)),
        ..Default::default()
    })
    .expect("Failed to create window");

    let context = window.gl();
    let mut gui = three_d::GUI::new(&context);
    let mut app = App::new(&context, window.viewport());

    window.render_loop(move |mut frame_input| {
        app.camera.handle_events(&frame_input.events);

        let mut layout = PanelLayout::default();

        gui.update(
            &mut frame_input.events,
            frame_input.accumulated_time,
            frame_input.viewport,
            frame_input.device_pixel_ratio,
            |gui_ctx| {
                layout = app.render_ui(gui_ctx, frame_input.window_width);
            },
        );

        app.reload_model_if_needed(&context);

        let viewport = app.compute_viewport(
            &layout,
            frame_input.device_pixel_ratio,
            &frame_input.viewport,
        );

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
            clear_rt = clear_rt.render_partially(viewport.into(), &app.camera.camera, model, &lights);
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
