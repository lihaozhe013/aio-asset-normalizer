use super::helpers;
use crate::modules::glb::{AnimationClip, AnimationRuntime};
use crate::modules::preferences::ViewPreferences;
use std::path::Path;
use three_d::*;

pub struct ViewportCanvas {
    pub axes: Vec<Gm<Mesh, ColorMaterial>>,
    pub grid: Vec<Gm<Mesh, ColorMaterial>>,
    pub origin_sphere: Gm<Mesh, ColorMaterial>,
    pub skeleton: Vec<Gm<Mesh, ColorMaterial>>,
    pub model: Option<Model<PhysicalMaterial>>,
    glb_animation: Option<AnimationRuntime>,
    pub show_axes: bool,
    pub show_grid: bool,
    pub show_origin: bool,
    ambient_light: AmbientLight,
    directional_light: DirectionalLight,
}

impl ViewportCanvas {
    pub fn new(context: &Context) -> Self {
        Self {
            axes: helpers::build_axes(context),
            grid: helpers::build_grid(context),
            origin_sphere: helpers::build_origin_sphere(context),
            skeleton: Vec::new(),
            model: None,
            glb_animation: None,
            show_axes: true,
            show_grid: true,
            show_origin: true,
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
        self.clear_glb();
        let mut raw = three_d_asset::io::load(&[path])
            .map_err(|e| format!("Asset load error: {}", e))?;

        let mut cpu_model: CpuModel = raw
            .deserialize(path)
            .map_err(|e| format!("No model found in GLB: {}", e))?;

        for geom in cpu_model.geometries.iter_mut() {
            if let three_d_asset::Geometry::Triangles(ref mut mesh) =
                geom.geometry
            {
                if mesh.tangents.is_none()
                    && mesh.normals.is_some()
                    && mesh.uvs.is_some()
                {
                    mesh.compute_tangents();
                }
            }
        }

        let model =
            Model::new(context, &cpu_model).map_err(|e| e.to_string())?;

        self.model = Some(model);
        let runtime =
            AnimationRuntime::load(path).map_err(|error| error.to_string())?;
        let render_primitive_count = self
            .model
            .as_ref()
            .map(|model| model.len())
            .unwrap_or_default();
        if render_primitive_count != runtime.primitives.len() {
            self.model = None;
            return Err(format!(
                "Preview primitive mapping mismatch: renderer has {}, runtime has {}",
                render_primitive_count,
                runtime.primitives.len()
            ));
        }
        let first_playable =
            runtime.clips.iter().position(AnimationClip::is_playable);
        self.glb_animation = Some(runtime);
        if let Some(index) = first_playable {
            self.update_glb_animation(index, 0.0)?;
        }
        self.show_origin = false;
        Ok(())
    }

    pub fn clear_glb(&mut self) {
        self.model = None;
        self.glb_animation = None;
    }

    pub fn animation_clips(&self) -> &[AnimationClip] {
        self.glb_animation
            .as_ref()
            .map(|runtime| runtime.clips.as_slice())
            .unwrap_or(&[])
    }

    pub fn update_glb_animation(
        &mut self,
        animation_index: usize,
        time: f32,
    ) -> Result<(), String> {
        let Some(runtime) = self.glb_animation.as_ref() else {
            return Ok(());
        };
        let pose = runtime
            .sample(animation_index, time)
            .map_err(|error| error.to_string())?;
        let Some(model) = self.model.as_mut() else {
            return Ok(());
        };
        if model.len() != runtime.primitives.len() {
            return Err(
                "Animation runtime and renderer primitive counts differ"
                    .to_owned(),
            );
        }
        for (part, (primitive, positions, normals, tangents)) in
            model.iter_mut().zip(
                runtime
                    .primitives
                    .iter()
                    .zip(pose.skinned_positions.iter())
                    .zip(pose.skinned_normals.iter())
                    .zip(pose.skinned_tangents.iter())
                    .map(|(((primitive, positions), normals), tangents)| {
                        (primitive, positions, normals, tangents)
                    }),
            )
        {
            let node_world =
                pose.node_world.get(primitive.node).ok_or_else(|| {
                    "Animation references a missing node".to_owned()
                })?;
            part.set_transformation(matrix_to_mat4(*node_world));
            // Indexed meshes expose the element count as `vertex_count`; partial
            // writes update the original vertex buffer without duplicating data.
            if let Some(positions) = positions {
                let values = positions
                    .iter()
                    .map(|value| vec3(value[0], value[1], value[2]))
                    .collect::<Vec<_>>();
                part.set_positions_partially(0, &values)
                    .map_err(|error| error.to_string())?;
            }
            if let Some(normals) = normals {
                let values = normals
                    .iter()
                    .map(|value| vec3(value[0], value[1], value[2]))
                    .collect::<Vec<_>>();
                part.set_normals_partially(0, &values)
                    .map_err(|error| error.to_string())?;
            }
            if let Some(tangents) = tangents {
                let values = tangents
                    .iter()
                    .map(|value| vec4(value[0], value[1], value[2], value[3]))
                    .collect::<Vec<_>>();
                part.set_tangents_partially(0, &values)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    pub fn model_lights(&self) -> [&dyn Light; 2] {
        [&self.ambient_light, &self.directional_light]
    }

    pub fn set_bvh_skeleton(
        &mut self,
        context: &Context,
        positions: &[[f32; 3]],
        parents: &[Option<usize>],
    ) {
        self.skeleton = helpers::build_skeleton(context, positions, parents);
    }

    pub fn clear_bvh_skeleton(&mut self) {
        self.skeleton.clear();
    }

    pub fn apply_view_prefs(&mut self, prefs: &ViewPreferences) {
        self.show_grid = prefs.show_grid;
        self.show_axes = prefs.show_axes;
        self.show_origin = prefs.show_origin;
    }

    pub fn to_view_prefs(&self) -> ViewPreferences {
        ViewPreferences {
            show_grid: self.show_grid,
            show_axes: self.show_axes,
            show_origin: self.show_origin,
        }
    }
}

fn matrix_to_mat4(matrix: [f32; 16]) -> Mat4 {
    Mat4::from_cols(
        vec4(matrix[0], matrix[1], matrix[2], matrix[3]),
        vec4(matrix[4], matrix[5], matrix[6], matrix[7]),
        vec4(matrix[8], matrix[9], matrix[10], matrix[11]),
        vec4(matrix[12], matrix[13], matrix[14], matrix[15]),
    )
}
