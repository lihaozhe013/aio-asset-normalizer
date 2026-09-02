use three_d::*;

/// The available debug representations for a skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkeletonDisplayMode {
    /// Blender-like tapered bones with a visible volume.
    Octahedral,
    /// Constant-radius cylindrical bones.
    Stick,
    /// Thin cylindrical bones intended for topology debugging.
    Lines,
}

/// A sampled, world-space skeleton pose shared by BVH and GLB previews.
#[derive(Debug, Clone, Default)]
pub struct SkeletonPose {
    /// World-space joint positions.
    pub positions: Vec<[f32; 3]>,
    /// World-space joint rotations in XYZW order.
    pub world_rotations: Vec<[f32; 4]>,
    /// Parent index for every joint.
    pub parents: Vec<Option<usize>>,
    /// Optional world-space End Site positions, indexed by joint.
    pub end_sites: Vec<Option<[f32; 3]>>,
    /// Optional authored Rest Pose positions. When omitted, the first pose is
    /// used as the display calibration pose.
    pub rest_positions: Option<Vec<[f32; 3]>>,
}

impl SkeletonPose {
    pub fn from_positions(
        positions: Vec<[f32; 3]>,
        parents: Vec<Option<usize>>,
    ) -> Self {
        let world_rotations = vec![[0.0, 0.0, 0.0, 1.0]; positions.len()];
        let end_sites = vec![None; positions.len()];
        Self {
            rest_positions: Some(positions.clone()),
            positions,
            world_rotations,
            parents,
            end_sites,
        }
    }

    pub fn with_rest_positions(
        positions: Vec<[f32; 3]>,
        world_rotations: Vec<[f32; 4]>,
        parents: Vec<Option<usize>>,
        end_sites: Vec<Option<[f32; 3]>>,
        rest_positions: Vec<[f32; 3]>,
    ) -> Self {
        Self {
            positions,
            world_rotations,
            parents,
            end_sites,
            rest_positions: Some(rest_positions),
        }
    }

    pub fn is_valid(&self) -> bool {
        let count = self.positions.len();
        self.parents.len() == count
            && self.world_rotations.len() == count
            && self.end_sites.len() == count
            && self
                .positions
                .iter()
                .all(|point| point.iter().all(|value| value.is_finite()))
            && self
                .world_rotations
                .iter()
                .all(|rotation| rotation.iter().all(|value| value.is_finite()))
            && self.parents.iter().enumerate().all(|(index, parent)| {
                parent
                    .map(|parent| parent < count && parent != index)
                    .unwrap_or(true)
            })
            && self.end_sites.iter().all(|end_site| {
                end_site
                    .map(|point| point.iter().all(|value| value.is_finite()))
                    .unwrap_or(true)
            })
            && self
                .rest_positions
                .as_ref()
                .map(|positions| {
                    positions.len() == count
                        && positions.iter().all(|point| {
                            point.iter().all(|value| value.is_finite())
                        })
                })
                .unwrap_or(true)
    }
}

/// User-facing controls for the shared skeleton renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonVisualConfig {
    pub mode: SkeletonDisplayMode,
    pub show_joints: bool,
    pub show_end_sites: bool,
    pub in_front: bool,
    pub width_scale: f32,
}

impl Default for SkeletonVisualConfig {
    fn default() -> Self {
        Self {
            mode: SkeletonDisplayMode::Octahedral,
            show_joints: true,
            show_end_sites: true,
            in_front: false,
            width_scale: 1.0,
        }
    }
}

/// Stable dimensions derived from an authored Rest Pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonMetrics {
    pub minimum: [f32; 3],
    pub maximum: [f32; 3],
    pub height: f32,
    pub diagonal: f32,
    pub median_bone_length: f32,
    pub joint_radius: f32,
}

impl SkeletonMetrics {
    pub fn from_positions(
        positions: &[[f32; 3]],
        parents: &[Option<usize>],
    ) -> Self {
        let mut minimum = [f32::INFINITY; 3];
        let mut maximum = [f32::NEG_INFINITY; 3];
        let mut lengths = Vec::new();
        for point in positions {
            if point.iter().any(|value| !value.is_finite()) {
                continue;
            }
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(point[axis]);
                maximum[axis] = maximum[axis].max(point[axis]);
            }
        }
        for (index, point) in positions.iter().enumerate() {
            let Some(parent) = parents.get(index).and_then(|value| *value)
            else {
                continue;
            };
            let Some(parent_point) = positions.get(parent) else {
                continue;
            };
            let length = distance(*point, *parent_point);
            if length > f32::EPSILON && length.is_finite() {
                lengths.push(length);
            }
        }
        lengths.sort_by(f32::total_cmp);
        let median_bone_length = if lengths.is_empty() {
            0.0
        } else {
            lengths[lengths.len() / 2]
        };
        if minimum.iter().any(|value| !value.is_finite()) {
            minimum = [0.0; 3];
            maximum = [0.0; 3];
        }
        let size = [
            (maximum[0] - minimum[0]).max(0.0),
            (maximum[1] - minimum[1]).max(0.0),
            (maximum[2] - minimum[2]).max(0.0),
        ];
        let diagonal = length(size);
        let height = size[1].max(diagonal * 0.57735);
        let scale_reference = if median_bone_length > f32::EPSILON {
            median_bone_length
        } else {
            diagonal.max(0.001)
        };
        let joint_radius = (scale_reference * 0.11)
            .max(diagonal.max(0.001) * 0.006)
            .clamp(0.00001, 10_000.0);
        Self {
            minimum,
            maximum,
            height: height.max(0.00001),
            diagonal: diagonal.max(0.00001),
            median_bone_length,
            joint_radius,
        }
    }

    pub fn bone_radius(&self, bone_length: f32, width_scale: f32) -> f32 {
        let reference = if bone_length > f32::EPSILON {
            bone_length
        } else {
            self.median_bone_length.max(self.diagonal * 0.01)
        };
        (reference * 0.07 * width_scale.max(0.05))
            .max(self.diagonal * 0.0015)
            .min(self.diagonal * 0.18)
            .max(0.000001)
    }
}

/// GPU-instanced skeleton debug geometry. The topology is updated by replacing
/// instance buffers; the base meshes are uploaded once and reused every frame.
pub struct SkeletonVisual {
    octahedral: Gm<InstancedMesh, ColorMaterial>,
    stick: Gm<InstancedMesh, ColorMaterial>,
    lines: Gm<InstancedMesh, ColorMaterial>,
    joints: Gm<InstancedMesh, ColorMaterial>,
    end_sites: Gm<InstancedMesh, ColorMaterial>,
    parents: Vec<Option<usize>>,
    metrics: SkeletonMetrics,
    config: SkeletonVisualConfig,
    calibration_positions: Vec<[f32; 3]>,
    bounds_minimum: [f32; 3],
    bounds_maximum: [f32; 3],
    has_pose: bool,
    last_pose: Option<SkeletonPose>,
}

impl SkeletonVisual {
    pub fn new(
        context: &Context,
        color: Srgba,
        config: SkeletonVisualConfig,
    ) -> Self {
        let config = sanitize_config(config);
        let empty = Instances::default();
        let mut octahedral = Gm::new(
            InstancedMesh::new(context, &empty, &octahedral_mesh(color)),
            ColorMaterial {
                color,
                ..Default::default()
            },
        );
        let mut stick = Gm::new(
            InstancedMesh::new(context, &empty, &stick_mesh()),
            ColorMaterial {
                color,
                ..Default::default()
            },
        );
        let mut lines = Gm::new(
            InstancedMesh::new(context, &empty, &stick_mesh()),
            ColorMaterial {
                color,
                ..Default::default()
            },
        );
        let mut joints = Gm::new(
            InstancedMesh::new(context, &empty, &sphere_mesh()),
            ColorMaterial {
                color,
                ..Default::default()
            },
        );
        let mut end_sites = Gm::new(
            InstancedMesh::new(context, &empty, &sphere_mesh()),
            ColorMaterial {
                color,
                ..Default::default()
            },
        );
        apply_render_states(
            &mut octahedral,
            &mut stick,
            &mut lines,
            &mut joints,
            &mut end_sites,
            config.in_front,
        );
        Self {
            octahedral,
            stick,
            lines,
            joints,
            end_sites,
            parents: Vec::new(),
            metrics: SkeletonMetrics::from_positions(&[], &[]),
            config,
            calibration_positions: Vec::new(),
            bounds_minimum: [0.0; 3],
            bounds_maximum: [0.0; 3],
            has_pose: false,
            last_pose: None,
        }
    }

    pub fn set_config(&mut self, config: SkeletonVisualConfig) {
        let config = sanitize_config(config);
        self.config = config;
        apply_render_states(
            &mut self.octahedral,
            &mut self.stick,
            &mut self.lines,
            &mut self.joints,
            &mut self.end_sites,
            config.in_front,
        );
        if let Some(pose) = self.last_pose.clone() {
            self.update_pose(&pose);
        }
    }

    pub fn metrics(&self) -> SkeletonMetrics {
        self.metrics
    }

    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        self.has_pose
            .then_some((self.bounds_minimum, self.bounds_maximum))
    }

    pub fn set_transformation(&mut self, transformation: Mat4) {
        self.octahedral.set_transformation(transformation);
        self.stick.set_transformation(transformation);
        self.lines.set_transformation(transformation);
        self.joints.set_transformation(transformation);
        self.end_sites.set_transformation(transformation);
    }

    pub fn update_pose(&mut self, pose: &SkeletonPose) {
        if !pose.is_valid() {
            return;
        }
        let rest_positions = pose
            .rest_positions
            .as_deref()
            .filter(|positions| positions.len() == pose.positions.len())
            .unwrap_or(&pose.positions);
        let rest_changed = pose.rest_positions.is_some()
            && self.calibration_positions != rest_positions;
        if !self.has_pose
            || self.parents != pose.parents
            || rest_changed
            || self.metrics.diagonal <= f32::EPSILON
        {
            self.parents = pose.parents.clone();
            self.calibration_positions = rest_positions.to_vec();
            self.metrics =
                SkeletonMetrics::from_positions(rest_positions, &pose.parents);
        }
        let mut bone_instances = Vec::new();
        let mut octahedral_instances = Vec::new();
        let mut line_instances = Vec::new();
        let mut joint_transforms = Vec::new();
        let mut end_site_instances = Vec::new();
        let mut minimum = [f32::INFINITY; 3];
        let mut maximum = [f32::NEG_INFINITY; 3];
        for (index, point) in pose.positions.iter().enumerate() {
            expand_bounds(&mut minimum, &mut maximum, *point);
            let depth = joint_depth(index, &pose.parents);
            let has_child =
                pose.parents.iter().any(|parent| *parent == Some(index));
            let size_factor = if depth == 0 {
                1.35
            } else if !has_child {
                0.82
            } else {
                1.0
            };
            let radius = self.metrics.joint_radius * size_factor;
            joint_transforms.push(
                Mat4::from_translation(point_to_vec(*point))
                    * Mat4::from_scale(radius),
            );
            if let Some(parent) =
                pose.parents.get(index).and_then(|value| *value)
            {
                let Some(parent_position) = pose.positions.get(parent) else {
                    continue;
                };
                let direction = subtract(*point, *parent_position);
                let bone_length = length(direction);
                if bone_length > f32::EPSILON && bone_length.is_finite() {
                    let rest_bone_length = self
                        .calibration_positions
                        .get(index)
                        .zip(self.calibration_positions.get(parent))
                        .map(|(point, parent)| distance(*point, *parent))
                        .filter(|length| length.is_finite())
                        .unwrap_or(bone_length);
                    let radius = self
                        .metrics
                        .bone_radius(rest_bone_length, self.config.width_scale);
                    let origin =
                        Mat4::from_translation(point_to_vec(*parent_position));
                    let rotation = rotation_from_y(direction);
                    bone_instances.push(
                        origin
                            * rotation
                            * Mat4::from_nonuniform_scale(
                                radius,
                                bone_length,
                                radius,
                            ),
                    );
                    octahedral_instances.push(
                        origin
                            * rotation
                            * Mat4::from_nonuniform_scale(
                                radius / 0.18,
                                bone_length,
                                radius / 0.18,
                            ),
                    );
                    line_instances.push(
                        origin
                            * rotation
                            * Mat4::from_nonuniform_scale(
                                radius * 0.28,
                                bone_length,
                                radius * 0.28,
                            ),
                    );
                }
            }
            if let Some(Some(end_site)) = pose.end_sites.get(index) {
                if end_site.iter().all(|value| value.is_finite()) {
                    expand_bounds(&mut minimum, &mut maximum, *end_site);
                    end_site_instances.push(
                        Mat4::from_translation(point_to_vec(*end_site))
                            * Mat4::from_scale(
                                self.metrics.joint_radius * 0.62,
                            ),
                    );
                }
            }
        }
        if minimum.iter().any(|value| !value.is_finite()) {
            minimum = [0.0; 3];
            maximum = [0.0; 3];
        }
        let expanded = self.metrics.joint_radius * 1.5;
        self.bounds_minimum = [
            minimum[0] - expanded,
            minimum[1] - expanded,
            minimum[2] - expanded,
        ];
        self.bounds_maximum = [
            maximum[0] + expanded,
            maximum[1] + expanded,
            maximum[2] + expanded,
        ];
        let joints = Instances {
            transformations: joint_transforms,
            ..Default::default()
        };
        let octahedral = Instances {
            transformations: octahedral_instances,
            ..Default::default()
        };
        let bones = Instances {
            transformations: bone_instances.clone(),
            ..Default::default()
        };
        let ends = Instances {
            transformations: end_site_instances,
            ..Default::default()
        };
        self.octahedral.set_instances(&octahedral);
        self.stick.set_instances(&bones);
        self.lines.set_instances(&Instances {
            transformations: line_instances,
            ..Default::default()
        });
        self.joints.set_instances(&joints);
        self.end_sites.set_instances(&ends);
        self.has_pose = true;
        self.last_pose = Some(pose.clone());
    }

    pub fn bone_object(&self) -> &Gm<InstancedMesh, ColorMaterial> {
        match self.config.mode {
            SkeletonDisplayMode::Octahedral => &self.octahedral,
            SkeletonDisplayMode::Stick => &self.stick,
            SkeletonDisplayMode::Lines => &self.lines,
        }
    }

    pub fn joints_object(&self) -> &Gm<InstancedMesh, ColorMaterial> {
        &self.joints
    }

    pub fn end_sites_object(&self) -> &Gm<InstancedMesh, ColorMaterial> {
        &self.end_sites
    }

    pub fn joints_visible(&self) -> bool {
        self.config.show_joints && self.has_pose
    }

    pub fn end_sites_visible(&self) -> bool {
        self.config.show_end_sites && self.has_pose
    }
}

fn sanitize_config(mut config: SkeletonVisualConfig) -> SkeletonVisualConfig {
    if !config.width_scale.is_finite() {
        config.width_scale = 1.0;
    }
    config.width_scale = config.width_scale.clamp(0.05, 8.0);
    config
}

fn apply_render_states(
    octahedral: &mut Gm<InstancedMesh, ColorMaterial>,
    stick: &mut Gm<InstancedMesh, ColorMaterial>,
    lines: &mut Gm<InstancedMesh, ColorMaterial>,
    joints: &mut Gm<InstancedMesh, ColorMaterial>,
    end_sites: &mut Gm<InstancedMesh, ColorMaterial>,
    in_front: bool,
) {
    let states = if in_front {
        RenderStates {
            depth_test: DepthTest::Always,
            write_mask: WriteMask::COLOR,
            cull: Cull::None,
            ..Default::default()
        }
    } else {
        RenderStates::default()
    };
    octahedral.material.render_states = states;
    stick.material.render_states = states;
    lines.material.render_states = states;
    joints.material.render_states = states;
    end_sites.material.render_states = states;
}

fn octahedral_mesh(color: Srgba) -> CpuMesh {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let root = vec3(0.0, 0.0, 0.0);
    let tip = vec3(0.0, 1.0, 0.0);
    let mut ring = Vec::new();
    for index in 0..4 {
        let angle = index as f32 * std::f32::consts::FRAC_PI_2;
        ring.push(vec3(angle.cos() * 0.18, 0.52, angle.sin() * 0.18));
    }
    for index in 0..4 {
        let next = (index + 1) % 4;
        push_triangle(
            &mut positions,
            &mut colors,
            root,
            ring[index],
            ring[next],
            shade(color, index % 2 == 0),
        );
        push_triangle(
            &mut positions,
            &mut colors,
            tip,
            ring[next],
            ring[index],
            shade(color, index % 2 != 0),
        );
    }
    let mut mesh = CpuMesh {
        positions: Positions::F32(positions),
        colors: Some(colors),
        ..Default::default()
    };
    mesh.compute_normals();
    mesh
}

fn stick_mesh() -> CpuMesh {
    let mut mesh = CpuMesh::cylinder(8);
    let _ = mesh.transform(Mat4::from_angle_z(degrees(90.0)));
    mesh
}

fn sphere_mesh() -> CpuMesh {
    CpuMesh::sphere(8)
}

fn push_triangle(
    positions: &mut Vec<Vec3>,
    colors: &mut Vec<Srgba>,
    a: Vec3,
    b: Vec3,
    c: Vec3,
    color: Srgba,
) {
    positions.extend([a, b, c]);
    colors.extend([color; 3]);
}

fn shade(color: Srgba, light: bool) -> Srgba {
    let value = if light { 255 } else { 190 };
    Srgba::new(value, value, value, color.a)
}

fn point_to_vec(point: [f32; 3]) -> Vec3 {
    vec3(point[0], point[1], point[2])
}

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    length(subtract(a, b))
}

fn length(value: [f32; 3]) -> f32 {
    (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt()
}

fn expand_bounds(
    minimum: &mut [f32; 3],
    maximum: &mut [f32; 3],
    point: [f32; 3],
) {
    if point.iter().any(|value| !value.is_finite()) {
        return;
    }
    for axis in 0..3 {
        minimum[axis] = minimum[axis].min(point[axis]);
        maximum[axis] = maximum[axis].max(point[axis]);
    }
}

fn joint_depth(index: usize, parents: &[Option<usize>]) -> usize {
    let mut depth = 0;
    let mut current = index;
    let mut guard = 0;
    while let Some(parent) = parents.get(current).and_then(|value| *value) {
        depth += 1;
        current = parent;
        guard += 1;
        if guard > parents.len() {
            break;
        }
    }
    depth
}

fn rotation_from_y(direction: [f32; 3]) -> Mat4 {
    let target = point_to_vec(direction);
    let magnitude = target.magnitude();
    if magnitude <= f32::EPSILON || !magnitude.is_finite() {
        return Mat4::identity();
    }
    let y = target / magnitude;
    let reference = if y.y.abs() < 0.999 {
        vec3(0.0, 1.0, 0.0)
    } else {
        vec3(1.0, 0.0, 0.0)
    };
    let x = reference.cross(y).normalize();
    let z = x.cross(y).normalize();
    Mat4::from_cols(
        x.extend(0.0),
        y.extend(0.0),
        z.extend(0.0),
        vec4(0.0, 0.0, 0.0, 1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octahedral_template_has_volume() {
        let mesh = octahedral_mesh(Srgba::new(255, 160, 40, 255));
        let Positions::F32(positions) = mesh.positions else {
            panic!("octahedral mesh must use f32 positions")
        };
        let min_z = positions
            .iter()
            .map(|value| value.z)
            .fold(f32::INFINITY, f32::min);
        let max_z = positions
            .iter()
            .map(|value| value.z)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(max_z - min_z > 0.1);
        assert!(mesh.normals.is_some());
    }

    #[test]
    fn metrics_use_bone_lengths_and_are_stable() {
        let parents = vec![None, Some(0), Some(1)];
        let positions = vec![[0.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 2.2, 0.0]];
        let metrics = SkeletonMetrics::from_positions(&positions, &parents);
        assert!(metrics.bone_radius(2.0, 1.0) > metrics.bone_radius(0.2, 1.0));
        let moved = vec![[0.0, 10.0, 0.0], [0.0, 12.0, 0.0], [0.0, 12.2, 0.0]];
        let rest_metrics =
            SkeletonMetrics::from_positions(&positions, &parents);
        let frame_metrics = SkeletonMetrics::from_positions(&moved, &parents);
        assert_eq!(rest_metrics, metrics);
        assert_eq!(frame_metrics.joint_radius, metrics.joint_radius);
    }

    #[test]
    fn zero_length_and_parallel_directions_are_finite() {
        let zero = rotation_from_y([0.0, 0.0, 0.0]);
        let parallel = rotation_from_y([0.0, 1.0, 0.0]);
        for matrix in [zero, parallel] {
            for value in [
                matrix.x.x, matrix.x.y, matrix.x.z, matrix.x.w, matrix.y.x,
                matrix.y.y, matrix.y.z, matrix.y.w, matrix.z.x, matrix.z.y,
                matrix.z.z, matrix.z.w, matrix.w.x, matrix.w.y, matrix.w.z,
                matrix.w.w,
            ] {
                assert!(value.is_finite());
            }
        }
    }

    #[test]
    fn pose_validation_rejects_invalid_topology_and_values() {
        let mut pose = SkeletonPose::from_positions(
            vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![None, Some(0)],
        );
        assert!(pose.is_valid());
        pose.parents[1] = Some(4);
        assert!(!pose.is_valid());
        pose.parents[1] = Some(0);
        pose.positions[0][0] = f32::NAN;
        assert!(!pose.is_valid());
    }
}
