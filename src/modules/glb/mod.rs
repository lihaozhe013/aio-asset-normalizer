//! GLB document loading, inspection, editing, and atomic export.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod transform;
use self::transform::*;

mod resources;
pub use self::resources::{PrimitiveTarget, TextureSlot};

mod animation;
mod animation_runtime;
mod smart_loop;
#[allow(unused_imports)]
pub use animation_runtime::{AnimationClip, AnimationRuntime, RuntimeNodePose};
#[allow(unused_imports)]
pub use smart_loop::{SmartLoopOptions, SmartLoopReport};

#[derive(Debug)]
pub enum GlbError {
    Io(io::Error),
    Invalid(String),
    Unsupported(String),
}

impl std::fmt::Display for GlbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Invalid(message) => write!(f, "Invalid GLB: {message}"),
            Self::Unsupported(message) => {
                write!(f, "Unsupported GLB feature: {message}")
            }
        }
    }
}

impl std::error::Error for GlbError {}

impl From<io::Error> for GlbError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<gltf::Error> for GlbError {
    fn from(error: gltf::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StandardizationProfile {
    pub unit_scale: f32,
    pub up_axis: Axis,
    pub forward_axis: ForwardAxis,
}

impl Default for StandardizationProfile {
    fn default() -> Self {
        Self {
            unit_scale: 1.0,
            up_axis: Axis::Y,
            forward_axis: ForwardAxis::NegativeZ,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ForwardAxis {
    PositiveX,
    NegativeX,
    PositiveZ,
    NegativeZ,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlbSummary {
    pub scenes: usize,
    pub nodes: usize,
    pub meshes: usize,
    pub materials: usize,
    pub skins: usize,
    pub animations: usize,
    pub images: usize,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SkinData {
    pub index: usize,
    pub name: String,
    pub skeleton: Option<usize>,
    pub joints: Vec<usize>,
    pub mesh_nodes: Vec<usize>,
    pub nodes: Vec<SkinNode>,
}

#[derive(Debug, Clone)]
pub struct SkinNode {
    pub index: usize,
    pub name: String,
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct AnimationChannelData {
    pub node: usize,
    pub rotations: Vec<[f32; 4]>,
    pub translations: Option<Vec<[f32; 3]>>,
}

#[derive(Debug, Clone)]
pub struct AnimationClipData {
    pub name: String,
    pub times: Vec<f32>,
    pub channels: Vec<AnimationChannelData>,
}

#[derive(Debug, Clone)]
pub struct GlbDocument {
    pub source_path: Option<PathBuf>,
    json: Value,
    bin: Option<Vec<u8>>,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditOperation {
    RotateRoots {
        euler_degrees: [f32; 3],
    },
    ScaleRoots {
        factor: f32,
    },
    TranslateRoots {
        offset: [f32; 3],
    },
    TrimAnimation {
        animation: usize,
        start: f32,
        end: f32,
    },
    ScaleAnimationRate {
        animation: usize,
        rate: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RootTransformPreview {
    pub euler_degrees: [f32; 3],
    pub scale: f32,
    pub translation: [f32; 3],
}

impl Default for RootTransformPreview {
    fn default() -> Self {
        Self {
            euler_degrees: [0.0, 0.0, 0.0],
            scale: 1.0,
            translation: [0.0, 0.0, 0.0],
        }
    }
}

impl RootTransformPreview {
    pub fn to_matrix(self) -> Result<[[f32; 4]; 4], GlbError> {
        if self.translation.iter().any(|value| !value.is_finite()) {
            return Err(GlbError::Invalid(
                "Preview translation must be finite".to_owned(),
            ));
        }
        if !self.scale.is_finite() || self.scale <= 0.0 {
            return Err(GlbError::Invalid(
                "Preview scale must be finite and greater than zero".to_owned(),
            ));
        }
        let rotation = euler_rotation_matrix(self.euler_degrees)?;
        Ok(multiply(
            translation_matrix(self.translation),
            multiply(
                scale_matrix([self.scale, self.scale, self.scale]),
                rotation,
            ),
        ))
    }
}

impl GlbDocument {
    pub fn load(path: &Path) -> Result<Self, GlbError> {
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"))
        {
            return Err(GlbError::Unsupported(
                "Only .glb files can be opened".to_owned(),
            ));
        }
        let bytes = fs::read(path)?;
        Self::from_bytes(&bytes, Some(path.to_path_buf()))
    }

    pub fn summary(&self) -> GlbSummary {
        GlbSummary {
            scenes: array_len(&self.json, "scenes"),
            nodes: array_len(&self.json, "nodes"),
            meshes: array_len(&self.json, "meshes"),
            materials: array_len(&self.json, "materials"),
            skins: array_len(&self.json, "skins"),
            animations: array_len(&self.json, "animations"),
            images: array_len(&self.json, "images"),
            extensions: self
                .json
                .get("extensionsUsed")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        }
    }

    pub fn scene_names(&self) -> Vec<String> {
        names(&self.json, "scenes", "Scene")
    }

    pub fn node_names(&self) -> Vec<String> {
        names(&self.json, "nodes", "Node")
    }

    pub fn mesh_names(&self) -> Vec<String> {
        names(&self.json, "meshes", "Mesh")
    }

    pub fn material_names(&self) -> Vec<String> {
        names(&self.json, "materials", "Material")
    }

    pub fn animation_names(&self) -> Vec<String> {
        names(&self.json, "animations", "Animation")
    }

    pub fn skin_data(&self) -> Result<SkinData, GlbError> {
        self.skin_data_at(0)
    }

    pub fn skin_data_at(
        &self,
        skin_index: usize,
    ) -> Result<SkinData, GlbError> {
        let skins = self
            .json
            .get("skins")
            .and_then(Value::as_array)
            .ok_or_else(|| GlbError::Invalid("GLB has no skins".to_owned()))?;
        let skin = skins.get(skin_index).ok_or_else(|| {
            GlbError::Invalid(format!("GLB has no Skin entry {skin_index}"))
        })?;
        let joints = skin
            .get("joints")
            .and_then(Value::as_array)
            .ok_or_else(|| GlbError::Invalid("Skin has no joints".to_owned()))?
            .iter()
            .map(|value| {
                let index = value.as_u64().ok_or_else(|| {
                    GlbError::Invalid("Skin joint index is invalid".to_owned())
                })?;
                usize::try_from(index).map_err(|_| {
                    GlbError::Invalid(
                        "Skin joint index is out of range".to_owned(),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut unique_joints = BTreeSet::new();
        for joint in &joints {
            if !unique_joints.insert(*joint) {
                return Err(GlbError::Invalid(format!(
                    "Skin {skin_index} lists joint {joint} more than once"
                )));
            }
        }
        let node_values = self
            .json
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| GlbError::Invalid("GLB has no nodes".to_owned()))?;
        let mut parents = vec![None; node_values.len()];
        for (index, node) in node_values.iter().enumerate() {
            if let Some(children) =
                node.get("children").and_then(Value::as_array)
            {
                for child in children {
                    let child = child.as_u64().ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Node {index} contains a non-integer child index"
                        ))
                    })?;
                    let child = usize::try_from(child).map_err(|_| {
                        GlbError::Invalid(format!(
                            "Node {index} child index is out of range"
                        ))
                    })?;
                    if child >= parents.len() {
                        return Err(GlbError::Invalid(format!(
                            "Node {index} references missing child {child}"
                        )));
                    }
                    if parents[child].replace(index).is_some() {
                        return Err(GlbError::Invalid(format!(
                            "Node {child} has more than one parent"
                        )));
                    }
                }
            }
        }
        for start in 0..parents.len() {
            let mut seen = HashSet::new();
            let mut current = Some(start);
            while let Some(index) = current {
                if !seen.insert(index) {
                    return Err(GlbError::Invalid(format!(
                        "Node hierarchy contains a cycle at node {index}"
                    )));
                }
                current = parents[index];
            }
        }
        if joints.iter().any(|index| *index >= node_values.len()) {
            return Err(GlbError::Invalid(
                "Skin joint index exceeds the node array".to_owned(),
            ));
        }
        self.validate_skin_inverse_bind_matrices(
            skin_index,
            skin,
            joints.len(),
        )?;
        let nodes = node_values
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let matrix = node_matrix(node)?;
                let (translation, rotation, scale) = decompose_matrix(matrix)?;
                Ok(SkinNode {
                    index,
                    name: match node.get("name") {
                        Some(value) => value
                            .as_str()
                            .ok_or_else(|| {
                                GlbError::Invalid(format!(
                                    "Node {index} name is not a string"
                                ))
                            })?
                            .to_owned(),
                        None => format!("Node {index}"),
                    },
                    parent: parents[index],
                    translation,
                    rotation,
                    scale,
                })
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        let mut mesh_nodes = Vec::new();
        for (index, node) in node_values.iter().enumerate() {
            if let Some(value) = node.get("skin") {
                let node_skin = value.as_u64().ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Node {index} Skin reference is not an integer"
                    ))
                })?;
                let node_skin = usize::try_from(node_skin).map_err(|_| {
                    GlbError::Invalid(format!(
                        "Node {index} Skin reference is out of range"
                    ))
                })?;
                if node_skin >= skins.len() {
                    return Err(GlbError::Invalid(format!(
                        "Node {index} references missing Skin {node_skin}"
                    )));
                }
                if node_skin == skin_index {
                    mesh_nodes.push(index);
                }
            }
        }
        let skeleton = if let Some(value) = skin.get("skeleton") {
            let skeleton = value.as_u64().ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Skin {skin_index} skeleton reference is not an integer"
                ))
            })?;
            let skeleton = usize::try_from(skeleton).map_err(|_| {
                GlbError::Invalid(format!(
                    "Skin {skin_index} skeleton reference is out of range"
                ))
            })?;
            if skeleton >= node_values.len() {
                return Err(GlbError::Invalid(format!(
                    "Skin {skin_index} references missing skeleton node {skeleton}"
                )));
            }
            Some(skeleton)
        } else {
            None
        };
        let skin_name = match skin.get("name") {
            Some(value) => value
                .as_str()
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Skin {skin_index} name is not a string"
                    ))
                })?
                .to_owned(),
            None => "Skin".to_owned(),
        };
        Ok(SkinData {
            index: skin_index,
            name: skin_name,
            skeleton,
            joints,
            mesh_nodes,
            nodes,
        })
    }

    fn validate_skin_inverse_bind_matrices(
        &self,
        skin_index: usize,
        skin: &Value,
        joint_count: usize,
    ) -> Result<(), GlbError> {
        let Some(value) = skin.get("inverseBindMatrices") else {
            return Ok(());
        };
        let accessor_index = value.as_u64().ok_or_else(|| {
            GlbError::Invalid(format!(
                "Skin {skin_index} inverseBindMatrices is not an integer"
            ))
        })?;
        let accessor_index = usize::try_from(accessor_index).map_err(|_| {
            GlbError::Invalid(format!(
                "Skin {skin_index} inverseBindMatrices index is out of range"
            ))
        })?;
        let accessor = self.accessor(accessor_index)?;
        if accessor.get("componentType").and_then(Value::as_u64) != Some(5126)
            || accessor.get("type").and_then(Value::as_str) != Some("MAT4")
        {
            return Err(GlbError::Unsupported(format!(
                "Skin {skin_index} inverse bind matrices must use FLOAT MAT4 values"
            )));
        }
        let count =
            accessor
                .get("count")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                    "Skin {skin_index} inverse bind matrix count is missing"
                ))
                })?;
        let count = usize::try_from(count).map_err(|_| {
            GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind matrix count is out of range"
            ))
        })?;
        if count != joint_count {
            return Err(GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind matrix count {count} does not match {joint_count} joints"
            )));
        }
        if accessor.get("sparse").is_some() {
            return Err(GlbError::Unsupported(format!(
                "Skin {skin_index} sparse inverse bind matrices are not supported"
            )));
        }
        let view_index = accessor
            .get("bufferView")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Skin {skin_index} inverse bind matrices have no bufferView"
                ))
            })?;
        let view_index = usize::try_from(view_index).map_err(|_| {
            GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind bufferView index is out of range"
            ))
        })?;
        let view = self
            .json
            .get("bufferViews")
            .and_then(Value::as_array)
            .and_then(|views| views.get(view_index))
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Skin {skin_index} inverse bind bufferView is missing"
                ))
            })?;
        let buffer_index = json_u64_field(
            view.get("buffer"),
            "Skin inverse bind buffer index",
            Some(0),
        )?;
        if buffer_index != 0 {
            return Err(GlbError::Unsupported(format!(
                "Skin {skin_index} inverse bind matrices must use the GLB BIN buffer"
            )));
        }
        let view_offset = json_u64_field(
            view.get("byteOffset"),
            &format!("Skin {skin_index} inverse bind bufferView byteOffset"),
            Some(0),
        )?;
        let view_length = json_u64_field(
            view.get("byteLength"),
            &format!("Skin {skin_index} inverse bind bufferView byteLength"),
            None,
        )?;
        let accessor_offset = json_u64_field(
            accessor.get("byteOffset"),
            &format!("Skin {skin_index} inverse bind accessor byteOffset"),
            Some(0),
        )?;
        let stride = json_u64_field(
            view.get("byteStride"),
            &format!("Skin {skin_index} inverse bind byteStride"),
            Some(64),
        )?;
        if stride < 64 || stride % 4 != 0 {
            return Err(GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind byteStride is invalid"
            )));
        }
        let view_offset = usize::try_from(view_offset).map_err(|_| {
            GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind bufferView offset is out of range"
            ))
        })?;
        let view_length = usize::try_from(view_length).map_err(|_| {
            GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind bufferView length is out of range"
            ))
        })?;
        let accessor_offset =
            usize::try_from(accessor_offset).map_err(|_| {
                GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind accessor offset is out of range"
            ))
            })?;
        let stride = usize::try_from(stride).map_err(|_| {
            GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind byteStride is out of range"
            ))
        })?;
        let view_end = view_offset.checked_add(view_length).ok_or_else(|| {
            GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind bufferView range is out of range"
            ))
        })?;
        let first =
            view_offset.checked_add(accessor_offset).ok_or_else(|| {
                GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind accessor range is out of range"
            ))
            })?;
        let last = if count == 0 {
            first
        } else {
            first
                .checked_add((count - 1).checked_mul(stride).ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Skin {skin_index} inverse bind accessor range is out of range"
                    ))
                })?)
                .and_then(|offset| offset.checked_add(64))
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Skin {skin_index} inverse bind accessor range is out of range"
                    ))
                })?
        };
        if last > view_end {
            return Err(GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind accessor exceeds its bufferView"
            )));
        }
        let bin = self.bin.as_deref().ok_or_else(|| {
            GlbError::Invalid("Accessor requires a BIN chunk".to_owned())
        })?;
        if view_end > bin.len() {
            return Err(GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind bufferView exceeds the BIN chunk"
            )));
        }
        let non_finite = (0..count).any(|index| {
            let Some(start) = first.checked_add(index.saturating_mul(stride))
            else {
                return true;
            };
            let Some(end) = start.checked_add(64) else {
                return true;
            };
            bin.get(start..end).is_none_or(|bytes| {
                bytes.chunks_exact(4).any(|chunk| {
                    !f32::from_le_bytes([
                        chunk[0], chunk[1], chunk[2], chunk[3],
                    ])
                    .is_finite()
                })
            })
        });
        if non_finite {
            return Err(GlbError::Invalid(format!(
                "Skin {skin_index} inverse bind matrices contain non-finite values"
            )));
        }
        Ok(())
    }

    pub fn append_animation(
        &mut self,
        clip: &AnimationClipData,
    ) -> Result<(), GlbError> {
        let backup = self.clone();
        if let Err(error) = self.append_animation_inner(clip) {
            *self = backup;
            return Err(error);
        }
        Ok(())
    }

    fn append_animation_inner(
        &mut self,
        clip: &AnimationClipData,
    ) -> Result<(), GlbError> {
        if clip.times.len() < 2
            || clip.times.iter().any(|time| !time.is_finite())
            || clip.times.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(GlbError::Invalid(
                "Animation must contain at least two finite keyframe times"
                    .to_owned(),
            ));
        }
        if clip.channels.is_empty() {
            return Err(GlbError::Invalid(
                "Animation has no channels".to_owned(),
            ));
        }
        let input = self.append_float_accessor(
            &clip
                .times
                .iter()
                .map(|time| vec![*time])
                .collect::<Vec<_>>(),
            "SCALAR",
        )?;
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for channel in &clip.channels {
            if self
                .json
                .get("nodes")
                .and_then(Value::as_array)
                .and_then(|nodes| nodes.get(channel.node))
                .is_none()
            {
                return Err(GlbError::Invalid(format!(
                    "Animation target node {} does not exist",
                    channel.node
                )));
            }
            self.ensure_node_trs(channel.node)?;
            if channel.rotations.len() != clip.times.len()
                || channel.rotations.iter().any(|rotation| {
                    rotation.iter().any(|value| !value.is_finite())
                        || rotation
                            .iter()
                            .map(|value| value * value)
                            .sum::<f32>()
                            <= f32::EPSILON
                })
            {
                return Err(GlbError::Invalid(
                    "Animation rotation keyframes do not match the timeline"
                        .to_owned(),
                ));
            }
            let output = self.append_float_accessor(
                &channel
                    .rotations
                    .iter()
                    .map(|rotation| rotation.to_vec())
                    .collect::<Vec<_>>(),
                "VEC4",
            )?;
            let sampler = samplers.len();
            samplers.push(json!({"input": input, "output": output, "interpolation": "LINEAR"}));
            channels.push(json!({"sampler": sampler, "target": {"node": channel.node, "path": "rotation"}}));
            if let Some(translations) = &channel.translations {
                if translations.len() != clip.times.len() {
                    return Err(GlbError::Invalid("Animation translation keyframes do not match the timeline".to_owned()));
                }
                if translations.iter().any(|translation| {
                    translation.iter().any(|value| !value.is_finite())
                }) {
                    return Err(GlbError::Invalid(
                        "Animation translation keyframes contain non-finite values"
                            .to_owned(),
                    ));
                }
                let output = self.append_float_accessor(
                    &translations
                        .iter()
                        .map(|translation| translation.to_vec())
                        .collect::<Vec<_>>(),
                    "VEC3",
                )?;
                let sampler = samplers.len();
                samplers.push(json!({"input": input, "output": output, "interpolation": "LINEAR"}));
                channels.push(json!({"sampler": sampler, "target": {"node": channel.node, "path": "translation"}}));
            }
        }
        let animations = self
            .json
            .get_mut("animations")
            .and_then(Value::as_array_mut);
        if let Some(animations) = animations {
            animations.push(json!({"name": clip.name, "samplers": samplers, "channels": channels}));
        } else if let Some(object) = self.json.as_object_mut() {
            object.insert("animations".to_owned(), json!([{"name": clip.name, "samplers": samplers, "channels": channels}]));
        } else {
            return Err(GlbError::Invalid(
                "GLB JSON root is not an object".to_owned(),
            ));
        }
        self.dirty = true;
        Ok(())
    }

    fn ensure_node_trs(&mut self, node_index: usize) -> Result<(), GlbError> {
        let matrix = self
            .json
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| nodes.get(node_index))
            .and_then(|node| node.get("matrix"))
            .cloned();
        let Some(matrix) = matrix else {
            return Ok(());
        };
        let matrix = node_matrix(&json!({"matrix": matrix}))?;
        let (translation, rotation, scale) = decompose_matrix(matrix)?;
        let node = self
            .json
            .get_mut("nodes")
            .and_then(Value::as_array_mut)
            .and_then(|nodes| nodes.get_mut(node_index))
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Animation target node {node_index} disappeared"
                ))
            })?;
        let object = node.as_object_mut().ok_or_else(|| {
            GlbError::Invalid(format!(
                "Animation target node {node_index} is not an object"
            ))
        })?;
        object.remove("matrix");
        object.insert("translation".to_owned(), json!(translation));
        object.insert("rotation".to_owned(), json!(rotation));
        object.insert("scale".to_owned(), json!(scale));
        Ok(())
    }

    /// Replace the document's animation list with one generated clip while
    /// leaving meshes, skins, buffers, extensions, and other resources intact.
    pub fn replace_animations(
        &mut self,
        clip: &AnimationClipData,
    ) -> Result<(), GlbError> {
        let backup = self.clone();
        if let Some(object) = self.json.as_object_mut() {
            object.insert("animations".to_owned(), json!([]));
        } else {
            return Err(GlbError::Invalid(
                "GLB JSON root is not an object".to_owned(),
            ));
        }
        if let Err(error) = self.append_animation(clip) {
            *self = backup;
            return Err(error);
        }
        Ok(())
    }

    pub fn strip_render_resources(&mut self) {
        if let Some(nodes) =
            self.json.get_mut("nodes").and_then(Value::as_array_mut)
        {
            for node in nodes {
                if let Some(object) = node.as_object_mut() {
                    object.remove("mesh");
                    object.remove("camera");
                }
            }
        }
        if let Some(object) = self.json.as_object_mut() {
            for key in [
                "meshes",
                "materials",
                "textures",
                "images",
                "samplers",
                "cameras",
            ] {
                object.remove(key);
            }
        }
        self.dirty = true;
    }

    pub fn apply(&mut self, operation: EditOperation) -> Result<(), GlbError> {
        match operation {
            EditOperation::RotateRoots { euler_degrees } => {
                let rotation = euler_rotation_matrix(euler_degrees)?;
                self.map_root_nodes(|node| {
                    let current = node_matrix(node)?;
                    set_matrix(node, multiply(rotation, current));
                    Ok(())
                })?;
            }
            EditOperation::ScaleRoots { factor } => {
                if !factor.is_finite() || factor <= 0.0 {
                    return Err(GlbError::Invalid(
                        "Scale factor must be finite and greater than zero"
                            .to_owned(),
                    ));
                }
                let scale = scale_matrix([factor, factor, factor]);
                self.map_root_nodes(|node| {
                    let current = node_matrix(node)?;
                    set_matrix(node, multiply(scale, current));
                    Ok(())
                })?;
            }
            EditOperation::TranslateRoots { offset } => {
                let translation = translation_matrix(offset);
                self.map_root_nodes(|node| {
                    let current = node_matrix(node)?;
                    set_matrix(node, multiply(translation, current));
                    Ok(())
                })?;
            }
            EditOperation::TrimAnimation {
                animation,
                start,
                end,
            } => self.trim_animation(animation, start, end)?,
            EditOperation::ScaleAnimationRate { animation, rate } => {
                self.scale_animation_rate(animation, rate)?
            }
        }
        self.dirty = true;
        Ok(())
    }

    pub fn standardize(
        &mut self,
        profile: &StandardizationProfile,
    ) -> Result<(), GlbError> {
        if !profile.unit_scale.is_finite() || profile.unit_scale <= 0.0 {
            return Err(GlbError::Invalid(
                "Unit scale must be finite and greater than zero".to_owned(),
            ));
        }
        if (profile.unit_scale - 1.0).abs() > f32::EPSILON {
            self.apply(EditOperation::ScaleRoots {
                factor: profile.unit_scale,
            })?;
        }
        if profile.up_axis != Axis::Y
            || profile.forward_axis != ForwardAxis::NegativeZ
        {
            return Err(GlbError::Unsupported(
                "Automatic axis detection is not enabled; use a manual rotation before export"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    pub fn export_atomic(&self, path: &Path) -> Result<(), GlbError> {
        let bytes = self.to_bytes()?;
        Self::from_bytes(&bytes, None)?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("glb.tmp");
        fs::write(&temporary, bytes)?;
        if let Err(error) =
            crate::modules::atomic_file::replace(&temporary, path)
        {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(())
    }

    fn from_bytes(
        bytes: &[u8],
        source_path: Option<PathBuf>,
    ) -> Result<Self, GlbError> {
        let glb = gltf::binary::Glb::from_slice(bytes)?;
        let json =
            serde_json::from_slice::<Value>(&glb.json).map_err(|error| {
                GlbError::Invalid(format!("JSON chunk: {error}"))
            })?;
        gltf::Gltf::from_slice(bytes)?;
        Ok(Self {
            source_path,
            json,
            bin: glb.bin.map(Cow::into_owned),
            dirty: false,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, GlbError> {
        let mut json_bytes =
            serde_json::to_vec(&self.json).map_err(|error| {
                GlbError::Invalid(format!("Serialize JSON: {error}"))
            })?;
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }
        let bin = self.bin.as_deref().map(|data| {
            let mut padded = data.to_vec();
            while padded.len() % 4 != 0 {
                padded.push(0);
            }
            padded
        });
        let glb = gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(json_bytes),
            bin: bin.map(Cow::Owned),
        };
        glb.to_vec().map_err(GlbError::from)
    }

    fn map_root_nodes<F>(&mut self, mut operation: F) -> Result<(), GlbError>
    where
        F: FnMut(&mut Value) -> Result<(), GlbError>,
    {
        let mut roots = BTreeSet::new();
        if let Some(scenes) = self.json.get("scenes").and_then(Value::as_array)
        {
            for scene in scenes {
                if let Some(nodes) =
                    scene.get("nodes").and_then(Value::as_array)
                {
                    for value in nodes {
                        let index = value.as_u64().ok_or_else(|| {
                            GlbError::Invalid(
                                "Scene root node index is not an integer"
                                    .to_owned(),
                            )
                        })?;
                        let index = usize::try_from(index).map_err(|_| {
                            GlbError::Invalid(
                                "Scene root node index is out of range"
                                    .to_owned(),
                            )
                        })?;
                        roots.insert(index);
                    }
                }
            }
        }
        if roots.is_empty() {
            roots.insert(0);
        }
        let nodes = self
            .json
            .get_mut("nodes")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no nodes array".to_owned())
            })?;
        for index in roots {
            let node = nodes.get_mut(index).ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Scene references missing node {index}"
                ))
            })?;
            operation(node)?;
        }
        Ok(())
    }

    fn trim_animation(
        &mut self,
        animation_index: usize,
        start: f32,
        end: f32,
    ) -> Result<(), GlbError> {
        self.trim_animation_interpolated(animation_index, start, end)
    }

    fn accessor(&self, index: usize) -> Result<&Value, GlbError> {
        self.json
            .get("accessors")
            .and_then(Value::as_array)
            .and_then(|items| items.get(index))
            .ok_or_else(|| {
                GlbError::Invalid(format!("Missing accessor {index}"))
            })
    }

    fn read_accessor_f32(
        &self,
        index: usize,
    ) -> Result<Vec<Vec<f32>>, GlbError> {
        let accessor = self.accessor(index)?;
        let component_type = accessor
            .get("componentType")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if component_type != 5126
            || accessor.get("type").and_then(Value::as_str) != Some("SCALAR")
        {
            return Err(GlbError::Unsupported(
                "Animation inputs must be float scalar accessors".to_owned(),
            ));
        }
        let count = accessor
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                GlbError::Invalid("Accessor count is missing".to_owned())
            })
            .and_then(|count| {
                usize::try_from(count).map_err(|_| {
                    GlbError::Invalid(
                        "Accessor count is out of range".to_owned(),
                    )
                })
            })?;
        let bytes = self.accessor_bytes(accessor, 4)?;
        Ok((0..count)
            .map(|index| {
                let offset = index * 4;
                [
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]
            })
            .map(|raw| vec![f32::from_le_bytes(raw)])
            .collect())
    }

    fn accessor_bytes(
        &self,
        accessor: &Value,
        element_size: usize,
    ) -> Result<Vec<u8>, GlbError> {
        let view_index = accessor
            .get("bufferView")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                GlbError::Unsupported(
                    "Sparse or unbound accessors are not supported".to_owned(),
                )
            })
            .and_then(|index| {
                usize::try_from(index).map_err(|_| {
                    GlbError::Invalid(
                        "Accessor bufferView index is out of range".to_owned(),
                    )
                })
            })?;
        let view = self
            .json
            .get("bufferViews")
            .and_then(Value::as_array)
            .and_then(|items| items.get(view_index))
            .ok_or_else(|| {
                GlbError::Invalid(format!("Missing bufferView {view_index}"))
            })?;
        let buffer_index = json_u64_field(
            view.get("buffer"),
            "Animation buffer index",
            Some(0),
        )?;
        if buffer_index != 0 {
            return Err(GlbError::Unsupported(
                "Animation accessors must use the GLB BIN buffer".to_owned(),
            ));
        }
        if view.get("byteStride").is_some() || accessor.get("sparse").is_some()
        {
            return Err(GlbError::Unsupported(
                "Strided and sparse animation accessors are not supported"
                    .to_owned(),
            ));
        }
        let view_offset = json_u64_field(
            view.get("byteOffset"),
            "Animation bufferView byteOffset",
            Some(0),
        )?;
        let view_length = json_u64_field(
            view.get("byteLength"),
            "Animation bufferView byteLength",
            None,
        )?;
        let accessor_offset = json_u64_field(
            accessor.get("byteOffset"),
            "Animation accessor byteOffset",
            Some(0),
        )?;
        let count =
            json_u64_field(accessor.get("count"), "Accessor count", None)?;
        let offset = usize::try_from(view_offset)
            .ok()
            .and_then(|value| {
                usize::try_from(accessor_offset).ok().and_then(
                    |accessor_offset| value.checked_add(accessor_offset),
                )
            })
            .ok_or_else(|| {
                GlbError::Invalid(
                    "Accessor byte offset is out of range".to_owned(),
                )
            })?;
        let length = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(element_size))
            .ok_or_else(|| {
                GlbError::Invalid(
                    "Accessor byte length is out of range".to_owned(),
                )
            })?;
        let end = offset.checked_add(length).ok_or_else(|| {
            GlbError::Invalid("Accessor byte range is out of range".to_owned())
        })?;
        let view_end = usize::try_from(view_length)
            .ok()
            .and_then(|length| {
                usize::try_from(view_offset)
                    .ok()
                    .and_then(|offset| offset.checked_add(length))
            })
            .ok_or_else(|| {
                GlbError::Invalid(
                    "Animation bufferView byte range is out of range"
                        .to_owned(),
                )
            })?;
        if end > view_end {
            return Err(GlbError::Invalid(
                "Animation accessor exceeds its bufferView".to_owned(),
            ));
        }
        let bin = self.bin.as_deref().ok_or_else(|| {
            GlbError::Invalid("Accessor requires a BIN chunk".to_owned())
        })?;
        if view_end > bin.len() {
            return Err(GlbError::Invalid(
                "Animation bufferView exceeds the BIN chunk".to_owned(),
            ));
        }
        bin.get(offset..end).map(ToOwned::to_owned).ok_or_else(|| {
            GlbError::Invalid("Animation accessor exceeds BIN chunk".to_owned())
        })
    }

    fn append_float_accessor(
        &mut self,
        values: &[Vec<f32>],
        accessor_type: &str,
    ) -> Result<usize, GlbError> {
        let components = match accessor_type {
            "SCALAR" => 1,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => {
                return Err(GlbError::Unsupported(
                    "Animation accessor type is unsupported".to_owned(),
                ))
            }
        };
        if values.is_empty()
            || values.iter().any(|value| {
                value.len() != components
                    || value.iter().any(|component| !component.is_finite())
            })
        {
            return Err(GlbError::Invalid(
                "Animation accessor component count is invalid".to_owned(),
            ));
        }
        let flat = values
            .iter()
            .flat_map(|value| value.iter().copied())
            .collect::<Vec<_>>();
        let bytes = flat
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let bin = self.bin.get_or_insert_with(Vec::new);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let offset = bin.len();
        bin.extend_from_slice(&bytes);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let views = self
            .json
            .get_mut("bufferViews")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no bufferViews array".to_owned())
            })?;
        let view_index = views.len();
        views.push(json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": bytes.len()
        }));
        let accessors = self
            .json
            .get_mut("accessors")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no accessors array".to_owned())
            })?;
        let accessor_index = accessors.len();
        accessors.push(json!({
            "bufferView": view_index,
            "componentType": 5126,
            "count": values.len(),
            "type": accessor_type
        }));
        if let Some(buffer) = self
            .json
            .get_mut("buffers")
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(0))
        {
            buffer["byteLength"] = json!(bin.len());
        }
        Ok(accessor_index)
    }
}

fn json_u64_field(
    value: Option<&Value>,
    label: &str,
    default: Option<u64>,
) -> Result<u64, GlbError> {
    match value {
        Some(value) => value.as_u64().ok_or_else(|| {
            GlbError::Invalid(format!("{label} must be a non-negative integer"))
        }),
        None => default
            .ok_or_else(|| GlbError::Invalid(format!("{label} is missing"))),
    }
}

fn array_len(json: &Value, key: &str) -> usize {
    json.get(key).and_then(Value::as_array).map_or(0, Vec::len)
}

fn names(json: &Value, key: &str, fallback: &str) -> Vec<String> {
    json.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{fallback} {index}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn euler_rotation_matrix(degrees: [f32; 3]) -> Result<[[f32; 4]; 4], GlbError> {
    if degrees.iter().any(|value| !value.is_finite()) {
        return Err(GlbError::Invalid(
            "Euler rotation angles must be finite".to_owned(),
        ));
    }
    let rotation_x = rotation_matrix([1.0, 0.0, 0.0], degrees[0].to_radians())?;
    let rotation_y = rotation_matrix([0.0, 1.0, 0.0], degrees[1].to_radians())?;
    let rotation_z = rotation_matrix([0.0, 0.0, 1.0], degrees[2].to_radians())?;
    Ok(multiply(rotation_z, multiply(rotation_y, rotation_x)))
}

fn rotation_matrix(
    axis: [f32; 3],
    radians: f32,
) -> Result<[[f32; 4]; 4], GlbError> {
    let length =
        (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if !length.is_finite() || length <= f32::EPSILON || !radians.is_finite() {
        return Err(GlbError::Invalid(
            "Rotation axis and angle must be finite".to_owned(),
        ));
    }
    let [x, y, z] = [axis[0] / length, axis[1] / length, axis[2] / length];
    let (sin, cos) = radians.sin_cos();
    let one = 1.0 - cos;
    Ok([
        [
            cos + x * x * one,
            x * y * one - z * sin,
            x * z * one + y * sin,
            0.0,
        ],
        [
            y * x * one + z * sin,
            cos + y * y * one,
            y * z * one - x * sin,
            0.0,
        ],
        [
            z * x * one - y * sin,
            z * y * one + x * sin,
            cos + z * z * one,
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

#[cfg(test)]
mod tests;
