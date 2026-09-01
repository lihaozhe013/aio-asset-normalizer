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

pub fn build_axes(context: &Context) -> Vec<Gm<Mesh, ColorMaterial>> {
    build_axes_scaled(context, 3.0)
}

pub fn build_axes_scaled(
    context: &Context,
    len: f32,
) -> Vec<Gm<Mesh, ColorMaterial>> {
    let len = len.max(0.0001);
    let t = (len * 0.005).clamp(0.00001, len * 0.04);
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
        return build_colored_sphere(
            context,
            color,
            (thickness * 3.0).max(0.002),
        );
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
    build_grid_scaled(context, 10.0)
}

pub fn build_grid_scaled(
    context: &Context,
    height: f32,
) -> Vec<Gm<Mesh, ColorMaterial>> {
    let extent = height.max(0.0001);
    let step = nice_grid_step(extent / 6.0);
    let size = (extent / step).ceil() as i32;
    let t = (step * 0.012).clamp(0.000005, extent * 0.02);
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

fn nice_grid_step(value: f32) -> f32 {
    if !value.is_finite() || value <= f32::EPSILON {
        return 1.0;
    }
    let exponent = value.log10().floor();
    let base = 10.0_f32.powf(exponent);
    let fraction = value / base;
    let nice = if fraction <= 1.0 {
        1.0
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * base
}

pub fn build_origin_sphere(context: &Context) -> Gm<Mesh, ColorMaterial> {
    build_colored_sphere(context, Srgba::new(255, 255, 255, 255), 0.1)
}

fn build_colored_sphere(
    context: &Context,
    color: Srgba,
    radius: f32,
) -> Gm<Mesh, ColorMaterial> {
    let mut sphere = CpuMesh::sphere(8);
    sphere.transform(Mat4::from_scale(radius)).unwrap();
    Gm::new(
        Mesh::new(context, &sphere),
        ColorMaterial {
            color,
            ..Default::default()
        },
    )
}
