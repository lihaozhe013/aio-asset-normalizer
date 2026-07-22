mod app;
mod modules;

use three_d::*;
use app::App;

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

        let mut panel_width = 0.0;

        gui.update(
            &mut frame_input.events,
            frame_input.accumulated_time,
            frame_input.viewport,
            frame_input.device_pixel_ratio,
            |gui_ctx| {
                panel_width = app.render_ui(gui_ctx, frame_input.window_width);
            },
        );

        let viewport = app.compute_viewport(
            panel_width,
            frame_input.device_pixel_ratio,
            &frame_input.viewport,
        );

        app.camera.set_viewport(viewport);

        frame_input
            .screen()
            .clear(ClearState::color_and_depth(0.12, 0.13, 0.17, 1.0, 1.0))
            .render_partially(viewport.into(), &app.camera.camera, &app.canvas.axes, &[])
            .render_partially(viewport.into(), &app.camera.camera, &app.canvas.grid, &[])
            .render_partially(
                viewport.into(),
                &app.camera.camera,
                &app.canvas.origin_sphere,
                &[],
            )
            .write(|| gui.render())
            .unwrap();

        FrameOutput::default()
    });
}
