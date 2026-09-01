//! Runtime sampling and CPU skinning for GLB animation preview.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::Path;

use gltf::animation::util::ReadOutputs;
use gltf::animation::Interpolation as GltfInterpolation;
use gltf::mesh::Mode;

pub type Matrix4 = [f32; 16];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPath {
    Translation,
    Rotation,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Step,
    Linear,
}

#[derive(Debug, Clone)]
pub struct AnimationCurve {
    pub path: AnimationPath,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
    pub interpolation: Interpolation,
}

impl AnimationCurve {
    fn sample(&self, time: f32) -> [f32; 4] {
        if self.times.len() == 1 || time <= self.times[0] {
            return self.values[0];
        }
        let last = self.times.len() - 1;
        if time >= self.times[last] {
            return self.values[last];
        }
        let next = self
            .times
            .partition_point(|key_time| *key_time <= time)
            .min(last);
        let previous = next.saturating_sub(1);
        if self.interpolation == Interpolation::Step {
            return self.values[previous];
        }
        let range = self.times[next] - self.times[previous];
        let amount = if range > f32::EPSILON {
            (time - self.times[previous]) / range
        } else {
            0.0
        };
        match self.path {
            AnimationPath::Rotation => {
                slerp(self.values[previous], self.values[next], amount)
            }
            AnimationPath::Translation | AnimationPath::Scale => {
                lerp(self.values[previous], self.values[next], amount)
            }
        }
    }
}

fn validate_curve(curve: &AnimationCurve) -> Result<(), RuntimeError> {
    if curve.times.len() != curve.values.len()
        || curve.times.is_empty()
        || curve.times.iter().any(|time| !time.is_finite())
        || curve.times.windows(2).any(|pair| pair[0] >= pair[1])
        || curve.values.iter().any(|value| {
            !finite_vec4(*value)
                || (curve.path == AnimationPath::Rotation
                    && quaternion_length(*value) <= f32::EPSILON)
        })
    {
        return Err(RuntimeError::Invalid(
            "Animation curve contains invalid keyframes".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct AnimationChannel {
    pub node: usize,
    pub curve: AnimationCurve,
}

#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
    pub unsupported: Vec<String>,
}

impl AnimationClip {
    pub fn is_playable(&self) -> bool {
        self.unsupported.is_empty()
            && !self.channels.is_empty()
            && self.duration.is_finite()
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeNode {
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeNodePose {
    pub local_translation: [f32; 3],
    pub local_rotation: [f32; 4],
    pub local_scale: [f32; 3],
    pub world_translation: [f32; 3],
    pub world_rotation: [f32; 4],
    pub world_scale: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct RuntimeSkin {
    pub joints: Vec<usize>,
    pub inverse_bind_matrices: Vec<Matrix4>,
}

#[derive(Debug, Clone)]
pub struct RuntimePrimitive {
    pub node: usize,
    pub skin: Option<usize>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub tangents: Option<Vec<[f32; 4]>>,
    pub joints: Option<Vec<[u16; 4]>>,
    pub weights: Option<Vec<[f32; 4]>>,
}

#[derive(Debug, Clone)]
pub struct RuntimePose {
    pub node_world: Vec<Matrix4>,
    pub skinned_positions: Vec<Option<Vec<[f32; 3]>>>,
    pub skinned_normals: Vec<Option<Vec<[f32; 3]>>>,
    pub skinned_tangents: Vec<Option<Vec<[f32; 4]>>>,
}

#[derive(Debug, Clone)]
pub struct AnimationRuntime {
    pub nodes: Vec<RuntimeNode>,
    pub skins: Vec<RuntimeSkin>,
    pub primitives: Vec<RuntimePrimitive>,
    pub clips: Vec<AnimationClip>,
}

#[derive(Debug)]
pub enum RuntimeError {
    Io(std::io::Error),
    Invalid(String),
    Unsupported(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Invalid(message) => {
                write!(f, "Invalid animation runtime data: {message}")
            }
            Self::Unsupported(message) => {
                write!(f, "Unsupported animation runtime data: {message}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<std::io::Error> for RuntimeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<gltf::Error> for RuntimeError {
    fn from(error: gltf::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}

impl AnimationRuntime {
    pub fn load(path: &Path) -> Result<Self, RuntimeError> {
        let bytes = fs::read(path)?;
        Self::from_bytes(&bytes, path.parent())
    }

    /// Build a runtime from an in-memory GLB.  Retarget previews use this to
    /// inspect a generated animation without writing a temporary user asset.
    pub fn from_bytes(
        bytes: &[u8],
        base_path: Option<&Path>,
    ) -> Result<Self, RuntimeError> {
        Self::from_bytes_internal(bytes, base_path, true)
    }

    /// Parse nodes, Skins, and animations without decoding Mesh primitives.
    /// This keeps GLB→GLB retargeting available for compressed geometry that
    /// cannot be previewed by the CPU renderer.
    pub fn from_bytes_skeleton_only(
        bytes: &[u8],
        base_path: Option<&Path>,
    ) -> Result<Self, RuntimeError> {
        Self::from_bytes_internal(bytes, base_path, false)
    }

    fn from_bytes_internal(
        bytes: &[u8],
        base_path: Option<&Path>,
        decode_primitives: bool,
    ) -> Result<Self, RuntimeError> {
        let source = gltf::Gltf::from_slice(&bytes)?;
        let (document, blob) = (source.document, source.blob);
        let buffers = gltf::import_buffers(&document, base_path, blob)?;
        let get_buffer_data = |buffer: gltf::Buffer<'_>| -> Option<&[u8]> {
            buffers.get(buffer.index()).map(|data| data.0.as_slice())
        };

        let mut parents = vec![None; document.nodes().count()];
        for node in document.nodes() {
            for child in node.children() {
                if parents[child.index()].replace(node.index()).is_some() {
                    return Err(RuntimeError::Invalid(format!(
                        "Node {} has more than one parent",
                        child.index()
                    )));
                }
            }
        }

        let nodes = document
            .nodes()
            .map(|node| {
                let (translation, rotation, scale) =
                    node.transform().decomposed();
                if !finite_vec3(translation)
                    || !finite_vec4(rotation)
                    || !finite_vec3(scale)
                {
                    return Err(RuntimeError::Invalid(format!(
                        "Node {} has non-finite TRS values",
                        node.index()
                    )));
                }
                Ok(RuntimeNode {
                    parent: parents[node.index()],
                    translation,
                    rotation: normalize_quaternion(rotation),
                    scale,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let skins = document
            .skins()
            .map(|skin| {
                let joints = skin.joints().map(|joint| joint.index()).collect::<Vec<_>>();
                let mut unique_joints = HashSet::new();
                if joints.iter().any(|joint| {
                    *joint >= nodes.len() || !unique_joints.insert(*joint)
                }) {
                    return Err(RuntimeError::Invalid(format!(
                        "Skin {} contains an invalid or duplicate joint",
                        skin.index()
                    )));
                }
                let inverse_bind_matrices = if skin.inverse_bind_matrices().is_some() {
                    skin.reader(get_buffer_data)
                        .read_inverse_bind_matrices()
                        .ok_or_else(|| {
                            RuntimeError::Invalid(format!(
                                "Skin {} inverse bind matrices are unreadable",
                                skin.index()
                            ))
                        })?
                        .map(matrix_from_columns)
                        .collect::<Vec<_>>()
                } else {
                    vec![identity(); joints.len()]
                };
                if inverse_bind_matrices.len() != joints.len() {
                    return Err(RuntimeError::Invalid(format!(
                        "Skin {} inverse bind matrix count does not match joint count",
                        skin.index()
                    )));
                }
                if inverse_bind_matrices
                    .iter()
                    .any(|matrix| matrix.iter().any(|value| !value.is_finite()))
                {
                    return Err(RuntimeError::Invalid(format!(
                        "Skin {} contains non-finite inverse bind matrices",
                        skin.index()
                    )));
                }
                Ok(RuntimeSkin {
                    joints,
                    inverse_bind_matrices,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut primitives = Vec::new();
        let scene = document.scenes().next().ok_or_else(|| {
            RuntimeError::Invalid("GLB has no scenes".to_owned())
        })?;
        for node in scene.nodes() {
            if decode_primitives {
                collect_primitives(node, &get_buffer_data, &mut primitives)?;
            }
        }

        let clips = document
            .animations()
            .map(|animation| parse_animation(animation, &get_buffer_data))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            nodes,
            skins,
            primitives,
            clips,
        })
    }

    /// Sample only node transforms.  This is intentionally independent from
    /// CPU Mesh decoding so a compressed target can still show a skeleton and
    /// participate in a validated animation retarget export.
    pub fn sample_nodes(
        &self,
        clip_index: usize,
        time: f32,
    ) -> Result<Vec<RuntimeNodePose>, RuntimeError> {
        let clip = self.clips.get(clip_index).ok_or_else(|| {
            RuntimeError::Invalid(format!(
                "Animation {clip_index} does not exist"
            ))
        })?;
        if !clip.is_playable() {
            return Err(RuntimeError::Unsupported(clip.unsupported.join(", ")));
        }
        if !time.is_finite() {
            return Err(RuntimeError::Invalid(
                "Animation sample time must be finite".to_owned(),
            ));
        }
        let sample_time = if clip.duration > f32::EPSILON {
            time.clamp(0.0, clip.duration)
        } else {
            0.0
        };
        let mut translations = self
            .nodes
            .iter()
            .map(|node| node.translation)
            .collect::<Vec<_>>();
        let mut rotations = self
            .nodes
            .iter()
            .map(|node| node.rotation)
            .collect::<Vec<_>>();
        let mut scales =
            self.nodes.iter().map(|node| node.scale).collect::<Vec<_>>();
        for channel in &clip.channels {
            if channel.node >= self.nodes.len() {
                return Err(RuntimeError::Invalid(format!(
                    "Animation channel references missing node {}",
                    channel.node
                )));
            }
            validate_curve(&channel.curve)?;
            let value = channel.curve.sample(sample_time);
            if !finite_vec4(value) {
                return Err(RuntimeError::Invalid(
                    "Animation sample contains non-finite values".to_owned(),
                ));
            }
            match channel.curve.path {
                AnimationPath::Translation => {
                    translations[channel.node] = [value[0], value[1], value[2]]
                }
                AnimationPath::Rotation => {
                    rotations[channel.node] = normalize_quaternion(value)
                }
                AnimationPath::Scale => {
                    scales[channel.node] = [value[0], value[1], value[2]]
                }
            }
        }
        let locals = (0..self.nodes.len())
            .map(|index| RuntimeNodePose {
                local_translation: translations[index],
                local_rotation: normalize_quaternion(rotations[index]),
                local_scale: scales[index],
                world_translation: [0.0; 3],
                world_rotation: identity_rotation(),
                world_scale: [1.0; 3],
            })
            .collect::<Vec<_>>();
        if locals.iter().any(|pose| {
            !finite_vec3(pose.local_translation)
                || !finite_vec4(pose.local_rotation)
                || !finite_vec3(pose.local_scale)
        }) {
            return Err(RuntimeError::Invalid(
                "Animation sample contains non-finite node transforms"
                    .to_owned(),
            ));
        }
        let mut sampled = vec![None; self.nodes.len()];
        let mut visiting = vec![false; self.nodes.len()];
        for index in 0..self.nodes.len() {
            resolve_node_pose(
                index,
                &self.nodes,
                &locals,
                &mut sampled,
                &mut visiting,
            )?;
        }
        sampled
            .into_iter()
            .map(|pose| {
                pose.ok_or_else(|| {
                    RuntimeError::Invalid(
                        "node pose resolution did not produce a value"
                            .to_owned(),
                    )
                })
            })
            .collect()
    }

    pub fn keyframe_times(
        &self,
        clip_index: usize,
    ) -> Result<Vec<f32>, RuntimeError> {
        let clip = self.clips.get(clip_index).ok_or_else(|| {
            RuntimeError::Invalid(format!(
                "Animation {clip_index} does not exist"
            ))
        })?;
        for channel in &clip.channels {
            if channel.node >= self.nodes.len() {
                return Err(RuntimeError::Invalid(format!(
                    "Animation channel references missing node {}",
                    channel.node
                )));
            }
            validate_curve(&channel.curve)?;
        }
        let mut times = clip
            .channels
            .iter()
            .flat_map(|channel| channel.curve.times.iter().copied())
            .collect::<Vec<_>>();
        times.sort_by(|left, right| left.total_cmp(right));
        times.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);
        Ok(times)
    }

    pub fn sample(
        &self,
        clip_index: usize,
        time: f32,
    ) -> Result<RuntimePose, RuntimeError> {
        let clip = self.clips.get(clip_index).ok_or_else(|| {
            RuntimeError::Invalid(format!(
                "Animation {clip_index} does not exist"
            ))
        })?;
        if !clip.is_playable() {
            return Err(RuntimeError::Unsupported(clip.unsupported.join(", ")));
        }
        if !time.is_finite() {
            return Err(RuntimeError::Invalid(
                "Animation sample time must be finite".to_owned(),
            ));
        }
        let sample_time = if clip.duration > f32::EPSILON {
            time.clamp(0.0, clip.duration)
        } else {
            0.0
        };
        let mut translations = self
            .nodes
            .iter()
            .map(|node| node.translation)
            .collect::<Vec<_>>();
        let mut rotations = self
            .nodes
            .iter()
            .map(|node| node.rotation)
            .collect::<Vec<_>>();
        let mut scales =
            self.nodes.iter().map(|node| node.scale).collect::<Vec<_>>();
        for channel in &clip.channels {
            if channel.node >= self.nodes.len() {
                return Err(RuntimeError::Invalid(format!(
                    "Animation channel references missing node {}",
                    channel.node
                )));
            }
            validate_curve(&channel.curve)?;
            let value = channel.curve.sample(sample_time);
            match channel.curve.path {
                AnimationPath::Translation => {
                    translations[channel.node] = [value[0], value[1], value[2]]
                }
                AnimationPath::Rotation => {
                    rotations[channel.node] = normalize_quaternion(value)
                }
                AnimationPath::Scale => {
                    scales[channel.node] = [value[0], value[1], value[2]]
                }
            }
        }

        let local = translations
            .iter()
            .zip(rotations.iter())
            .zip(scales.iter())
            .map(|((translation, rotation), scale)| {
                compose(*translation, *rotation, *scale)
            })
            .collect::<Vec<_>>();
        let mut node_world = vec![identity(); self.nodes.len()];
        let mut visiting = vec![false; self.nodes.len()];
        for index in 0..self.nodes.len() {
            compute_world(
                index,
                &self.nodes,
                &local,
                &mut node_world,
                &mut visiting,
            )?;
        }

        let skin_matrices = self
            .skins
            .iter()
            .map(|skin| {
                skin.joints
                    .iter()
                    .zip(skin.inverse_bind_matrices.iter())
                    .map(|(joint, inverse_bind)| {
                        multiply(node_world[*joint], *inverse_bind)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut skinned_positions = Vec::with_capacity(self.primitives.len());
        let mut skinned_normals = Vec::with_capacity(self.primitives.len());
        let mut skinned_tangents = Vec::with_capacity(self.primitives.len());
        for primitive in &self.primitives {
            let Some(skin_index) = primitive.skin else {
                skinned_positions.push(None);
                skinned_normals.push(None);
                skinned_tangents.push(None);
                continue;
            };
            let matrices = skin_matrices.get(skin_index).ok_or_else(|| {
                RuntimeError::Invalid(format!(
                    "Primitive references missing Skin {skin_index}"
                ))
            })?;
            let joints = primitive.joints.as_ref().ok_or_else(|| {
                RuntimeError::Invalid(
                    "Skinned primitive is missing JOINTS_0".to_owned(),
                )
            })?;
            let weights = primitive.weights.as_ref().ok_or_else(|| {
                RuntimeError::Invalid(
                    "Skinned primitive is missing WEIGHTS_0".to_owned(),
                )
            })?;
            if joints.len() != primitive.positions.len()
                || weights.len() != primitive.positions.len()
            {
                return Err(RuntimeError::Invalid(
                    "Skinned vertex attribute counts do not match POSITION"
                        .to_owned(),
                ));
            }
            let mut positions = Vec::with_capacity(primitive.positions.len());
            let mut normals = primitive
                .normals
                .as_ref()
                .map(|values| Vec::with_capacity(values.len()));
            let mut tangents = primitive
                .tangents
                .as_ref()
                .map(|values| Vec::with_capacity(values.len()));
            for (index, position) in primitive.positions.iter().enumerate() {
                let (skinned_position, total_weight) = blend_point(
                    *position,
                    joints[index],
                    weights[index],
                    matrices,
                )?;
                positions.push(if total_weight > f32::EPSILON {
                    skinned_position
                } else {
                    *position
                });
                if let (Some(source_normals), Some(output_normals)) =
                    (&primitive.normals, &mut normals)
                {
                    let (normal, total_weight) = blend_vector(
                        source_normals[index],
                        joints[index],
                        weights[index],
                        matrices,
                    )?;
                    output_normals.push(if total_weight > f32::EPSILON {
                        normalize_vec3(normal)
                    } else {
                        normalize_vec3(source_normals[index])
                    });
                }
                if let (Some(source_tangents), Some(output_tangents)) =
                    (&primitive.tangents, &mut tangents)
                {
                    let tangent = source_tangents[index];
                    let (value, total_weight) = blend_vector(
                        [tangent[0], tangent[1], tangent[2]],
                        joints[index],
                        weights[index],
                        matrices,
                    )?;
                    let value = if total_weight > f32::EPSILON {
                        normalize_vec3(value)
                    } else {
                        normalize_vec3([tangent[0], tangent[1], tangent[2]])
                    };
                    output_tangents
                        .push([value[0], value[1], value[2], tangent[3]]);
                }
            }
            skinned_positions.push(Some(positions));
            skinned_normals.push(normals);
            skinned_tangents.push(tangents);
        }

        Ok(RuntimePose {
            node_world,
            skinned_positions,
            skinned_normals,
            skinned_tangents,
        })
    }
}

fn collect_primitives<'a, F>(
    node: gltf::Node<'a>,
    get_buffer_data: &F,
    output: &mut Vec<RuntimePrimitive>,
) -> Result<(), RuntimeError>
where
    F: Clone + for<'b> Fn(gltf::Buffer<'b>) -> Option<&'a [u8]>,
{
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            if primitive.mode() != Mode::Triangles {
                return Err(RuntimeError::Unsupported(format!(
                    "Primitive {} uses {:?}; only TRIANGLES are supported",
                    primitive.index(),
                    primitive.mode()
                )));
            }
            let reader = primitive.reader(get_buffer_data.clone());
            let Some(position_reader) = reader.read_positions() else {
                continue;
            };
            let positions = position_reader
                .map(|value| [value[0], value[1], value[2]])
                .collect::<Vec<_>>();
            if positions.iter().any(|value| !finite_vec3(*value)) {
                return Err(RuntimeError::Invalid(format!(
                    "Primitive {} contains non-finite positions",
                    primitive.index()
                )));
            }
            let normals = reader.read_normals().map(|values| {
                values
                    .map(|value| [value[0], value[1], value[2]])
                    .collect::<Vec<_>>()
            });
            if normals
                .as_ref()
                .is_some_and(|values| values.len() != positions.len())
            {
                return Err(RuntimeError::Invalid(
                    "NORMAL count does not match POSITION".to_owned(),
                ));
            }
            let tangents = reader.read_tangents().map(|values| {
                values
                    .map(|value| [value[0], value[1], value[2], value[3]])
                    .collect::<Vec<_>>()
            });
            if tangents
                .as_ref()
                .is_some_and(|values| values.len() != positions.len())
            {
                return Err(RuntimeError::Invalid(
                    "TANGENT count does not match POSITION".to_owned(),
                ));
            }
            if tangents.as_ref().is_some_and(|values| {
                values.iter().any(|value| !finite_vec4(*value))
            }) {
                return Err(RuntimeError::Invalid(
                    "TANGENT contains non-finite values".to_owned(),
                ));
            }
            let joints = reader
                .read_joints(0)
                .map(|values| values.into_u16().collect::<Vec<_>>());
            let weights = reader
                .read_weights(0)
                .map(|values| values.into_f32().collect::<Vec<_>>());
            if joints.is_some() != weights.is_some() {
                return Err(RuntimeError::Invalid(
                    "JOINTS_0 and WEIGHTS_0 must be provided together"
                        .to_owned(),
                ));
            }
            if joints
                .as_ref()
                .is_some_and(|values| values.len() != positions.len())
                || weights
                    .as_ref()
                    .is_some_and(|values| values.len() != positions.len())
            {
                return Err(RuntimeError::Invalid(
                    "JOINTS_0/WEIGHTS_0 count does not match POSITION"
                        .to_owned(),
                ));
            }
            if weights.as_ref().is_some_and(|values| {
                values.iter().any(|value| {
                    !finite_vec4(*value)
                        || value.iter().any(|weight| *weight < 0.0)
                })
            }) {
                return Err(RuntimeError::Invalid(
                    "WEIGHTS_0 contains invalid values".to_owned(),
                ));
            }
            output.push(RuntimePrimitive {
                node: node.index(),
                skin: node.skin().map(|skin| skin.index()),
                positions,
                normals,
                tangents,
                joints,
                weights,
            });
        }
    }
    for child in node.children() {
        collect_primitives(child, get_buffer_data, output)?;
    }
    Ok(())
}

fn parse_animation<'a, F>(
    animation: gltf::Animation<'a>,
    get_buffer_data: &F,
) -> Result<AnimationClip, RuntimeError>
where
    F: Clone + for<'b> Fn(gltf::Buffer<'b>) -> Option<&'a [u8]>,
{
    let mut channels = Vec::new();
    let mut duration: f32 = 0.0;
    let mut unsupported = Vec::new();
    for channel in animation.channels() {
        let sampler = channel.sampler();
        let interpolation = match sampler.interpolation() {
            GltfInterpolation::Step => Interpolation::Step,
            GltfInterpolation::Linear => Interpolation::Linear,
            GltfInterpolation::CubicSpline => {
                unsupported.push(format!(
                    "channel targeting node {} uses CUBICSPLINE",
                    channel.target().node().index()
                ));
                continue;
            }
        };
        let reader = channel.reader(get_buffer_data.clone());
        let times = reader
            .read_inputs()
            .ok_or_else(|| {
                RuntimeError::Invalid(
                    "Animation sampler has no input accessor".to_owned(),
                )
            })?
            .collect::<Vec<_>>();
        if times.is_empty() || times.iter().any(|time| !time.is_finite()) {
            return Err(RuntimeError::Invalid(
                "Animation sampler has invalid input times".to_owned(),
            ));
        }
        if times.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(RuntimeError::Invalid(
                "Animation sampler times are not strictly increasing"
                    .to_owned(),
            ));
        }
        duration = duration.max(*times.last().unwrap_or(&0.0));
        let path = match channel.target().property() {
            gltf::animation::Property::Translation => {
                AnimationPath::Translation
            }
            gltf::animation::Property::Rotation => AnimationPath::Rotation,
            gltf::animation::Property::Scale => AnimationPath::Scale,
            gltf::animation::Property::MorphTargetWeights => {
                unsupported.push(format!(
                    "channel targeting node {} uses Morph Target weights",
                    channel.target().node().index()
                ));
                continue;
            }
        };
        let values = match reader.read_outputs().ok_or_else(|| {
            RuntimeError::Invalid(
                "Animation sampler has no output accessor".to_owned(),
            )
        })? {
            ReadOutputs::Translations(values) | ReadOutputs::Scales(values) => {
                values
                    .map(|value| [value[0], value[1], value[2], 0.0])
                    .collect::<Vec<_>>()
            }
            ReadOutputs::Rotations(values) => values
                .into_f32()
                .map(|value| [value[0], value[1], value[2], value[3]])
                .collect::<Vec<_>>(),
            ReadOutputs::MorphTargetWeights(_) => unreachable!(),
        };
        if values.len() != times.len() {
            return Err(RuntimeError::Invalid(format!(
                "Animation channel for node {} has mismatched input/output counts",
                channel.target().node().index()
            )));
        }
        if values.iter().any(|value| !finite_vec4(*value)) {
            return Err(RuntimeError::Invalid(
                "Animation output contains non-finite values".to_owned(),
            ));
        }
        if path == AnimationPath::Rotation
            && values
                .iter()
                .any(|value| quaternion_length(*value) <= f32::EPSILON)
        {
            return Err(RuntimeError::Invalid(
                "Animation rotation output contains a zero quaternion"
                    .to_owned(),
            ));
        }
        channels.push(AnimationChannel {
            node: channel.target().node().index(),
            curve: AnimationCurve {
                path,
                times,
                values,
                interpolation,
            },
        });
    }
    Ok(AnimationClip {
        name: animation.name().unwrap_or("Animation").to_owned(),
        duration,
        channels,
        unsupported,
    })
}

fn compute_world(
    index: usize,
    nodes: &[RuntimeNode],
    local: &[Matrix4],
    world: &mut [Matrix4],
    visiting: &mut [bool],
) -> Result<(), RuntimeError> {
    if visiting[index] {
        return Err(RuntimeError::Invalid(
            "Node hierarchy contains a cycle".to_owned(),
        ));
    }
    if world[index] != identity() {
        return Ok(());
    }
    visiting[index] = true;
    world[index] = if let Some(parent) = nodes[index].parent {
        if parent >= nodes.len() {
            return Err(RuntimeError::Invalid(
                "Node parent index is invalid".to_owned(),
            ));
        }
        compute_world(parent, nodes, local, world, visiting)?;
        multiply(world[parent], local[index])
    } else {
        local[index]
    };
    visiting[index] = false;
    Ok(())
}

fn resolve_node_pose(
    index: usize,
    nodes: &[RuntimeNode],
    locals: &[RuntimeNodePose],
    sampled: &mut [Option<RuntimeNodePose>],
    visiting: &mut [bool],
) -> Result<RuntimeNodePose, RuntimeError> {
    if let Some(pose) = sampled.get(index).and_then(|pose| *pose) {
        return Ok(pose);
    }
    if visiting.get(index).copied().unwrap_or(false) {
        return Err(RuntimeError::Invalid(
            "Node hierarchy contains a cycle".to_owned(),
        ));
    }
    let local = locals.get(index).copied().ok_or_else(|| {
        RuntimeError::Invalid("Node pose index is invalid".to_owned())
    })?;
    visiting[index] = true;
    let pose = if let Some(parent) = nodes[index].parent {
        if parent >= nodes.len() {
            return Err(RuntimeError::Invalid(
                "Node parent index is invalid".to_owned(),
            ));
        }
        let parent_pose =
            resolve_node_pose(parent, nodes, locals, sampled, visiting)?;
        RuntimeNodePose {
            world_translation: add_vec3(
                parent_pose.world_translation,
                rotate_vector(
                    parent_pose.world_rotation,
                    multiply_vec3(
                        parent_pose.world_scale,
                        local.local_translation,
                    ),
                ),
            ),
            world_rotation: normalize_quaternion(multiply_quaternion(
                parent_pose.world_rotation,
                local.local_rotation,
            )),
            world_scale: multiply_vec3(
                parent_pose.world_scale,
                local.local_scale,
            ),
            ..local
        }
    } else {
        RuntimeNodePose {
            world_translation: local.local_translation,
            world_rotation: local.local_rotation,
            world_scale: local.local_scale,
            ..local
        }
    };
    if !finite_vec3(pose.world_translation)
        || !finite_vec4(pose.world_rotation)
        || !finite_vec3(pose.world_scale)
    {
        return Err(RuntimeError::Invalid(
            "Node pose contains non-finite world transforms".to_owned(),
        ));
    }
    visiting[index] = false;
    sampled[index] = Some(pose);
    Ok(pose)
}

fn identity_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn add_vec3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn multiply_vec3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn multiply_quaternion(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn rotate_vector(rotation: [f32; 4], value: [f32; 3]) -> [f32; 3] {
    let inverse = [-rotation[0], -rotation[1], -rotation[2], rotation[3]];
    let result = multiply_quaternion(
        multiply_quaternion(rotation, [value[0], value[1], value[2], 0.0]),
        inverse,
    );
    [result[0], result[1], result[2]]
}

fn blend_point(
    point: [f32; 3],
    joints: [u16; 4],
    weights: [f32; 4],
    matrices: &[Matrix4],
) -> Result<([f32; 3], f32), RuntimeError> {
    let mut result = [0.0; 3];
    let mut total = 0.0;
    for (joint, weight) in joints.into_iter().zip(weights) {
        if weight <= f32::EPSILON {
            continue;
        }
        let matrix = matrices.get(joint as usize).ok_or_else(|| {
            RuntimeError::Invalid(format!(
                "Vertex references missing joint {joint}"
            ))
        })?;
        let value = transform_point(*matrix, point);
        result[0] += value[0] * weight;
        result[1] += value[1] * weight;
        result[2] += value[2] * weight;
        total += weight;
    }
    Ok((result, total))
}

fn blend_vector(
    vector: [f32; 3],
    joints: [u16; 4],
    weights: [f32; 4],
    matrices: &[Matrix4],
) -> Result<([f32; 3], f32), RuntimeError> {
    let mut result = [0.0; 3];
    let mut total = 0.0;
    for (joint, weight) in joints.into_iter().zip(weights) {
        if weight <= f32::EPSILON {
            continue;
        }
        let matrix = matrices.get(joint as usize).ok_or_else(|| {
            RuntimeError::Invalid(format!(
                "Vertex references missing joint {joint}"
            ))
        })?;
        let value = transform_vector(*matrix, vector);
        result[0] += value[0] * weight;
        result[1] += value[1] * weight;
        result[2] += value[2] * weight;
        total += weight;
    }
    Ok((result, total))
}

fn compose(
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
) -> Matrix4 {
    let [x, y, z, w] = normalize_quaternion(rotation);
    [
        (1.0 - 2.0 * (y * y + z * z)) * scale[0],
        (2.0 * (x * y + z * w)) * scale[0],
        (2.0 * (x * z - y * w)) * scale[0],
        0.0,
        (2.0 * (x * y - z * w)) * scale[1],
        (1.0 - 2.0 * (x * x + z * z)) * scale[1],
        (2.0 * (y * z + x * w)) * scale[1],
        0.0,
        (2.0 * (x * z + y * w)) * scale[2],
        (2.0 * (y * z - x * w)) * scale[2],
        (1.0 - 2.0 * (x * x + y * y)) * scale[2],
        0.0,
        translation[0],
        translation[1],
        translation[2],
        1.0,
    ]
}

pub fn identity() -> Matrix4 {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 1.0,
    ]
}

pub fn multiply(a: Matrix4, b: Matrix4) -> Matrix4 {
    let mut result = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            result[column * 4 + row] = (0..4)
                .map(|index| a[index * 4 + row] * b[column * 4 + index])
                .sum();
        }
    }
    result
}

fn transform_point(matrix: Matrix4, point: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * point[0]
            + matrix[4] * point[1]
            + matrix[8] * point[2]
            + matrix[12],
        matrix[1] * point[0]
            + matrix[5] * point[1]
            + matrix[9] * point[2]
            + matrix[13],
        matrix[2] * point[0]
            + matrix[6] * point[1]
            + matrix[10] * point[2]
            + matrix[14],
    ]
}

fn transform_vector(matrix: Matrix4, vector: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * vector[0] + matrix[4] * vector[1] + matrix[8] * vector[2],
        matrix[1] * vector[0] + matrix[5] * vector[1] + matrix[9] * vector[2],
        matrix[2] * vector[0] + matrix[6] * vector[1] + matrix[10] * vector[2],
    ]
}

fn matrix_from_columns(columns: [[f32; 4]; 4]) -> Matrix4 {
    [
        columns[0][0],
        columns[0][1],
        columns[0][2],
        columns[0][3],
        columns[1][0],
        columns[1][1],
        columns[1][2],
        columns[1][3],
        columns[2][0],
        columns[2][1],
        columns[2][2],
        columns[2][3],
        columns[3][0],
        columns[3][1],
        columns[3][2],
        columns[3][3],
    ]
}

fn lerp(a: [f32; 4], b: [f32; 4], amount: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * amount,
        a[1] + (b[1] - a[1]) * amount,
        a[2] + (b[2] - a[2]) * amount,
        a[3] + (b[3] - a[3]) * amount,
    ]
}

fn slerp(a: [f32; 4], b: [f32; 4], amount: f32) -> [f32; 4] {
    let a = normalize_quaternion(a);
    let mut b = normalize_quaternion(b);
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        b = b.map(|value| -value);
        dot = -dot;
    }
    if dot > 0.9995 {
        return normalize_quaternion(lerp(a, b, amount));
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin();
    let first = ((1.0 - amount) * theta).sin() / sin_theta;
    let second = (amount * theta).sin() / sin_theta;
    normalize_quaternion([
        a[0] * first + b[0] * second,
        a[1] * first + b[1] * second,
        a[2] * first + b[2] * second,
        a[3] * first + b[3] * second,
    ])
}

fn normalize_quaternion(value: [f32; 4]) -> [f32; 4] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if length > f32::EPSILON && length.is_finite() {
        value.map(|component| component / length)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn quaternion_length(value: [f32; 4]) -> f32 {
    value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt()
}

fn normalize_vec3(value: [f32; 3]) -> [f32; 3] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if length > f32::EPSILON && length.is_finite() {
        value.map(|component| component / length)
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn finite_vec3(value: [f32; 3]) -> bool {
    value.iter().all(|component| component.is_finite())
}

fn finite_vec4(value: [f32; 4]) -> bool {
    value.iter().all(|component| component.is_finite())
}

#[cfg(test)]
#[path = "animation_runtime_tests.rs"]
mod tests;
