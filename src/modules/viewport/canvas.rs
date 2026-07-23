use super::helpers;
use crate::modules::preferences::ViewPreferences;
use std::path::Path;
use three_d::*;

pub struct ViewportCanvas {
    pub axes: Gm<Mesh, ColorMaterial>,
    pub grid: Gm<Mesh, ColorMaterial>,
    pub origin_sphere: Gm<Mesh, ColorMaterial>,
    pub model: Option<Model<PhysicalMaterial>>,
    pub show_axes: bool,
    pub show_grid: bool,
    pub show_origin: bool,
    pub show_bones: bool,
    pub bone_sticks: Option<Gm<Mesh, ColorMaterial>>,
    pub bone_joints: Option<Gm<Mesh, ColorMaterial>>,
    ambient_light: AmbientLight,
    directional_light: DirectionalLight,
}

impl ViewportCanvas {
    pub fn new(context: &Context) -> Self {
        Self {
            axes: helpers::build_axes(context),
            grid: helpers::build_grid(context),
            origin_sphere: helpers::build_origin_sphere(context),
            model: None,
            show_axes: true,
            show_grid: true,
            show_origin: true,
            show_bones: true,
            bone_sticks: None,
            bone_joints: None,
            ambient_light: AmbientLight {
                intensity: 0.3,
                color: Srgba::new(255, 255, 255, 255),
                environment: None,
            },
            directional_light: DirectionalLight::new(
                context,
                0.8,
                Srgba::new(255, 255, 255, 255),
                vec3(-1.0, -2.0, -1.0),
            ),
        }
    }

    pub fn load_glb(
        &mut self,
        context: &Context,
        path: &Path,
    ) -> Result<(), String> {
        let mut raw = three_d_asset::io::load(&[path])
            .map_err(|e| format!("Asset load error: {}", e))?;

        let cpu_model: CpuModel = raw
            .deserialize("Scene")
            .or_else(|_| raw.deserialize("scene"))
            .map_err(|e| format!("No model found in GLB: {}", e))?;

        let model =
            Model::new(context, &cpu_model).map_err(|e| e.to_string())?;

        self.model = Some(model);
        self.bone_sticks = None;
        self.bone_joints = None;
        Ok(())
    }

    pub fn update_bones(
        &mut self,
        context: &Context,
        bone_segments: &[(Vec3, Vec3)],
        joint_positions: &[Vec3],
        highlighted: Option<usize>,
    ) {
        if bone_segments.is_empty() {
            self.bone_sticks = None;
            self.bone_joints = None;
            return;
        }

        self.bone_sticks =
            Some(build_bone_sticks(context, bone_segments, highlighted));
        self.bone_joints = Some(build_joint_spheres(context, joint_positions));
    }

    pub fn model_lights(&self) -> [&dyn Light; 2] {
        [&self.ambient_light, &self.directional_light]
    }

    pub fn apply_view_prefs(&mut self, prefs: &ViewPreferences) {
        self.show_grid = prefs.show_grid;
        self.show_axes = prefs.show_axes;
        self.show_origin = prefs.show_origin;
        self.show_bones = prefs.show_bones;
    }

    pub fn to_view_prefs(&self) -> ViewPreferences {
        ViewPreferences {
            show_grid: self.show_grid,
            show_axes: self.show_axes,
            show_origin: self.show_origin,
            show_bones: self.show_bones,
        }
    }
}

fn build_bone_sticks(
    context: &Context,
    segments: &[(Vec3, Vec3)],
    _highlighted: Option<usize>,
) -> Gm<Mesh, ColorMaterial> {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let bone_color = Srgba::new(255, 200, 60, 255);

    for (start, end) in segments {
        positions.push(*start);
        positions.push(*end);
        colors.push(bone_color);
        colors.push(bone_color);
    }

    let cpu_mesh = CpuMesh {
        positions: Positions::F32(positions),
        colors: Some(colors),
        ..Default::default()
    };
    Gm::new(Mesh::new(context, &cpu_mesh), ColorMaterial::default())
}

fn build_joint_spheres(
    context: &Context,
    positions: &[Vec3],
) -> Gm<Mesh, ColorMaterial> {
    let joint_radius = 0.03;
    let mut sphere_template = CpuMesh::sphere(6);
    sphere_template
        .transform(Mat4::from_scale(joint_radius))
        .unwrap();

    let sphere_vertex_count = match &sphere_template.positions {
        Positions::F32(v) => v.len(),
        Positions::F64(v) => v.len(),
    };

    let sphere_indices: Vec<u32> = match &sphere_template.indices {
        Indices::U8(idxs) => idxs.iter().map(|&i| i as u32).collect(),
        Indices::U16(idxs) => idxs.iter().map(|&i| i as u32).collect(),
        Indices::U32(idxs) => idxs.clone(),
        Indices::None => (0..sphere_vertex_count as u32).collect(),
    };

    let mut all_positions = Vec::new();
    let mut all_colors = Vec::new();

    for (i, &pos) in positions.iter().enumerate() {
        let color = if i == 0 {
            Srgba::new(255, 60, 60, 255)
        } else {
            Srgba::new(60, 220, 255, 255)
        };
        match &sphere_template.positions {
            Positions::F32(verts) => {
                for v in verts {
                    all_positions.push(*v + pos);
                    all_colors.push(color);
                }
            }
            Positions::F64(verts) => {
                for v in verts {
                    all_positions
                        .push(vec3(v.x as f32, v.y as f32, v.z as f32) + pos);
                    all_colors.push(color);
                }
            }
        }
    }

    let mut all_indices =
        Vec::with_capacity(sphere_indices.len() * positions.len());
    for i in 0..positions.len() {
        let offset = (i * sphere_vertex_count) as u32;
        for idx in &sphere_indices {
            all_indices.push(offset + idx);
        }
    }

    let cpu_mesh = CpuMesh {
        positions: Positions::F32(all_positions),
        colors: Some(all_colors),
        indices: Indices::U32(all_indices),
        ..Default::default()
    };

    Gm::new(Mesh::new(context, &cpu_mesh), ColorMaterial::default())
}
