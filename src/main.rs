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

    let mut camera = Camera::new_perspective(
        window.viewport(),
        vec3(4.0, 3.0, 6.0),
        vec3(0.0, 0.5, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        100.0,
    );

    let axes = build_axes(&context);
    let grid = build_grid(&context);
    let origin_sphere = build_origin_sphere(&context);

    let mut gui = three_d::GUI::new(&context);

    window.render_loop(move |mut frame_input| {
        let mut panel_width = 0.0;

        gui.update(
            &mut frame_input.events,
            frame_input.accumulated_time,
            frame_input.viewport,
            frame_input.device_pixel_ratio,
            |gui_ctx| {
                use three_d::egui::*;
                Panel::left("control_panel")
                    .resizable(true)
                    .default_size(300.0)
                    .min_size(200.0)
                    .show_inside(gui_ctx, |ui| {
                        ScrollArea::vertical().show(ui, |ui| {
                            ui.heading("控制面板");
                            ui.separator();

                            ui.collapsing("资产导入", |ui| {
                                ui.label("拖拽模型文件到此处...");
                            });

                            ui.collapsing("转换配置", |ui| {
                                ui.label("目标单位比例: 1.0");
                                ui.label("目标朝向: Y-Up / Z-Forward");
                                ui.label("清理策略: 清除无用材质");
                            });

                            ui.collapsing("日志输出", |ui| {
                                ui.label("就绪，等待任务...");
                            });
                        });
                    });
                panel_width = frame_input.window_width as f32 - gui_ctx.available_width();
            },
        );

        let viewport = Viewport {
            x: (panel_width * frame_input.device_pixel_ratio) as i32,
            y: 0,
            width: frame_input.viewport.width
                - (panel_width * frame_input.device_pixel_ratio) as u32,
            height: frame_input.viewport.height,
        };

        camera.set_viewport(viewport);

        frame_input
            .screen()
            .clear(ClearState::color_and_depth(0.12, 0.13, 0.17, 1.0, 1.0))
            .render_partially(viewport.into(), &camera, &axes, &[])
            .render_partially(viewport.into(), &camera, &grid, &[])
            .render_partially(viewport.into(), &camera, &origin_sphere, &[])
            .write(|| gui.render())
            .unwrap();

        FrameOutput::default()
    });
}

fn build_axes(context: &Context) -> Gm<Mesh, ColorMaterial> {
    let axis_len = 3.0;
    let positions = Positions::F32(vec![
        vec3(0.0, 0.0, 0.0),
        vec3(axis_len, 0.0, 0.0),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, axis_len, 0.0),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 0.0, axis_len),
    ]);
    let colors = vec![
        Srgba::new(255, 60, 60, 255),
        Srgba::new(255, 60, 60, 255),
        Srgba::new(60, 255, 60, 255),
        Srgba::new(60, 255, 60, 255),
        Srgba::new(60, 80, 255, 255),
        Srgba::new(60, 80, 255, 255),
    ];
    let cpu_mesh = CpuMesh {
        positions,
        colors: Some(colors),
        ..Default::default()
    };
    Gm::new(Mesh::new(context, &cpu_mesh), ColorMaterial::default())
}

fn build_grid(context: &Context) -> Gm<Mesh, ColorMaterial> {
    let size = 10;
    let step = 1.0;
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let major_color = Srgba::new(100, 100, 100, 255);
    let minor_color = Srgba::new(60, 60, 60, 255);

    for i in -size..=size {
        let p = i as f32 * step;
        let is_major = i % 5 == 0;
        let c = if is_major { major_color } else { minor_color };

        positions.push(vec3(p, 0.0, -(size as f32 * step)));
        positions.push(vec3(p, 0.0, size as f32 * step));
        colors.push(c);
        colors.push(c);

        positions.push(vec3(-(size as f32 * step), 0.0, p));
        positions.push(vec3(size as f32 * step, 0.0, p));
        colors.push(c);
        colors.push(c);
    }

    let cpu_mesh = CpuMesh {
        positions: Positions::F32(positions),
        colors: Some(colors),
        ..Default::default()
    };
    Gm::new(Mesh::new(context, &cpu_mesh), ColorMaterial::default())
}

fn build_origin_sphere(context: &Context) -> Gm<Mesh, ColorMaterial> {
    let mut sphere = CpuMesh::sphere(8);
    sphere.transform(Mat4::from_scale(0.1)).unwrap();
    Gm::new(
        Mesh::new(context, &sphere),
        ColorMaterial {
            color: Srgba::new(255, 255, 255, 255),
            ..Default::default()
        },
    )
}
