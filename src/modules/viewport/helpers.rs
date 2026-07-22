use three_d::*;

pub fn build_axes(context: &Context) -> Gm<Mesh, ColorMaterial> {
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

pub fn build_grid(context: &Context) -> Gm<Mesh, ColorMaterial> {
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

pub fn build_origin_sphere(context: &Context) -> Gm<Mesh, ColorMaterial> {
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
