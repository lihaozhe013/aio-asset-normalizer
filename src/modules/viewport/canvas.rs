use super::helpers;
use super::skeleton_visual::{
    SkeletonPose, SkeletonVisual, SkeletonVisualConfig,
};
use crate::modules::glb::{
    AnimationClip, AnimationRuntime, RuntimeNode, RuntimeNodePose,
};
use crate::modules::preferences::ViewPreferences;
use std::path::Path;
use three_d::*;

pub struct ViewportCanvas {
    pub axes: Vec<Gm<Mesh, ColorMaterial>>,
    pub grid: Vec<Gm<Mesh, ColorMaterial>>,
    pub origin_sphere: Gm<Mesh, ColorMaterial>,
    pub skeleton: Option<SkeletonVisual>,
    pub target_skeleton: Option<SkeletonVisual>,
    pub glb_skeleton: Option<SkeletonVisual>,
    pub model: Option<Model<PhysicalMaterial>>,
    glb_animation: Option<AnimationRuntime>,
    glb_skeleton_nodes: Vec<usize>,
    model_base_transforms: Vec<Mat4>,
    animated_node_world: Option<Vec<Mat4>>,
    root_preview_transform: Mat4,
    pub show_axes: bool,
    pub show_grid: bool,
    pub show_origin: bool,
    pub show_source_skeleton: bool,
    pub show_target_skeleton: bool,
    pub show_glb_skeleton: bool,
    pub skeleton_config: SkeletonVisualConfig,
    ambient_light: AmbientLight,
    directional_light: DirectionalLight,
}

impl ViewportCanvas {
    pub fn new(context: &Context) -> Self {
        Self {
            axes: helpers::build_axes(context),
            grid: helpers::build_grid(context),
            origin_sphere: helpers::build_origin_sphere(context),
            skeleton: None,
            target_skeleton: None,
            glb_skeleton: None,
            model: None,
            glb_animation: None,
            glb_skeleton_nodes: Vec::new(),
            model_base_transforms: Vec::new(),
            animated_node_world: None,
            root_preview_transform: Mat4::identity(),
            show_axes: true,
            show_grid: true,
            show_origin: true,
            show_source_skeleton: true,
            show_target_skeleton: true,
            show_glb_skeleton: true,
            skeleton_config: SkeletonVisualConfig::default(),
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

    /// Load a target GLB for BVH or retarget preview without creating the GLB
    /// Editor's skeleton overlay.
    pub fn load_glb_for_target_preview(
        &mut self,
        context: &Context,
        path: &Path,
    ) -> Result<(), String> {
        let runtime =
            AnimationRuntime::load(path).map_err(|error| error.to_string())?;
        self.load_glb_with_runtime_for_target_preview(context, path, runtime)
    }

    pub fn load_glb_with_runtime(
        &mut self,
        context: &Context,
        path: &Path,
        runtime: AnimationRuntime,
    ) -> Result<(), String> {
        self.load_glb_with_runtime_internal(context, path, runtime, true)
    }

    /// Load a target preview from a prepared runtime without creating the GLB
    /// Editor's skeleton overlay.
    pub fn load_glb_with_runtime_for_target_preview(
        &mut self,
        context: &Context,
        path: &Path,
        runtime: AnimationRuntime,
    ) -> Result<(), String> {
        self.load_glb_with_runtime_internal(context, path, runtime, false)
    }

    fn load_glb_with_runtime_internal(
        &mut self,
        context: &Context,
        path: &Path,
        runtime: AnimationRuntime,
        initialize_skeleton: bool,
    ) -> Result<(), String> {
        self.clear_glb();
        let result = self.load_glb_with_runtime_unchecked(
            context,
            path,
            runtime,
            initialize_skeleton,
        );
        if result.is_err() {
            self.clear_glb();
        }
        result
    }

    fn load_glb_with_runtime_unchecked(
        &mut self,
        context: &Context,
        path: &Path,
        runtime: AnimationRuntime,
        initialize_skeleton: bool,
    ) -> Result<(), String> {
        let first_playable =
            runtime.clips.iter().position(AnimationClip::is_playable);
        let renderable_primitive_count = runtime.primitives.len();
        self.glb_animation = Some(runtime);
        if renderable_primitive_count == 0 {
            if initialize_skeleton {
                self.initialize_glb_skeleton(context)?;
            }
            if let Some(index) = first_playable {
                self.update_glb_animation(index, 0.0)?;
            }
            self.show_origin = false;
            return Ok(());
        }

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
        if render_primitive_count != renderable_primitive_count {
            self.model = None;
            return Err(format!(
                "Preview primitive mapping mismatch: renderer has {}, runtime has {}",
                render_primitive_count,
                renderable_primitive_count
            ));
        }
        if initialize_skeleton {
            self.initialize_glb_skeleton(context)?;
        }
        if let Some(index) = first_playable {
            self.update_glb_animation(index, 0.0)?;
        }
        self.show_origin = false;
        Ok(())
    }

    pub fn clear_glb(&mut self) {
        self.model = None;
        self.glb_skeleton = None;
        self.glb_skeleton_nodes.clear();
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

    fn initialize_glb_skeleton(
        &mut self,
        context: &Context,
    ) -> Result<(), String> {
        let (nodes, selected_nodes, rest_poses) = {
            let runtime = self
                .glb_animation
                .as_ref()
                .ok_or_else(|| "GLB skeleton runtime is missing".to_owned())?;
            let nodes = runtime.nodes.clone();
            let selected_nodes = runtime.preview_skeleton_nodes();
            let rest_poses = runtime
                .rest_node_poses()
                .map_err(|error| error.to_string())?;
            (nodes, selected_nodes, rest_poses)
        };
        if selected_nodes.is_empty() {
            self.clear_glb_skeleton();
            return Ok(());
        }
        let rest_positions = rest_poses
            .iter()
            .map(|pose| pose.world_translation)
            .collect::<Vec<_>>();
        let pose = skeleton_pose_from_runtime(
            &nodes,
            &rest_poses,
            &selected_nodes,
            Some(&rest_positions),
        );
        if !pose.is_valid() {
            return Err("GLB skeleton Rest Pose is invalid".to_owned());
        }
        self.glb_skeleton_nodes = selected_nodes;
        let skeleton = self.glb_skeleton.get_or_insert_with(|| {
            SkeletonVisual::new(
                context,
                Srgba::new(255, 190, 80, 255),
                self.skeleton_config,
            )
        });
        skeleton.set_transformation(self.root_preview_transform);
        skeleton.update_pose(&pose);
        Ok(())
    }

    pub fn set_root_preview_transform(
        &mut self,
        transform: Mat4,
    ) -> Result<(), String> {
        self.root_preview_transform = transform;
        if let Some(skeleton) = self.glb_skeleton.as_mut() {
            skeleton.set_transformation(transform);
        }
        if let Some(skeleton) = self.target_skeleton.as_mut() {
            skeleton.set_transformation(transform);
        }
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
        let animated_node_world = pose
            .node_world
            .iter()
            .copied()
            .map(matrix_to_mat4)
            .collect::<Vec<_>>();
        self.animated_node_world = Some(animated_node_world.clone());
        if self.glb_skeleton.is_some() {
            let skeleton_pose = skeleton_pose_from_runtime(
                &runtime.nodes,
                &pose.node_poses,
                &self.glb_skeleton_nodes,
                None,
            );
            if let Some(skeleton) = self.glb_skeleton.as_mut() {
                skeleton.update_pose(&skeleton_pose);
            }
        }
        let Some(model) = self.model.as_mut() else {
            return Ok(());
        };
        if model.len() != runtime.primitives.len() {
            return Err(
                "Animation runtime and renderer primitive counts differ"
                    .to_owned(),
            );
        }
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
        self.set_bvh_skeleton_pose(
            context,
            &SkeletonPose::from_positions(positions.to_vec(), parents.to_vec()),
        );
    }

    pub fn set_bvh_skeleton_pose(
        &mut self,
        context: &Context,
        pose: &SkeletonPose,
    ) {
        let skeleton = self.skeleton.get_or_insert_with(|| {
            SkeletonVisual::new(
                context,
                Srgba::new(255, 150, 40, 255),
                self.skeleton_config,
            )
        });
        skeleton.update_pose(pose);
    }

    pub fn clear_bvh_skeleton(&mut self) {
        self.skeleton = None;
    }

    pub fn clear_glb_skeleton(&mut self) {
        self.glb_skeleton = None;
        self.glb_skeleton_nodes.clear();
    }

    pub fn has_glb_skeleton(&self) -> bool {
        self.glb_skeleton.is_some()
    }

    pub fn is_glb_skeleton_only_preview(&self) -> bool {
        self.glb_skeleton.is_some() && self.model.is_none()
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
        let pose =
            SkeletonPose::from_positions(positions.to_vec(), parents.to_vec());
        self.set_target_skeleton_pose(context, &filter_pose(&pose, joints));
    }

    pub fn set_target_skeleton_pose(
        &mut self,
        context: &Context,
        pose: &SkeletonPose,
    ) {
        let skeleton = self.target_skeleton.get_or_insert_with(|| {
            let mut skeleton = SkeletonVisual::new(
                context,
                Srgba::new(70, 220, 255, 255),
                self.skeleton_config,
            );
            skeleton.set_transformation(self.root_preview_transform);
            skeleton
        });
        skeleton.update_pose(pose);
    }

    pub fn clear_target_skeleton(&mut self) {
        self.target_skeleton = None;
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
        let world_rotations = poses
            .iter()
            .map(|pose| pose.world_rotation)
            .collect::<Vec<_>>();
        let parents = runtime
            .nodes
            .iter()
            .map(|node| node.parent)
            .collect::<Vec<_>>();
        let pose = SkeletonPose {
            positions,
            world_rotations,
            parents,
            end_sites: vec![None; poses.len()],
            rest_positions: None,
        };
        self.set_target_skeleton_pose(context, &filter_pose(&pose, joints));
        Ok(())
    }

    pub fn update_target_skeleton_animation_cached(
        &mut self,
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
        let pose = SkeletonPose {
            positions: poses
                .iter()
                .map(|pose| pose.world_translation)
                .collect(),
            world_rotations: poses
                .iter()
                .map(|pose| pose.world_rotation)
                .collect(),
            parents: runtime.nodes.iter().map(|node| node.parent).collect(),
            end_sites: vec![None; poses.len()],
            rest_positions: None,
        };
        if let Some(skeleton) = self.target_skeleton.as_mut() {
            skeleton.update_pose(&filter_pose(&pose, joints));
        }
        Ok(())
    }

    pub fn set_skeleton_config(&mut self, config: SkeletonVisualConfig) {
        self.skeleton_config = config;
        if let Some(skeleton) = self.skeleton.as_mut() {
            skeleton.set_config(config);
        }
        if let Some(skeleton) = self.target_skeleton.as_mut() {
            skeleton.set_config(config);
        }
        if let Some(skeleton) = self.glb_skeleton.as_mut() {
            skeleton.set_config(config);
        }
    }

    pub fn set_guide_scale(&mut self, context: &Context, height: f32) {
        let height = height.max(0.0001);
        self.axes = helpers::build_axes_scaled(context, height * 1.25);
        self.grid = helpers::build_grid_scaled(context, height * 1.5);
        let origin_radius = (height * 0.03).clamp(0.00001, 100.0);
        self.origin_sphere
            .set_transformation(Mat4::from_scale(origin_radius / 0.1));
    }

    pub fn skeleton_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        self.skeleton.as_ref().and_then(SkeletonVisual::bounds)
    }

    pub fn target_skeleton_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let bounds = self
            .target_skeleton
            .as_ref()
            .and_then(SkeletonVisual::bounds)?;
        Some(transform_bounds(bounds, self.root_preview_transform))
    }

    pub fn glb_skeleton_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let bounds = self
            .glb_skeleton
            .as_ref()
            .and_then(SkeletonVisual::bounds)?;
        Some(transform_bounds(bounds, self.root_preview_transform))
    }

    pub fn model_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let model = self.model.as_ref()?;
        let mut minimum = [f32::INFINITY; 3];
        let mut maximum = [f32::NEG_INFINITY; 3];
        for part in model.iter() {
            let aabb = part.aabb();
            if aabb.is_empty() || aabb.is_infinite() {
                continue;
            }
            let min = aabb.min();
            let max = aabb.max();
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min([min.x, min.y, min.z][axis]);
                maximum[axis] = maximum[axis].max([max.x, max.y, max.z][axis]);
            }
        }
        if minimum.iter().any(|value| !value.is_finite()) {
            None
        } else {
            Some((minimum, maximum))
        }
    }

    pub fn preview_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut bounds = self.skeleton_bounds();
        if let Some(glb_skeleton) = self.glb_skeleton_bounds() {
            bounds = Some(merge_bounds(bounds, glb_skeleton));
        }
        if let Some(target) = self.target_skeleton_bounds() {
            bounds = Some(merge_bounds(bounds, target));
        }
        if let Some(model) = self.model_bounds() {
            bounds = Some(merge_bounds(bounds, model));
        }
        bounds
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

fn filter_pose(pose: &SkeletonPose, joints: &[usize]) -> SkeletonPose {
    if joints.is_empty() {
        return SkeletonPose::default();
    }
    let mut included = vec![false; pose.positions.len()];
    for &joint in joints {
        let mut current = Some(joint);
        let mut guard = 0;
        while let Some(index) = current {
            if index >= included.len() || included[index] {
                break;
            }
            included[index] = true;
            current = pose.parents.get(index).and_then(|parent| *parent);
            guard += 1;
            if guard > included.len() {
                break;
            }
        }
    }
    let mut remap = vec![None; included.len()];
    let mut indices = Vec::new();
    for (index, keep) in included.iter().copied().enumerate() {
        if keep {
            remap[index] = Some(indices.len());
            indices.push(index);
        }
    }
    let positions = indices
        .iter()
        .filter_map(|index| pose.positions.get(*index).copied())
        .collect::<Vec<_>>();
    let world_rotations = indices
        .iter()
        .map(|index| {
            pose.world_rotations
                .get(*index)
                .copied()
                .unwrap_or([0.0, 0.0, 0.0, 1.0])
        })
        .collect::<Vec<_>>();
    let parents = indices
        .iter()
        .map(|index| {
            pose.parents
                .get(*index)
                .and_then(|parent| *parent)
                .and_then(|parent| remap.get(parent).and_then(|value| *value))
        })
        .collect::<Vec<_>>();
    let end_sites = indices
        .iter()
        .map(|index| pose.end_sites.get(*index).copied().unwrap_or(None))
        .collect::<Vec<_>>();
    let rest_positions = pose.rest_positions.as_ref().map(|rest| {
        indices
            .iter()
            .filter_map(|index| rest.get(*index).copied())
            .collect::<Vec<_>>()
    });
    SkeletonPose {
        positions,
        world_rotations,
        parents,
        end_sites,
        rest_positions,
    }
}

fn skeleton_pose_from_runtime(
    nodes: &[RuntimeNode],
    node_poses: &[RuntimeNodePose],
    selected_nodes: &[usize],
    rest_positions: Option<&[[f32; 3]]>,
) -> SkeletonPose {
    let full_pose = SkeletonPose {
        positions: node_poses
            .iter()
            .map(|pose| pose.world_translation)
            .collect(),
        world_rotations: node_poses
            .iter()
            .map(|pose| pose.world_rotation)
            .collect(),
        parents: nodes.iter().map(|node| node.parent).collect(),
        end_sites: vec![None; node_poses.len()],
        rest_positions: rest_positions.map(|positions| positions.to_vec()),
    };
    filter_pose(&full_pose, selected_nodes)
}

fn transform_bounds(
    bounds: ([f32; 3], [f32; 3]),
    transform: Mat4,
) -> ([f32; 3], [f32; 3]) {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for x in [bounds.0[0], bounds.1[0]] {
        for y in [bounds.0[1], bounds.1[1]] {
            for z in [bounds.0[2], bounds.1[2]] {
                let point = transform * vec4(x, y, z, 1.0);
                let point = [point.x, point.y, point.z];
                for axis in 0..3 {
                    minimum[axis] = minimum[axis].min(point[axis]);
                    maximum[axis] = maximum[axis].max(point[axis]);
                }
            }
        }
    }
    (minimum, maximum)
}

fn merge_bounds(
    first: Option<([f32; 3], [f32; 3])>,
    second: ([f32; 3], [f32; 3]),
) -> ([f32; 3], [f32; 3]) {
    let Some((first_minimum, first_maximum)) = first else {
        return second;
    };
    (
        [
            first_minimum[0].min(second.0[0]),
            first_minimum[1].min(second.0[1]),
            first_minimum[2].min(second.0[2]),
        ],
        [
            first_maximum[0].max(second.1[0]),
            first_maximum[1].max(second.1[1]),
            first_maximum[2].max(second.1[2]),
        ],
    )
}

fn matrix_to_mat4(matrix: [f32; 16]) -> Mat4 {
    Mat4::from_cols(
        vec4(matrix[0], matrix[1], matrix[2], matrix[3]),
        vec4(matrix[4], matrix[5], matrix[6], matrix[7]),
        vec4(matrix[8], matrix[9], matrix[10], matrix[11]),
        vec4(matrix[12], matrix[13], matrix[14], matrix[15]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_skeleton_pose_keeps_selected_joint_ancestors() {
        let nodes = vec![
            RuntimeNode {
                parent: None,
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            RuntimeNode {
                parent: Some(0),
                translation: [0.0, 1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            RuntimeNode {
                parent: None,
                translation: [4.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
        ];
        let poses = vec![
            RuntimeNodePose {
                local_translation: [0.0, 0.0, 0.0],
                local_rotation: [0.0, 0.0, 0.0, 1.0],
                local_scale: [1.0, 1.0, 1.0],
                world_translation: [0.0, 0.0, 0.0],
                world_rotation: [0.0, 0.0, 0.0, 1.0],
                world_scale: [1.0, 1.0, 1.0],
            },
            RuntimeNodePose {
                local_translation: [0.0, 1.0, 0.0],
                local_rotation: [0.0, 0.0, 0.0, 1.0],
                local_scale: [1.0, 1.0, 1.0],
                world_translation: [0.0, 1.0, 0.0],
                world_rotation: [0.0, 0.0, 0.0, 1.0],
                world_scale: [1.0, 1.0, 1.0],
            },
            RuntimeNodePose {
                local_translation: [4.0, 0.0, 0.0],
                local_rotation: [0.0, 0.0, 0.0, 1.0],
                local_scale: [1.0, 1.0, 1.0],
                world_translation: [4.0, 0.0, 0.0],
                world_rotation: [0.0, 0.0, 0.0, 1.0],
                world_scale: [1.0, 1.0, 1.0],
            },
        ];
        let rest_positions = poses
            .iter()
            .map(|pose| pose.world_translation)
            .collect::<Vec<_>>();

        let pose = skeleton_pose_from_runtime(
            &nodes,
            &poses,
            &[1],
            Some(&rest_positions),
        );

        assert!(pose.is_valid());
        assert_eq!(pose.positions, vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        assert_eq!(pose.parents, vec![None, Some(0)]);
        assert_eq!(
            pose.rest_positions,
            Some(vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
        );
    }
}
