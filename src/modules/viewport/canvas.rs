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
    pub target_skeleton: Vec<Gm<Mesh, ColorMaterial>>,
    pub model: Option<Model<PhysicalMaterial>>,
    glb_animation: Option<AnimationRuntime>,
    model_base_transforms: Vec<Mat4>,
    animated_node_world: Option<Vec<Mat4>>,
    root_preview_transform: Mat4,
    pub show_axes: bool,
    pub show_grid: bool,
    pub show_origin: bool,
    pub show_source_skeleton: bool,
    pub show_target_skeleton: bool,
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
            target_skeleton: Vec::new(),
            model: None,
            glb_animation: None,
            model_base_transforms: Vec::new(),
            animated_node_world: None,
            root_preview_transform: Mat4::identity(),
            show_axes: true,
            show_grid: true,
            show_origin: true,
            show_source_skeleton: true,
            show_target_skeleton: true,
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
        let runtime =
            AnimationRuntime::load(path).map_err(|error| error.to_string())?;
        self.load_glb_with_runtime(context, path, runtime)
    }

    pub fn load_glb_with_runtime(
        &mut self,
        context: &Context,
        path: &Path,
        runtime: AnimationRuntime,
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

        self.model_base_transforms =
            model.iter().map(|part| part.transformation()).collect();
        self.model = Some(model);
        self.animated_node_world = None;
        self.root_preview_transform = Mat4::identity();
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
        self.model_base_transforms.clear();
        self.animated_node_world = None;
        self.root_preview_transform = Mat4::identity();
    }

    /// Keep a skeleton-only runtime for compressed targets that the CPU
    /// renderer cannot decode.  Node animation and overlays remain available
    /// while the original Mesh resources are left untouched for export.
    pub fn load_skeleton_runtime(&mut self, runtime: AnimationRuntime) {
        self.clear_glb();
        self.glb_animation = Some(runtime);
        self.show_origin = false;
    }

    pub fn set_root_preview_transform(
        &mut self,
        transform: Mat4,
    ) -> Result<(), String> {
        self.root_preview_transform = transform;
        self.apply_model_transforms()
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
        let animated_node_world = pose
            .node_world
            .iter()
            .copied()
            .map(matrix_to_mat4)
            .collect::<Vec<_>>();
        self.animated_node_world = Some(animated_node_world.clone());
        let root_preview_transform = self.root_preview_transform;
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
                animated_node_world.get(primitive.node).ok_or_else(|| {
                    "Animation references a missing node".to_owned()
                })?;
            part.set_transformation(root_preview_transform * *node_world);
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

    fn apply_model_transforms(&mut self) -> Result<(), String> {
        let transforms = if let Some(animated_node_world) =
            self.animated_node_world.as_ref()
        {
            let runtime = self.glb_animation.as_ref().ok_or_else(|| {
                "Animated preview runtime is missing".to_owned()
            })?;
            runtime
                .primitives
                .iter()
                .map(|primitive| {
                    animated_node_world.get(primitive.node).copied().ok_or_else(
                        || "Animation references a missing node".to_owned(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            self.model_base_transforms.clone()
        };
        let Some(model) = self.model.as_mut() else {
            return Ok(());
        };
        if model.len() != transforms.len() {
            return Err(
                "Preview transform and renderer primitive counts differ"
                    .to_owned(),
            );
        }
        for (part, transform) in model.iter_mut().zip(transforms) {
            part.set_transformation(self.root_preview_transform * transform);
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

    pub fn animation_runtime(&self) -> Option<AnimationRuntime> {
        self.glb_animation.clone()
    }

    pub fn set_target_skeleton_filtered(
        &mut self,
        context: &Context,
        positions: &[[f32; 3]],
        parents: &[Option<usize>],
        joints: &[usize],
    ) {
        self.target_skeleton = helpers::build_skeleton_colored_filtered(
            context,
            positions,
            parents,
            joints,
            Srgba::new(70, 220, 255, 255),
        );
    }

    pub fn clear_target_skeleton(&mut self) {
        self.target_skeleton.clear();
    }

    pub fn update_target_skeleton_animation(
        &mut self,
        context: &Context,
        animation_index: usize,
        time: f32,
        joints: &[usize],
    ) -> Result<(), String> {
        let Some(runtime) = self.glb_animation.as_ref() else {
            return Ok(());
        };
        let poses = runtime
            .sample_nodes(animation_index, time)
            .map_err(|error| error.to_string())?;
        let positions = poses
            .iter()
            .map(|pose| pose.world_translation)
            .collect::<Vec<_>>();
        let parents = runtime
            .nodes
            .iter()
            .map(|node| node.parent)
            .collect::<Vec<_>>();
        self.target_skeleton = helpers::build_skeleton_colored_filtered(
            context,
            &positions,
            &parents,
            joints,
            Srgba::new(70, 220, 255, 255),
        );
        Ok(())
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
