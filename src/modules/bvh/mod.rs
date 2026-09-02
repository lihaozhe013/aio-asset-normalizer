//! Generic BVH parsing, trimming, mapping, and retargeting contracts.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::glb::{AnimationChannelData, SkinData};

mod mapping;
pub use self::mapping::{
    load_mapping, save_mapping, MappingSuggestion, MappingValidation,
    SuggestionConfidence,
};

mod pose;

mod key_reduction;

#[derive(Debug, Clone)]
pub struct BvhDocument {
    pub source_path: Option<PathBuf>,
    pub joints: Vec<BvhJoint>,
    pub frames: Vec<Vec<f32>>,
    pub frame_time: f32,
}

#[derive(Debug, Clone)]
pub struct BvhJoint {
    pub name: String,
    pub parent: Option<usize>,
    pub offset: [f32; 3],
    pub channels: Vec<BvhChannel>,
    pub children: Vec<usize>,
    pub end_site: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BvhChannel {
    Xposition,
    Yposition,
    Zposition,
    Xrotation,
    Yrotation,
    Zrotation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingFile {
    pub schema_version: u32,
    pub source: MappingSource,
    pub target: MappingTarget,
    pub bones: Vec<BoneMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingSource {
    pub up_axis: String,
    pub forward_axis: String,
    pub unit: String,
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingTarget {
    pub skin: String,
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoneMapping {
    pub source_joint: String,
    pub target_node: String,
    #[serde(default = "identity_quaternion")]
    pub rotation_offset_xyzw: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct RetargetPlan {
    pub source_to_target: HashMap<usize, String>,
    pub unmapped_source_joints: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RetargetClip {
    pub name: String,
    pub times: Vec<f32>,
    pub channels: Vec<AnimationChannelData>,
}

#[derive(Debug)]
pub enum BvhError {
    Io(std::io::Error),
    Parse(String),
    Mapping(String),
}

impl std::fmt::Display for BvhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Parse(message) => write!(f, "BVH parse error: {message}"),
            Self::Mapping(message) => write!(f, "Mapping error: {message}"),
        }
    }
}

impl std::error::Error for BvhError {}

impl From<std::io::Error> for BvhError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl BvhDocument {
    pub fn parse(text: &str) -> Result<Self, BvhError> {
        let mut tokens = Tokenizer::new(text);
        tokens.expect("HIERARCHY")?;
        tokens.expect("ROOT")?;
        let root_name = tokens.next_required("root name")?;
        let mut joints = Vec::new();
        parse_joint(&mut tokens, &mut joints, root_name, None)?;
        let mut joint_names = HashSet::new();
        for joint in &joints {
            if !joint_names.insert(joint.name.as_str()) {
                return Err(BvhError::Parse(format!(
                    "duplicate joint name '{}'",
                    joint.name
                )));
            }
        }
        tokens.expect("MOTION")?;
        tokens.expect("Frames:")?;
        let frame_count = tokens.next_usize("frame count")?;
        tokens.expect("Frame")?;
        tokens.expect("Time:")?;
        let frame_time = tokens.next_f32("frame time")?;
        if !frame_time.is_finite() || frame_time <= 0.0 {
            return Err(BvhError::Parse(
                "frame time must be finite and greater than zero".to_owned(),
            ));
        }
        let channel_count =
            joints.iter().map(|joint| joint.channels.len()).sum();
        let mut frames = Vec::with_capacity(frame_count);
        for _ in 0..frame_count {
            let mut frame = Vec::with_capacity(channel_count);
            for _ in 0..channel_count {
                frame.push(tokens.next_f32("motion value")?);
            }
            frames.push(frame);
        }
        Ok(Self {
            source_path: None,
            joints,
            frames,
            frame_time,
        })
    }

    pub fn load(path: &Path) -> Result<Self, BvhError> {
        let text = fs::read_to_string(path)?;
        let mut document = Self::parse(&text)?;
        document.source_path = Some(path.to_path_buf());
        Ok(document)
    }

    pub fn duration(&self) -> f32 {
        self.frame_time * self.frames.len().saturating_sub(1) as f32
    }

    /// Return world transforms for one motion frame to the shared retargeter.
    ///
    /// This deliberately exposes a crate-local tuple instead of the parser's
    /// private transform type so BVH remains independent from GLB internals.
    pub(crate) fn frame_transforms_for_retarget(
        &self,
        frame: &[f32],
    ) -> Result<Vec<([f32; 3], [f32; 4], [f32; 3])>, BvhError> {
        Ok(self
            .frame_transforms(frame)?
            .into_iter()
            .map(|transform| {
                (transform.position, transform.rotation, transform.scale)
            })
            .collect())
    }

    /// Return the authored BVH Rest Pose (OFFSET plus identity rotations).
    ///
    /// A motion file's first frame is not guaranteed to be its Rest Pose and
    /// must never be used as the reference for retargeting.
    pub(crate) fn rest_transforms_for_retarget(
        &self,
    ) -> Result<Vec<([f32; 3], [f32; 4], [f32; 3])>, BvhError> {
        let zero_frame = vec![0.0; self.channel_count()];
        self.frame_transforms_for_retarget(&zero_frame)
    }

    pub fn trim(&mut self, start: f32, end: f32) -> Result<(), BvhError> {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start
        {
            return Err(BvhError::Parse(
                "trim range must satisfy 0 <= start < end".to_owned(),
            ));
        }
        let first = (start / self.frame_time).ceil() as usize;
        let last = (end / self.frame_time).floor() as usize;
        if first >= self.frames.len()
            || last >= self.frames.len()
            || first > last
        {
            return Err(BvhError::Parse(
                "trim range is outside the motion".to_owned(),
            ));
        }
        self.frames = self.frames[first..=last].to_vec();
        Ok(())
    }

    pub fn plan_retarget(
        &self,
        mapping: &MappingFile,
    ) -> Result<RetargetPlan, BvhError> {
        if mapping.schema_version != 1 {
            return Err(BvhError::Mapping(format!(
                "unsupported mapping schema version {}",
                mapping.schema_version
            )));
        }
        let source_lookup: HashMap<&str, usize> = self
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| (joint.name.as_str(), index))
            .collect();
        let mut source_to_target = HashMap::new();
        for bone in &mapping.bones {
            if let Some(&source) = source_lookup.get(bone.source_joint.as_str())
            {
                source_to_target.insert(source, bone.target_node.clone());
            }
        }
        let unmapped_source_joints = self
            .joints
            .iter()
            .enumerate()
            .filter(|(index, _)| !source_to_target.contains_key(index))
            .map(|(_, joint)| joint.name.clone())
            .collect();
        Ok(RetargetPlan {
            source_to_target,
            unmapped_source_joints,
        })
    }

    #[allow(dead_code)]
    pub fn retarget_to_skin(
        &self,
        mapping: &MappingFile,
        target: &SkinData,
    ) -> Result<RetargetClip, BvhError> {
        let source_file_sha256 = self
            .source_path
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .map(|bytes| crate::modules::retarget::sha256_hex(&bytes))
            .unwrap_or_default();
        let source_descriptor =
            crate::modules::retarget::SkeletonDescriptor::from_bvh(
                self,
                source_file_sha256,
                mapping.source.up_axis.clone(),
                mapping.source.forward_axis.clone(),
                mapping.source.unit.clone(),
            )
            .map_err(|error| BvhError::Mapping(error.to_string()))?;
        let target_descriptor =
            crate::modules::retarget::SkeletonDescriptor::from_skin(
                target,
                crate::modules::retarget::SourceKind::Glb,
                String::new(),
                String::new(),
                "Y",
                "-Z",
                "m",
                &HashSet::new(),
            )
            .map_err(|error| BvhError::Mapping(error.to_string()))?;
        let mapping_v2 = crate::modules::retarget::from_legacy_bvh_mapping(
            mapping,
            &source_descriptor,
            &target_descriptor,
        )
        .map_err(|error| BvhError::Mapping(error.to_string()))?;
        crate::modules::retarget::retarget_bvh(
            self,
            target,
            &mapping_v2,
            crate::modules::retarget::RetargetOptions::default(),
            "BVH Retarget",
        )
        .map_err(|error| BvhError::Mapping(error.to_string()))
    }

    fn frame_transforms(
        &self,
        frame: &[f32],
    ) -> Result<Vec<MotionTransform>, BvhError> {
        if frame.len() != self.channel_count() {
            return Err(BvhError::Parse(
                "motion frame channel count does not match the hierarchy"
                    .to_owned(),
            ));
        }
        let mut cursor = 0;
        let mut transforms: Vec<MotionTransform> =
            Vec::with_capacity(self.joints.len());
        for joint in &self.joints {
            let mut position = joint.offset;
            let mut rotation = identity_quaternion();
            for channel in &joint.channels {
                let value = frame[cursor];
                cursor += 1;
                match channel {
                    BvhChannel::Xposition => position[0] += value,
                    BvhChannel::Yposition => position[1] += value,
                    BvhChannel::Zposition => position[2] += value,
                    BvhChannel::Xrotation => {
                        rotation = quat_mul(
                            rotation,
                            axis_quaternion(
                                [1.0, 0.0, 0.0],
                                value.to_radians(),
                            ),
                        )
                    }
                    BvhChannel::Yrotation => {
                        rotation = quat_mul(
                            rotation,
                            axis_quaternion(
                                [0.0, 1.0, 0.0],
                                value.to_radians(),
                            ),
                        )
                    }
                    BvhChannel::Zrotation => {
                        rotation = quat_mul(
                            rotation,
                            axis_quaternion(
                                [0.0, 0.0, 1.0],
                                value.to_radians(),
                            ),
                        )
                    }
                }
            }
            let world = if let Some(parent) = joint.parent {
                let parent_transform =
                    transforms.get(parent).ok_or_else(|| {
                        BvhError::Parse(
                            "BVH joint order is not parent-first".to_owned(),
                        )
                    })?;
                MotionTransform {
                    position: add(
                        parent_transform.position,
                        quat_rotate(
                            parent_transform.rotation,
                            multiply_vec3(parent_transform.scale, position),
                        ),
                    ),
                    rotation: quat_normalize(quat_mul(
                        parent_transform.rotation,
                        rotation,
                    )),
                    scale: parent_transform.scale,
                }
            } else {
                MotionTransform {
                    position,
                    rotation: quat_normalize(rotation),
                    scale: [1.0, 1.0, 1.0],
                }
            };
            transforms.push(world);
        }
        Ok(transforms)
    }

    fn channel_count(&self) -> usize {
        self.joints.iter().map(|joint| joint.channels.len()).sum()
    }

    pub fn write(&self, path: &Path) -> Result<(), BvhError> {
        let mut output = String::new();
        output.push_str("HIERARCHY\n");
        let root = self
            .joints
            .iter()
            .position(|joint| joint.parent.is_none())
            .ok_or_else(|| {
                BvhError::Parse("BVH has no root joint".to_owned())
            })?;
        write_joint(&mut output, self, root, 0);
        output.push_str("MOTION\n");
        output.push_str(&format!("Frames: {}\n", self.frames.len()));
        output.push_str(&format!("Frame Time: {:.9}\n", self.frame_time));
        for frame in &self.frames {
            let values = frame
                .iter()
                .map(|value| format!("{value:.6}"))
                .collect::<Vec<_>>();
            output.push_str(&values.join(" "));
            output.push('\n');
        }
        let temporary = path.with_extension("bvh.tmp");
        fs::write(&temporary, output)?;
        if let Err(error) =
            crate::modules::atomic_file::replace(&temporary, path)
        {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct MotionTransform {
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

struct CoordinateBasis;

impl CoordinateBasis {
    fn from_mapping(
        up_axis: &str,
        forward_axis: &str,
    ) -> Result<Self, BvhError> {
        let up = parse_axis(up_axis).ok_or_else(|| {
            BvhError::Mapping(format!("unsupported source up axis '{up_axis}'"))
        })?;
        let forward = parse_axis(forward_axis).ok_or_else(|| {
            BvhError::Mapping(format!(
                "unsupported source forward axis '{forward_axis}'"
            ))
        })?;
        if dot(up, forward).abs() > f32::EPSILON {
            return Err(BvhError::Mapping(
                "source up and forward axes must be perpendicular".to_owned(),
            ));
        }
        if length(cross(forward, up)) <= f32::EPSILON {
            return Err(BvhError::Mapping(
                "source coordinate basis is degenerate".to_owned(),
            ));
        }
        Ok(Self)
    }
}

fn parse_axis(axis: &str) -> Option<[f32; 3]> {
    match axis.to_ascii_lowercase().replace(['_', ' '], "").as_str() {
        "x" | "+x" | "positivex" => Some([1.0, 0.0, 0.0]),
        "-x" | "negativex" => Some([-1.0, 0.0, 0.0]),
        "y" | "+y" | "positivey" => Some([0.0, 1.0, 0.0]),
        "-y" | "negativey" => Some([0.0, -1.0, 0.0]),
        "z" | "+z" | "positivez" => Some([0.0, 0.0, 1.0]),
        "-z" | "negativez" => Some([0.0, 0.0, -1.0]),
        _ => None,
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(value: [f32; 3]) -> f32 {
    dot(value, value).sqrt()
}

fn axis_quaternion(axis: [f32; 3], radians: f32) -> [f32; 4] {
    let half = radians * 0.5;
    let (sin, cos) = half.sin_cos();
    [axis[0] * sin, axis[1] * sin, axis[2] * sin, cos]
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn quat_normalize(q: [f32; 4]) -> [f32; 4] {
    let length = q.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length <= f32::EPSILON || !length.is_finite() {
        identity_quaternion()
    } else {
        q.map(|value| value / length)
    }
}

fn quat_rotate(q: [f32; 4], vector: [f32; 3]) -> [f32; 3] {
    let q = quat_normalize(q);
    let q_vector = [q[0], q[1], q[2]];
    let twice_cross = multiply_scalar(cross(q_vector, vector), 2.0);
    add(
        vector,
        add(
            multiply_scalar(twice_cross, q[3]),
            cross(q_vector, twice_cross),
        ),
    )
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn multiply_scalar(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn multiply_vec3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn identity_quaternion() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

struct Tokenizer<'a> {
    tokens: Vec<&'a str>,
    index: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            tokens: input.split_whitespace().collect(),
            index: 0,
        }
    }

    fn next_required(&mut self, label: &str) -> Result<String, BvhError> {
        self.tokens
            .get(self.index)
            .map(|token| {
                self.index += 1;
                (*token).to_owned()
            })
            .ok_or_else(|| BvhError::Parse(format!("missing {label}")))
    }

    fn expect(&mut self, expected: &str) -> Result<(), BvhError> {
        let actual = self.next_required(expected)?;
        if actual == expected {
            Ok(())
        } else {
            Err(BvhError::Parse(format!(
                "expected {expected}, found {actual}"
            )))
        }
    }

    fn next_f32(&mut self, label: &str) -> Result<f32, BvhError> {
        let token = self.next_required(label)?;
        let value = token.parse::<f32>().map_err(|_| {
            BvhError::Parse(format!("invalid {label}: {token}"))
        })?;
        if !value.is_finite() {
            return Err(BvhError::Parse(format!(
                "{label} must be finite: {token}"
            )));
        }
        Ok(value)
    }

    fn next_usize(&mut self, label: &str) -> Result<usize, BvhError> {
        let token = self.next_required(label)?;
        token
            .parse()
            .map_err(|_| BvhError::Parse(format!("invalid {label}: {token}")))
    }
}

fn parse_joint(
    tokens: &mut Tokenizer<'_>,
    joints: &mut Vec<BvhJoint>,
    name: String,
    parent: Option<usize>,
) -> Result<usize, BvhError> {
    tokens.expect("{")?;
    tokens.expect("OFFSET")?;
    let offset = [
        tokens.next_f32("x offset")?,
        tokens.next_f32("y offset")?,
        tokens.next_f32("z offset")?,
    ];
    tokens.expect("CHANNELS")?;
    let channel_count = tokens.next_usize("channel count")?;
    let mut channels = Vec::with_capacity(channel_count);
    for _ in 0..channel_count {
        channels.push(parse_channel(&tokens.next_required("channel")?)?);
    }
    let index = joints.len();
    joints.push(BvhJoint {
        name,
        parent,
        offset,
        channels,
        children: Vec::new(),
        end_site: None,
    });
    if let Some(parent_index) = parent {
        joints[parent_index].children.push(index);
    }
    loop {
        match tokens.next_required("joint or closing brace")?.as_str() {
            "JOINT" => {
                let child = tokens.next_required("joint name")?;
                parse_joint(tokens, joints, child, Some(index))?;
            }
            "End" => {
                tokens.expect("Site")?;
                tokens.expect("{")?;
                tokens.expect("OFFSET")?;
                let end_site = [
                    tokens.next_f32("end site x")?,
                    tokens.next_f32("end site y")?,
                    tokens.next_f32("end site z")?,
                ];
                tokens.expect("}")?;
                joints[index].end_site = Some(end_site);
            }
            "}" => return Ok(index),
            token => {
                return Err(BvhError::Parse(format!(
                    "unexpected hierarchy token {token}"
                )))
            }
        }
    }
}

fn parse_channel(channel: &str) -> Result<BvhChannel, BvhError> {
    match channel {
        "Xposition" => Ok(BvhChannel::Xposition),
        "Yposition" => Ok(BvhChannel::Yposition),
        "Zposition" => Ok(BvhChannel::Zposition),
        "Xrotation" => Ok(BvhChannel::Xrotation),
        "Yrotation" => Ok(BvhChannel::Yrotation),
        "Zrotation" => Ok(BvhChannel::Zrotation),
        other => Err(BvhError::Parse(format!("unsupported channel {other}"))),
    }
}

fn write_joint(
    output: &mut String,
    document: &BvhDocument,
    index: usize,
    depth: usize,
) {
    let joint = &document.joints[index];
    let indent = "  ".repeat(depth);
    let kind = if joint.parent.is_none() {
        "ROOT"
    } else {
        "JOINT"
    };
    output.push_str(&format!("{indent}{kind} {}\n{indent}{{\n", joint.name));
    output.push_str(&format!(
        "{}  OFFSET {:.6} {:.6} {:.6}\n",
        indent, joint.offset[0], joint.offset[1], joint.offset[2]
    ));
    output.push_str(&format!("{}  CHANNELS {}", indent, joint.channels.len()));
    for channel in &joint.channels {
        output.push_str(&format!(" {}", channel_name(*channel)));
    }
    output.push('\n');
    for &child in &joint.children {
        write_joint(output, document, child, depth + 1);
    }
    if let Some(offset) = joint.end_site {
        output.push_str(&format!(
            "{}  End Site\n{}  {{\n{}    OFFSET {:.6} {:.6} {:.6}\n{}  }}\n",
            indent, indent, indent, offset[0], offset[1], offset[2], indent
        ));
    }
    output.push_str(&format!("{indent}}}\n"));
}

fn channel_name(channel: BvhChannel) -> &'static str {
    match channel {
        BvhChannel::Xposition => "Xposition",
        BvhChannel::Yposition => "Yposition",
        BvhChannel::Zposition => "Zposition",
        BvhChannel::Xrotation => "Xrotation",
        BvhChannel::Yrotation => "Yrotation",
        BvhChannel::Zrotation => "Zrotation",
    }
}

#[cfg(test)]
mod tests;
