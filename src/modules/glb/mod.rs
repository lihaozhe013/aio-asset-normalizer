//! GLB document loading, inspection, editing, and atomic export.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

mod transform;
use self::transform::*;

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
    pub name: String,
    pub joints: Vec<usize>,
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
        axis: [f32; 3],
        degrees: f32,
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
        let glb = gltf::binary::Glb::from_slice(&bytes)?;
        let json =
            serde_json::from_slice::<Value>(&glb.json).map_err(|error| {
                GlbError::Invalid(format!("JSON chunk: {error}"))
            })?;
        gltf::Gltf::from_slice(&bytes)?;
        Ok(Self {
            source_path: Some(path.to_path_buf()),
            json,
            bin: glb.bin.map(Cow::into_owned),
            dirty: false,
        })
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

    pub fn skin_names(&self) -> Vec<String> {
        names(&self.json, "skins", "Skin")
    }

    pub fn skin_data(&self) -> Result<SkinData, GlbError> {
        let skins = self
            .json
            .get("skins")
            .and_then(Value::as_array)
            .ok_or_else(|| GlbError::Invalid("GLB has no skins".to_owned()))?;
        let skin = skins.first().ok_or_else(|| {
            GlbError::Invalid("GLB has no Skin entries".to_owned())
        })?;
        let joints = skin
            .get("joints")
            .and_then(Value::as_array)
            .ok_or_else(|| GlbError::Invalid("Skin has no joints".to_owned()))?
            .iter()
            .map(|value| {
                value.as_u64().map(|index| index as usize).ok_or_else(|| {
                    GlbError::Invalid("Skin joint index is invalid".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
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
                for child in children.iter().filter_map(Value::as_u64) {
                    let child = child as usize;
                    if child < parents.len() {
                        parents[child] = Some(index);
                    }
                }
            }
        }
        let nodes = node_values
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let matrix = node_matrix(node)?;
                let (translation, rotation, scale) = decompose_matrix(matrix)?;
                Ok(SkinNode {
                    index,
                    name: node
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Node {index}")),
                    parent: parents[index],
                    translation,
                    rotation,
                    scale,
                })
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        Ok(SkinData {
            name: skin
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Skin")
                .to_owned(),
            joints,
            nodes,
        })
    }

    pub fn append_animation(
        &mut self,
        clip: &AnimationClipData,
    ) -> Result<(), GlbError> {
        if clip.times.len() < 2
            || clip.times.iter().any(|time| !time.is_finite())
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
            if channel.rotations.len() != clip.times.len()
                || channel.rotations.iter().any(|rotation| {
                    rotation.iter().any(|value| !value.is_finite())
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
            for key in ["meshes", "materials", "textures", "images", "samplers"]
            {
                object.remove(key);
            }
        }
        self.dirty = true;
    }

    pub fn apply(&mut self, operation: EditOperation) -> Result<(), GlbError> {
        match operation {
            EditOperation::RotateRoots { axis, degrees } => {
                let rotation = rotation_matrix(axis, degrees.to_radians())?;
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
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("glb.tmp");
        fs::write(&temporary, bytes)?;
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(())
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
                    roots.extend(
                        nodes
                            .iter()
                            .filter_map(Value::as_u64)
                            .map(|index| index as usize),
                    );
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
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start
        {
            return Err(GlbError::Invalid(
                "Animation range must satisfy 0 <= start < end".to_owned(),
            ));
        }
        let animations = self
            .json
            .get("animations")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no animations".to_owned())
            })?;
        let animation = animations.get(animation_index).ok_or_else(|| {
            GlbError::Invalid(format!(
                "Animation {animation_index} does not exist"
            ))
        })?;
        let samplers = animation
            .get("samplers")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("Animation has no samplers".to_owned())
            })?;
        let mut ranges = Vec::with_capacity(samplers.len());
        for sampler in samplers {
            let input = sampler
                .get("input")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    GlbError::Invalid(
                        "Animation sampler has no input accessor".to_owned(),
                    )
                })? as usize;
            ranges.push(self.accessor_time_range(input, start, end)?);
        }
        let sampler_ids: Vec<(usize, usize)> = samplers
            .iter()
            .map(|sampler| {
                (
                    sampler.get("input").and_then(Value::as_u64).unwrap()
                        as usize,
                    sampler.get("output").and_then(Value::as_u64).unwrap()
                        as usize,
                )
            })
            .collect();
        let mut updates = Vec::with_capacity(sampler_ids.len());
        for ((input, output), (first, last)) in
            sampler_ids.into_iter().zip(ranges)
        {
            let input_accessor = self.accessor(input)?.clone();
            let output_accessor = self.accessor(output)?.clone();
            let input_view =
                self.copy_accessor_range(&input_accessor, first, last)?;
            let output_view =
                self.copy_accessor_range(&output_accessor, first, last)?;
            let new_input = self.append_accessor(
                input_accessor,
                input_view,
                last - first + 1,
            )?;
            let new_output = self.append_accessor(
                output_accessor,
                output_view,
                last - first + 1,
            )?;
            updates.push((new_input, new_output));
        }
        let animation = self
            .json
            .get_mut("animations")
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(animation_index))
            .unwrap();
        let samplers = animation
            .get_mut("samplers")
            .and_then(Value::as_array_mut)
            .unwrap();
        for (sampler, (new_input, new_output)) in
            samplers.iter_mut().zip(updates)
        {
            sampler["input"] = json!(new_input);
            sampler["output"] = json!(new_output);
        }
        Ok(())
    }

    fn accessor_time_range(
        &self,
        accessor: usize,
        start: f32,
        end: f32,
    ) -> Result<(usize, usize), GlbError> {
        let values = self.read_accessor_f32(accessor)?;
        let mut first = None;
        let mut last = None;
        for (index, value) in values.iter().enumerate() {
            let time = value[0];
            if time >= start && time <= end {
                first.get_or_insert(index);
                last = Some(index);
            }
        }
        match (first, last) {
            (Some(first), Some(last)) if first < last => Ok((first, last)),
            _ => Err(GlbError::Invalid(
                "Animation range does not contain at least two keyframes"
                    .to_owned(),
            )),
        }
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
            .unwrap_or_default() as usize;
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
            })? as usize;
        let view = self
            .json
            .get("bufferViews")
            .and_then(Value::as_array)
            .and_then(|items| items.get(view_index))
            .ok_or_else(|| {
                GlbError::Invalid(format!("Missing bufferView {view_index}"))
            })?;
        if view.get("byteStride").is_some() || accessor.get("sparse").is_some()
        {
            return Err(GlbError::Unsupported(
                "Strided and sparse animation accessors are not supported"
                    .to_owned(),
            ));
        }
        let offset = view
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize
            + accessor
                .get("byteOffset")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
        let length = accessor
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize
            * element_size;
        let bin = self.bin.as_deref().ok_or_else(|| {
            GlbError::Invalid("Animation requires a BIN chunk".to_owned())
        })?;
        bin.get(offset..offset + length)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                GlbError::Invalid(
                    "Animation accessor exceeds BIN chunk".to_owned(),
                )
            })
    }

    fn copy_accessor_range(
        &mut self,
        accessor: &Value,
        first: usize,
        last: usize,
    ) -> Result<Vec<u8>, GlbError> {
        let components = match accessor.get("type").and_then(Value::as_str) {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            _ => {
                return Err(GlbError::Unsupported(
                    "Animation output accessor type is unsupported".to_owned(),
                ))
            }
        };
        let component_size =
            match accessor.get("componentType").and_then(Value::as_u64) {
                Some(5126) => 4,
                Some(5123) | Some(5125) => 2,
                _ => {
                    return Err(GlbError::Unsupported(
                        "Animation component type is unsupported".to_owned(),
                    ))
                }
            };
        let bytes =
            self.accessor_bytes(accessor, components * component_size)?;
        let element_size = components * component_size;
        Ok(bytes[first * element_size..(last + 1) * element_size].to_vec())
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

    fn append_accessor(
        &mut self,
        template: Value,
        data: Vec<u8>,
        count: usize,
    ) -> Result<usize, GlbError> {
        let bin = self.bin.get_or_insert_with(Vec::new);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let offset = bin.len();
        bin.extend_from_slice(&data);
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
        let mut view = Map::new();
        view.insert("buffer".to_owned(), json!(0));
        view.insert("byteOffset".to_owned(), json!(offset));
        view.insert("byteLength".to_owned(), json!(data.len()));
        let view_index = views.len();
        views.push(Value::Object(view));
        let accessors = self
            .json
            .get_mut("accessors")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no accessors array".to_owned())
            })?;
        let mut accessor = template;
        accessor["bufferView"] = json!(view_index);
        accessor["byteOffset"] = json!(0);
        accessor["count"] = json!(count);
        if let Some(object) = accessor.as_object_mut() {
            object.remove("min");
            object.remove("max");
        }
        let index = accessors.len();
        accessors.push(accessor);
        if let Some(buffer) = self
            .json
            .get_mut("buffers")
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(0))
        {
            buffer["byteLength"] = json!(bin.len());
        }
        Ok(index)
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
