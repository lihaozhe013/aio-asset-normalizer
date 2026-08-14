use three_d::*;

fn build_axis_quad(
    context: &Context,
    from: Vec3,
    to: Vec3,
    thickness: f32,
    color: Srgba,
) -> Gm<Mesh, ColorMaterial> {
    let direction = to - from;
    if direction.magnitude() <= f32::EPSILON {
        return build_origin_sphere(context);
    }
    let dir = direction.normalize();
    let up = if dir.y.abs() < 0.99 {
        vec3(0.0, 1.0, 0.0)
    } else {
        vec3(1.0, 0.0, 0.0)
    };
    let right = dir.cross(up).normalize() * thickness;
    let positions = Positions::F32(vec![
        from - right,
        from + right,
        to + right,
        from - right,
        to + right,
        to - right,
    ]);
    let colors = vec![color; 6];
    let cpu_mesh = CpuMesh {
        positions,
        colors: Some(colors),
        ..Default::default()
    };
    Gm::new(Mesh::new(context, &cpu_mesh), ColorMaterial::default())
}

pub fn build_skeleton(
    context: &Context,
    positions: &[[f32; 3]],
    parents: &[Option<usize>],
) -> Vec<Gm<Mesh, ColorMaterial>> {
    let color = Srgba::new(255, 190, 70, 255);
    positions
        .iter()
        .enumerate()
        .filter_map(|(index, position)| {
            let parent = parents.get(index).and_then(|parent| *parent)?;
            let parent_position = positions.get(parent)?;
            Some(build_line_quad(
                context,
                vec3(
                    parent_position[0],
                    parent_position[1],
                    parent_position[2],
                ),
                vec3(position[0], position[1], position[2]),
                0.025,
                color,
            ))
        })
        .collect()
}

pub fn build_axes(context: &Context) -> Vec<Gm<Mesh, ColorMaterial>> {
    let len = 3.0;
    let t = 0.015;
    vec![
        build_axis_quad(
            context,
            vec3(0.0, 0.0, 0.0),
            vec3(len, 0.0, 0.0),
            t,
            Srgba::new(255, 60, 60, 255),
        ),
        build_axis_quad(
            context,
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, len, 0.0),
            t,
            Srgba::new(60, 255, 60, 255),
        ),
        build_axis_quad(
            context,
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 0.0, len),
            t,
            Srgba::new(60, 80, 255, 255),
        ),
    ]
}

fn build_line_quad(
    context: &Context,
    from: Vec3,
    to: Vec3,
    thickness: f32,
    color: Srgba,
) -> Gm<Mesh, ColorMaterial> {
    let direction = to - from;
    if direction.magnitude() <= f32::EPSILON {
        return build_origin_sphere(context);
    }
    let dir = direction.normalize();
    let up = if dir.y.abs() < 0.99 {
        vec3(0.0, 1.0, 0.0)
    } else {
        vec3(1.0, 0.0, 0.0)
    };
    let right = dir.cross(up).normalize() * thickness;
    let positions = Positions::F32(vec![
        from - right,
        from + right,
        to + right,
        from - right,
        to + right,
        to - right,
    ]);
    let colors = vec![color; 6];
    let cpu_mesh = CpuMesh {
        positions,
        colors: Some(colors),
        ..Default::default()
    };
    Gm::new(Mesh::new(context, &cpu_mesh), ColorMaterial::default())
}

pub fn build_grid(context: &Context) -> Vec<Gm<Mesh, ColorMaterial>> {
    let size = 10;
    let step = 1.0;
    let t = 0.008;
    let major_color = Srgba::new(100, 100, 100, 255);
    let minor_color = Srgba::new(60, 60, 60, 255);
    let mut lines = Vec::new();

    for i in -size..=size {
        let p = i as f32 * step;
        let is_major = i % 5 == 0;
        let c = if is_major { major_color } else { minor_color };
        let s = size as f32 * step;

        lines.push(build_line_quad(
            context,
            vec3(p, 0.0, -s),
            vec3(p, 0.0, s),
            if is_major { t * 1.5 } else { t },
            c,
        ));
        lines.push(build_line_quad(
            context,
            vec3(-s, 0.0, p),
            vec3(s, 0.0, p),
            if is_major { t * 1.5 } else { t },
            c,
        ));
    }

    lines
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
