//! Shared, format-neutral skeleton retargeting and Agent Mapping support.
//!
//! BVH and GLB are deliberately kept at the edges of this module.  Once an
//! input is reduced to a rest skeleton and a sequence of world-space poses,
//! the same validation, coordinate conversion, and target-local animation
//! generation code serves both workflows.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::modules::bvh::{BvhDocument, MappingFile};
use crate::modules::glb::{
    AnimationChannelData, AnimationRuntime, GlbDocument, SkinData,
};

#[path = "retarget_prompt.rs"]
mod retarget_prompt;
pub use self::retarget_prompt::{
    build_agent_prompt, build_bvh_agent_prompt, save_agent_prompt,
};

pub const MAPPING_SCHEMA: &str = "com.aio-asset-normalizer.skeleton-mapping";
pub const MAPPING_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Bvh,
    Glb,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeRef {
    pub node: String,
    pub path: Vec<String>,
    pub index: usize,
}

impl NodeRef {
    pub fn new(
        name: impl Into<String>,
        path: Vec<String>,
        index: usize,
    ) -> Self {
        Self {
            node: name.into(),
            path,
            index,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinRef {
    pub index: usize,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingEndpoint {
    pub kind: SourceKind,
    #[serde(default)]
    pub file_sha256: String,
    #[serde(default)]
    pub skeleton_sha256: String,
    #[serde(default)]
    pub skin: Option<SkinRef>,
    pub root: NodeRef,
    pub up_axis: String,
    pub forward_axis: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingBone {
    pub source: NodeRef,
    pub target: NodeRef,
    #[serde(default = "identity_quaternion")]
    pub rotation_offset_xyzw: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RootMotionMapping {
    pub source: NodeRef,
    pub target: NodeRef,
    #[serde(default = "one")]
    pub translation_scale: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkeletonMapping {
    pub schema: String,
    pub version: u32,
    pub source: MappingEndpoint,
    pub target: MappingEndpoint,
    #[serde(default)]
    pub bones: Vec<MappingBone>,
    #[serde(default)]
    pub ignored_sources: Vec<NodeRef>,
    #[serde(default)]
    pub root_motion: Option<RootMotionMapping>,
}

impl SkeletonMapping {
    pub fn new(source: MappingEndpoint, target: MappingEndpoint) -> Self {
        Self {
            schema: MAPPING_SCHEMA.to_owned(),
            version: MAPPING_VERSION,
            source,
            target,
            bones: Vec::new(),
            ignored_sources: Vec::new(),
            root_motion: None,
        }
    }

    pub fn validate_schema(&self) -> Result<(), RetargetError> {
        if self.schema != MAPPING_SCHEMA {
            return Err(RetargetError::Mapping(format!(
                "unsupported mapping schema '{}', expected '{}'",
                self.schema, MAPPING_SCHEMA
            )));
        }
        if self.version != MAPPING_VERSION {
            return Err(RetargetError::Mapping(format!(
                "unsupported mapping version {}, expected {}",
                self.version, MAPPING_VERSION
            )));
        }
        if self.bones.is_empty() {
            return Err(RetargetError::Mapping(
                "mapping contains no bones".to_owned(),
            ));
        }
        validate_endpoint(&self.source, "source")?;
        validate_endpoint(&self.target, "target")?;
        if self.target.kind != SourceKind::Glb {
            return Err(RetargetError::Mapping(
                "mapping target must be a GLB skeleton".to_owned(),
            ));
        }
        if self.source.kind == SourceKind::Glb && self.source.skin.is_none() {
            return Err(RetargetError::Mapping(
                "GLB mapping sources must identify a selected Skin".to_owned(),
            ));
        }
        if self.source.kind == SourceKind::Bvh && self.source.skin.is_some() {
            return Err(RetargetError::Mapping(
                "BVH mapping sources cannot identify a GLB Skin".to_owned(),
            ));
        }
        if self.target.skin.is_none() {
            return Err(RetargetError::Mapping(
                "GLB mapping targets must identify a selected Skin".to_owned(),
            ));
        }
        for (endpoint, label) in
            [(&self.source, "source"), (&self.target, "target")]
        {
            if let Some(skin) = endpoint.skin.as_ref() {
                if skin.name.trim().is_empty() {
                    return Err(RetargetError::Mapping(format!(
                        "{label} selected Skin name is required"
                    )));
                }
            }
        }
        validate_node_reference(&self.source.root, "source root")?;
        validate_node_reference(&self.target.root, "target root")?;
        for bone in &self.bones {
            validate_node_reference(&bone.source, "source bone")?;
            validate_node_reference(&bone.target, "target bone")?;
            let offset_length = bone
                .rotation_offset_xyzw
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                .sqrt();
            if bone
                .rotation_offset_xyzw
                .iter()
                .any(|value| !value.is_finite())
                || !offset_length.is_finite()
                || offset_length <= f32::EPSILON
            {
                return Err(RetargetError::Mapping(format!(
                    "rotation offset for '{}' is not finite",
                    bone.source.node
                )));
            }
        }
        for ignored in &self.ignored_sources {
            validate_node_reference(ignored, "ignored source")?;
        }
        if let Some(root_motion) = &self.root_motion {
            validate_node_reference(&root_motion.source, "root motion source")?;
            validate_node_reference(&root_motion.target, "root motion target")?;
        }
        if let Some(root_motion) = &self.root_motion {
            if !root_motion.translation_scale.is_finite()
                || root_motion.translation_scale <= 0.0
            {
                return Err(RetargetError::Mapping(
                    "root motion translation scale must be finite and greater than zero"
                        .to_owned(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SkeletonNode {
    pub index: usize,
    pub name: String,
    pub path: Vec<String>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub is_skin_joint: bool,
    pub animated: bool,
}

#[derive(Debug, Clone)]
pub struct SkeletonDescriptor {
    pub kind: SourceKind,
    pub file_sha256: String,
    pub skeleton_sha256: String,
    pub skin: Option<SkinRef>,
    pub root: usize,
    pub up_axis: String,
    pub forward_axis: String,
    pub unit: String,
    pub nodes: Vec<SkeletonNode>,
    pub animated_nodes: Vec<usize>,
    /// Nodes that instance the selected Skin.  BVH sources have no mesh
    /// relationship and therefore keep this list empty.
    pub mesh_nodes: Vec<usize>,
}

impl SkeletonDescriptor {
    #[allow(dead_code)]
    pub fn endpoint(&self) -> MappingEndpoint {
        MappingEndpoint {
            kind: self.kind,
            file_sha256: self.file_sha256.clone(),
            skeleton_sha256: self.skeleton_sha256.clone(),
            skin: self.skin.clone(),
            root: self.node_ref(self.root),
            up_axis: self.up_axis.clone(),
            forward_axis: self.forward_axis.clone(),
            unit: self.unit.clone(),
        }
    }

    pub fn node_ref(&self, index: usize) -> NodeRef {
        let node = &self.nodes[index];
        NodeRef::new(node.name.clone(), node.path.clone(), node.index)
    }

    pub fn from_skin(
        skin: &SkinData,
        kind: SourceKind,
        file_sha256: impl Into<String>,
        skeleton_sha256: impl Into<String>,
        up_axis: impl Into<String>,
        forward_axis: impl Into<String>,
        unit: impl Into<String>,
        animated_nodes: &HashSet<usize>,
    ) -> Result<Self, RetargetError> {
        let mut nodes = Vec::with_capacity(skin.nodes.len());
        let mut children = vec![Vec::new(); skin.nodes.len()];
        for (position, node) in skin.nodes.iter().enumerate() {
            if node.index != position {
                return Err(RetargetError::Mapping(format!(
                    "Skin node index {} does not match its array position {}",
                    node.index, position
                )));
            }
            if !node.translation.iter().all(|value| value.is_finite())
                || !node.rotation.iter().all(|value| value.is_finite())
                || !node.scale.iter().all(|value| value.is_finite())
                || node.name.trim().is_empty()
            {
                return Err(RetargetError::Mapping(format!(
                    "Skin node {} contains invalid name or non-finite TRS values",
                    node.index
                )));
            }
            if let Some(parent) = node.parent {
                if parent >= skin.nodes.len() {
                    return Err(RetargetError::Mapping(format!(
                        "node {} has invalid parent {}",
                        node.index, parent
                    )));
                }
                children[parent].push(node.index);
            }
        }
        let mut unique_joints = HashSet::new();
        if skin.joints.iter().any(|index| {
            *index >= skin.nodes.len() || !unique_joints.insert(*index)
        }) {
            return Err(RetargetError::Mapping(
                "Skin joints must be unique and within the node array"
                    .to_owned(),
            ));
        }
        if skin
            .mesh_nodes
            .iter()
            .any(|index| *index >= skin.nodes.len())
        {
            return Err(RetargetError::Mapping(
                "Skin mesh node index exceeds the node array".to_owned(),
            ));
        }
        let root = skin
            .skeleton
            .filter(|index| skin.joints.contains(index))
            .or_else(|| {
                skin.joints.iter().copied().find(|index| {
                    let mut current =
                        skin.nodes.get(*index).and_then(|node| node.parent);
                    while let Some(parent) = current {
                        if skin.joints.contains(&parent) {
                            return false;
                        }
                        current =
                            skin.nodes.get(parent).and_then(|node| node.parent);
                    }
                    true
                })
            })
            .or_else(|| skin.skeleton)
            .ok_or_else(|| {
                RetargetError::Mapping("skeleton has no root node".to_owned())
            })?;
        if root >= skin.nodes.len() {
            return Err(RetargetError::Mapping(
                "Skin skeleton root index exceeds the node array".to_owned(),
            ));
        }
        for node in &skin.nodes {
            let path = node_path(&skin.nodes, node.index)?;
            nodes.push(SkeletonNode {
                index: node.index,
                name: node.name.clone(),
                path,
                parent: node.parent,
                children: children[node.index].clone(),
                translation: node.translation,
                rotation: normalize_quaternion(node.rotation),
                scale: node.scale,
                is_skin_joint: skin.joints.contains(&node.index),
                animated: animated_nodes.contains(&node.index),
            });
        }
        let computed_skeleton_hash = skeleton_hash(
            kind,
            Some(&SkinRef {
                index: skin.index,
                name: skin.name.clone(),
            }),
            &nodes,
        );
        let supplied_skeleton_hash = skeleton_sha256.into();
        if !supplied_skeleton_hash.is_empty()
            && !supplied_skeleton_hash
                .eq_ignore_ascii_case(&computed_skeleton_hash)
        {
            return Err(RetargetError::Mapping(
                "provided Skin skeleton fingerprint does not match the computed skeleton"
                    .to_owned(),
            ));
        }
        let mut descriptor = Self {
            kind,
            file_sha256: file_sha256.into(),
            skeleton_sha256: computed_skeleton_hash,
            skin: Some(SkinRef {
                index: skin.index,
                name: skin.name.clone(),
            }),
            root,
            up_axis: up_axis.into(),
            forward_axis: forward_axis.into(),
            unit: unit.into(),
            nodes,
            animated_nodes: animated_nodes.iter().copied().collect(),
            mesh_nodes: skin.mesh_nodes.clone(),
        };
        descriptor.animated_nodes.sort_unstable();
        Ok(descriptor)
    }

    pub fn from_bvh(
        document: &BvhDocument,
        file_sha256: impl Into<String>,
        up_axis: impl Into<String>,
        forward_axis: impl Into<String>,
        unit: impl Into<String>,
    ) -> Result<Self, RetargetError> {
        let mut nodes = Vec::with_capacity(document.joints.len());
        for (index, joint) in document.joints.iter().enumerate() {
            nodes.push(SkeletonNode {
                index,
                name: joint.name.clone(),
                path: bvh_node_path(document, index)?,
                parent: joint.parent,
                children: joint.children.clone(),
                translation: joint.offset,
                rotation: identity_quaternion(),
                scale: [1.0; 3],
                is_skin_joint: true,
                animated: !joint.channels.is_empty(),
            });
        }
        let root = document
            .joints
            .iter()
            .position(|joint| joint.parent.is_none())
            .ok_or_else(|| {
                RetargetError::Mapping("BVH has no root".to_owned())
            })?;
        let animated_nodes = nodes
            .iter()
            .filter(|node| node.animated)
            .map(|node| node.index)
            .collect::<Vec<_>>();
        let skeleton_sha256 = skeleton_hash(SourceKind::Bvh, None, &nodes);
        Ok(Self {
            kind: SourceKind::Bvh,
            file_sha256: file_sha256.into(),
            skeleton_sha256,
            skin: None,
            root,
            up_axis: up_axis.into(),
            forward_axis: forward_axis.into(),
            unit: unit.into(),
            nodes,
            animated_nodes,
            mesh_nodes: Vec::new(),
        })
    }

    pub fn refresh_skeleton_hash(&mut self) {
        self.skeleton_sha256 =
            skeleton_hash(self.kind, self.skin.as_ref(), &self.nodes);
    }

    pub fn rest_world_transforms(
        &self,
    ) -> Result<Vec<([f32; 3], [f32; 4], [f32; 3])>, RetargetError> {
        target_rest_world(self).map(|values| {
            values
                .into_iter()
                .map(|value| (value.position, value.rotation, value.scale))
                .collect()
        })
    }

    pub fn context_value(&self) -> Value {
        let world = target_rest_world(self).unwrap_or_default();
        let nodes = self
            .nodes
            .iter()
            .map(|node| {
                json!({
                    "index": node.index,
                    "name": node.name,
                    "path": node.path,
                    "parent": node.parent.and_then(|index| self.nodes.get(index).map(|parent| json!({
                        "index": index,
                        "name": parent.name,
                    }))),
                    "children": node.children.iter().map(|index| json!({
                        "index": index,
                        "name": self.nodes.get(*index).map(|child| child.name.clone()).unwrap_or_default(),
                    })).collect::<Vec<_>>(),
                    "is_skin_joint": node.is_skin_joint,
                    "animated": node.animated,
                    "local_translation": finite_array(node.translation),
                     "local_rotation_xyzw": finite_array(node.rotation),
                     "local_scale": finite_array(node.scale),
                     "world_translation": world.get(node.index).map(|value| finite_array(value.position)).unwrap_or(Value::Null),
                     "world_rotation_xyzw": world.get(node.index).map(|value| finite_array(value.rotation)).unwrap_or(Value::Null),
                     "world_scale": world.get(node.index).map(|value| finite_array(value.scale)).unwrap_or(Value::Null),
                     "parent_distance": node.parent.and_then(|parent| {
                         world.get(node.index).zip(world.get(parent)).map(|(child, parent)| {
                             let delta = sub(child.position, parent.position);
                             length(delta)
                         })
                     }),
                 })
            })
            .collect::<Vec<_>>();
        let mesh_nodes = self
            .mesh_nodes
            .iter()
            .filter_map(|index| {
                self.nodes.get(*index).map(|_| self.node_ref(*index))
            })
            .collect::<Vec<_>>();
        let skin_joints = self
            .nodes
            .iter()
            .filter(|node| node.is_skin_joint)
            .map(|node| self.node_ref(node.index))
            .collect::<Vec<_>>();
        let root = self
            .nodes
            .get(self.root)
            .map(|_| self.node_ref(self.root))
            .map_or(Value::Null, |reference| json!(reference));
        json!({
            "kind": self.kind,
            "file_sha256": self.file_sha256,
            "skeleton_sha256": self.skeleton_sha256,
            "skin": self.skin,
            "root": root,
            "up_axis": self.up_axis,
            "forward_axis": self.forward_axis,
            "unit": self.unit,
            "nodes": nodes,
            "animated_nodes": self.animated_nodes,
            "skin_joints": skin_joints,
            "mesh_nodes": mesh_nodes,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct MappingValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub mapped_count: usize,
    pub unmapped_source_nodes: Vec<String>,
}

impl MappingValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty() && self.mapped_count > 0
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedBone {
    pub source: usize,
    pub target: usize,
    pub rotation_offset: [f32; 4],
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ResolvedMapping {
    pub bones: Vec<ResolvedBone>,
    pub ignored_sources: HashSet<usize>,
    pub source_root: usize,
    pub target_root: usize,
    pub root_motion: Option<(usize, usize, f32)>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct RetargetOptions {
    pub root_motion: bool,
    pub normalize_initial_heading: bool,
    pub sample_rate: f32,
    /// Optional uniform transform applied to sampled source world poses.
    /// GLB editor root controls use this instead of writing a matrix onto a
    /// node that may also have animated TRS channels.
    pub source_root_rotation: [f32; 4],
    pub source_root_scale: f32,
    pub source_root_translation: [f32; 3],
}

impl Default for RetargetOptions {
    fn default() -> Self {
        Self {
            root_motion: true,
            normalize_initial_heading: false,
            sample_rate: 60.0,
            source_root_rotation: identity_quaternion(),
            source_root_scale: 1.0,
            source_root_translation: [0.0; 3],
        }
    }
}

#[derive(Debug)]
pub enum RetargetError {
    Io(std::io::Error),
    Mapping(String),
    Source(String),
    Target(String),
    Unsupported(String),
}

impl std::fmt::Display for RetargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Mapping(message) => write!(f, "Mapping error: {message}"),
            Self::Source(message) => {
                write!(f, "Source animation error: {message}")
            }
            Self::Target(message) => {
                write!(f, "Target skeleton error: {message}")
            }
            Self::Unsupported(message) => {
                write!(f, "Unsupported retargeting: {message}")
            }
        }
    }
}

impl std::error::Error for RetargetError {}

impl From<std::io::Error> for RetargetError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn load_mapping(path: &Path) -> Result<SkeletonMapping, RetargetError> {
    let text = fs::read_to_string(path)?;
    let mapping: SkeletonMapping = serde_json::from_str(&text)
        .map_err(|error| RetargetError::Mapping(error.to_string()))?;
    mapping.validate_schema()?;
    Ok(mapping)
}

pub fn save_mapping(
    path: &Path,
    mapping: &SkeletonMapping,
) -> Result<(), RetargetError> {
    mapping.validate_schema()?;
    let bytes = serde_json::to_vec_pretty(mapping)
        .map_err(|error| RetargetError::Mapping(error.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    if let Err(error) = crate::modules::atomic_file::replace(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(RetargetError::Io(error));
    }
    Ok(())
}

pub fn validate_mapping(
    mapping: &SkeletonMapping,
    source: &SkeletonDescriptor,
    target: &SkeletonDescriptor,
) -> MappingValidationReport {
    let mut report = MappingValidationReport::default();
    if let Err(error) = mapping.validate_schema() {
        report.errors.push(error.to_string());
        return report;
    }
    if mapping.source.kind != source.kind {
        report.errors.push(format!(
            "mapping source kind {:?} does not match {:?}",
            mapping.source.kind, source.kind
        ));
    }
    if mapping.target.kind != target.kind {
        report.errors.push(format!(
            "mapping target kind {:?} does not match {:?}",
            mapping.target.kind, target.kind
        ));
    }
    validate_identity(&mapping.source, source, "source", &mut report);
    validate_identity(&mapping.target, target, "target", &mut report);
    let source_root =
        resolve_node(&mapping.source.root, source, "source", &mut report);
    let target_root =
        resolve_node(&mapping.target.root, target, "target", &mut report);
    let mut source_assignments = HashSet::new();
    let mut target_assignments = HashSet::new();
    let mut mapped_sources = HashSet::new();
    for bone in &mapping.bones {
        let source_index =
            resolve_node(&bone.source, source, "source", &mut report);
        let target_index =
            resolve_node(&bone.target, target, "target", &mut report);
        if let Some(index) = source_index {
            if !source_assignments.insert(index) {
                report.errors.push(format!(
                    "source node '{}' is mapped more than once",
                    bone.source.node
                ));
            }
            mapped_sources.insert(index);
            if source.kind == SourceKind::Glb
                && !source.nodes[index].is_skin_joint
            {
                report.errors.push(format!(
                    "source node '{}' is not in the selected Skin",
                    bone.source.node
                ));
            }
        }
        if let Some(index) = target_index {
            if !target_assignments.insert(index) {
                report.errors.push(format!(
                    "target node '{}' is mapped more than once",
                    bone.target.node
                ));
            }
            if target.kind == SourceKind::Glb
                && !target.nodes[index].is_skin_joint
            {
                report.errors.push(format!(
                    "target node '{}' is not in the selected Skin",
                    bone.target.node
                ));
            }
        }
    }
    let mut ignored_sources = HashSet::new();
    for reference in &mapping.ignored_sources {
        if let Some(index) =
            resolve_node(reference, source, "source", &mut report)
        {
            if !ignored_sources.insert(index) {
                report.errors.push(format!(
                    "source node '{}' is ignored more than once",
                    reference.node
                ));
            }
            if mapped_sources.contains(&index) {
                report.errors.push(format!(
                    "source node '{}' is both mapped and ignored",
                    reference.node
                ));
            }
        }
    }
    for index in &source.animated_nodes {
        let Some(node) = source.nodes.get(*index) else {
            report.errors.push(format!(
                "animated source node index {index} is outside the skeleton"
            ));
            continue;
        };
        if !mapped_sources.contains(index) && !ignored_sources.contains(index) {
            report.unmapped_source_nodes.push(node.name.clone());
        }
    }
    if !report.unmapped_source_nodes.is_empty() {
        report.errors.push(format!(
            "animated source nodes are neither mapped nor ignored: {}",
            report.unmapped_source_nodes.join(", ")
        ));
    }
    if let (Some(source_root), Some(target_root)) = (source_root, target_root) {
        if !is_skeleton_root(source, source_root) {
            report.errors.push(
                "source root reference is not a hierarchy root".to_owned(),
            );
        }
        if !is_skeleton_root(target, target_root) {
            report.errors.push(
                "target root reference is not a hierarchy root".to_owned(),
            );
        }
        if target.kind == SourceKind::Glb
            && !target.nodes[target_root].is_skin_joint
        {
            report
                .errors
                .push("target root is not in the selected Skin".to_owned());
        }
        if !mapped_sources.contains(&source_root) {
            report.errors.push("source root is not mapped".to_owned());
        }
        if !target_assignments.contains(&target_root) {
            report.warnings.push("target root is not a mapped bone; root motion may still target it".to_owned());
        }
    }
    if let Some(root_motion) = &mapping.root_motion {
        let source_index = resolve_node(
            &root_motion.source,
            source,
            "root motion source",
            &mut report,
        );
        if let Some(source_index) = source_index {
            if source.kind == SourceKind::Glb
                && !source.nodes[source_index].is_skin_joint
            {
                report.errors.push(
                    "root motion source is not in the selected Skin".to_owned(),
                );
            }
        }
        let target_index = resolve_node(
            &root_motion.target,
            target,
            "root motion target",
            &mut report,
        );
        if let Some(target_index) = target_index {
            if target.kind == SourceKind::Glb
                && !target.nodes[target_index].is_skin_joint
            {
                report.errors.push(
                    "root motion target is not in the selected Skin".to_owned(),
                );
            }
        }
    }
    for left in &mapping.bones {
        let Some(left_source) = resolve_node_no_report(&left.source, source)
        else {
            continue;
        };
        let Some(left_target) = resolve_node_no_report(&left.target, target)
        else {
            continue;
        };
        for right in &mapping.bones {
            let Some(right_source) =
                resolve_node_no_report(&right.source, source)
            else {
                continue;
            };
            let Some(right_target) =
                resolve_node_no_report(&right.target, target)
            else {
                continue;
            };
            if left_source != right_source
                && is_ancestor(source, left_source, right_source)
                && !is_ancestor(target, left_target, right_target)
            {
                report.errors.push(format!(
                    "mapping reverses hierarchy order for '{}' and '{}'",
                    left.source.node, right.source.node
                ));
            }
        }
    }
    report.mapped_count = mapped_sources.len();
    report
}

pub fn resolve_mapping(
    mapping: &SkeletonMapping,
    source: &SkeletonDescriptor,
    target: &SkeletonDescriptor,
) -> Result<ResolvedMapping, RetargetError> {
    let report = validate_mapping(mapping, source, target);
    if !report.is_valid() {
        return Err(RetargetError::Mapping(format!(
            "mapping validation failed: {}",
            report.errors.join("; ")
        )));
    }
    let source_root =
        resolve_node_required(&mapping.source.root, source, "source")?;
    let target_root =
        resolve_node_required(&mapping.target.root, target, "target")?;
    let bones = mapping
        .bones
        .iter()
        .map(|bone| {
            Ok(ResolvedBone {
                source: resolve_node_required(&bone.source, source, "source")?,
                target: resolve_node_required(&bone.target, target, "target")?,
                rotation_offset: normalize_quaternion(
                    bone.rotation_offset_xyzw,
                ),
            })
        })
        .collect::<Result<Vec<_>, RetargetError>>()?;
    let ignored_sources = mapping
        .ignored_sources
        .iter()
        .map(|reference| resolve_node_required(reference, source, "source"))
        .collect::<Result<HashSet<_>, _>>()?;
    let root_motion = mapping
        .root_motion
        .as_ref()
        .map(|root| {
            Ok::<(usize, usize, f32), RetargetError>((
                resolve_node_required(&root.source, source, "source")?,
                resolve_node_required(&root.target, target, "target")?,
                root.translation_scale,
            ))
        })
        .transpose()?;
    Ok(ResolvedMapping {
        bones,
        ignored_sources,
        source_root,
        target_root,
        root_motion,
        warnings: report.warnings,
    })
}

pub fn from_legacy_bvh_mapping(
    mapping: &MappingFile,
    source: &SkeletonDescriptor,
    target: &SkeletonDescriptor,
) -> Result<SkeletonMapping, RetargetError> {
    if mapping.schema_version != 1 {
        return Err(RetargetError::Mapping(format!(
            "unsupported legacy mapping version {}",
            mapping.schema_version
        )));
    }
    if target
        .skin
        .as_ref()
        .is_none_or(|skin| skin.name != mapping.target.skin)
    {
        return Err(RetargetError::Mapping(format!(
            "legacy mapping targets Skin '{}', but the selected Skin is '{}'",
            mapping.target.skin,
            target
                .skin
                .as_ref()
                .map(|skin| skin.name.as_str())
                .unwrap_or("unknown")
        )));
    }
    let source_lookup = unique_name_lookup(source);
    let target_lookup = unique_name_lookup(target);
    let source_root = source_lookup
        .get(&mapping.source.root)
        .copied()
        .ok_or_else(|| {
            RetargetError::Mapping("legacy source root not found".to_owned())
        })?;
    let target_root = target_lookup
        .get(&mapping.target.root)
        .copied()
        .ok_or_else(|| {
            RetargetError::Mapping("legacy target root not found".to_owned())
        })?;
    let mut converted = SkeletonMapping::new(
        MappingEndpoint {
            kind: SourceKind::Bvh,
            file_sha256: source.file_sha256.clone(),
            skeleton_sha256: source.skeleton_sha256.clone(),
            skin: None,
            root: source.node_ref(source_root),
            up_axis: mapping.source.up_axis.clone(),
            forward_axis: mapping.source.forward_axis.clone(),
            unit: mapping.source.unit.clone(),
        },
        MappingEndpoint {
            kind: SourceKind::Glb,
            file_sha256: target.file_sha256.clone(),
            skeleton_sha256: target.skeleton_sha256.clone(),
            skin: target.skin.clone(),
            root: target.node_ref(target_root),
            up_axis: "Y".to_owned(),
            forward_axis: "-Z".to_owned(),
            unit: "m".to_owned(),
        },
    );
    converted.bones = mapping
        .bones
        .iter()
        .map(|bone| {
            let source_index = source_lookup
                .get(&bone.source_joint)
                .copied()
                .ok_or_else(|| {
                RetargetError::Mapping(format!(
                    "legacy source joint '{}' is ambiguous or missing",
                    bone.source_joint
                ))
            })?;
            let target_index = target_lookup
                .get(&bone.target_node)
                .copied()
                .ok_or_else(|| {
                    RetargetError::Mapping(format!(
                        "legacy target node '{}' is ambiguous or missing",
                        bone.target_node
                    ))
                })?;
            Ok(MappingBone {
                source: source.node_ref(source_index),
                target: target.node_ref(target_index),
                rotation_offset_xyzw: bone.rotation_offset_xyzw,
            })
        })
        .collect::<Result<Vec<_>, RetargetError>>()?;
    let mapped = converted
        .bones
        .iter()
        .filter_map(|bone| resolve_node_no_report(&bone.source, source))
        .collect::<HashSet<_>>();
    converted.ignored_sources = source
        .animated_nodes
        .iter()
        .filter(|index| !mapped.contains(index))
        .map(|index| source.node_ref(*index))
        .collect();
    converted.root_motion = Some(RootMotionMapping {
        source: source.node_ref(source_root),
        target: target.node_ref(target_root),
        translation_scale: 1.0,
    });
    Ok(converted)
}

#[derive(Debug, Clone, Copy)]
struct Transform {
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

pub fn retarget_bvh(
    source: &BvhDocument,
    target: &SkinData,
    mapping: &SkeletonMapping,
    options: RetargetOptions,
    name: impl Into<String>,
) -> Result<crate::modules::bvh::RetargetClip, RetargetError> {
    let source_file_sha256 = source
        .source_path
        .as_deref()
        .and_then(|path| fs::read(path).ok())
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_default();
    let source_descriptor = SkeletonDescriptor::from_bvh(
        source,
        source_file_sha256,
        mapping.source.up_axis.clone(),
        mapping.source.forward_axis.clone(),
        mapping.source.unit.clone(),
    )?;
    let target_descriptor = SkeletonDescriptor::from_skin(
        target,
        SourceKind::Glb,
        String::new(),
        String::new(),
        mapping.target.up_axis.clone(),
        mapping.target.forward_axis.clone(),
        mapping.target.unit.clone(),
        &HashSet::new(),
    )?;
    let resolved =
        resolve_mapping(mapping, &source_descriptor, &target_descriptor)?;
    let source_rest = bvh_rest_transforms(source)?;
    let source_frames = source
        .frames
        .iter()
        .map(|frame| bvh_frame_transforms(source, frame))
        .collect::<Result<Vec<_>, _>>()?;
    let times = (0..source_frames.len())
        .map(|index| index as f32 * source.frame_time)
        .collect::<Vec<_>>();
    let frames = source_frames
        .into_iter()
        .map(|frame| frame.into_iter().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let rest = source_rest.into_iter().collect::<Vec<_>>();
    retarget_frames(
        &source_descriptor,
        &target_descriptor,
        &resolved,
        &rest,
        &frames,
        &times,
        options,
        name,
    )
}

pub fn retarget_glb(
    source_runtime: &AnimationRuntime,
    source_document: &GlbDocument,
    source_clip_index: usize,
    target: &SkinData,
    mapping: &SkeletonMapping,
    options: RetargetOptions,
    name: impl Into<String>,
) -> Result<crate::modules::bvh::RetargetClip, RetargetError> {
    if source_clip_index >= source_runtime.clips.len() {
        return Err(RetargetError::Source(format!(
            "animation {} does not exist",
            source_clip_index
        )));
    }
    let clip = &source_runtime.clips[source_clip_index];
    if !clip.is_playable() {
        return Err(RetargetError::Unsupported(clip.unsupported.join(", ")));
    }
    let valid_times = source_runtime
        .keyframe_times(source_clip_index)
        .map_err(|error| RetargetError::Source(error.to_string()))?;
    if valid_times.len() < 2 || clip.duration <= f32::EPSILON {
        return Err(RetargetError::Unsupported(
            "selected animation must contain at least two distinct samples"
                .to_owned(),
        ));
    }
    if !options.sample_rate.is_finite() || options.sample_rate <= 0.0 {
        return Err(RetargetError::Mapping(
            "sample rate must be finite and greater than zero".to_owned(),
        ));
    }
    let animated_nodes = clip
        .channels
        .iter()
        .map(|channel| channel.node)
        .collect::<HashSet<_>>();
    let source_file_sha256 = source_document
        .to_bytes()
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_default();
    let mut source_descriptor = SkeletonDescriptor::from_runtime(
        source_runtime,
        source_document,
        mapping
            .source
            .skin
            .as_ref()
            .map(|skin| skin.index)
            .unwrap_or(0),
        &animated_nodes,
        source_file_sha256,
        mapping.source.up_axis.clone(),
        mapping.source.forward_axis.clone(),
        mapping.source.unit.clone(),
    )?;
    source_descriptor.refresh_skeleton_hash();
    let target_descriptor = SkeletonDescriptor::from_skin(
        target,
        SourceKind::Glb,
        String::new(),
        String::new(),
        mapping.target.up_axis.clone(),
        mapping.target.forward_axis.clone(),
        mapping.target.unit.clone(),
        &HashSet::new(),
    )?;
    let resolved =
        resolve_mapping(mapping, &source_descriptor, &target_descriptor)?;
    let clip_start = *valid_times.first().ok_or_else(|| {
        RetargetError::Unsupported(
            "selected animation has no finite keyframe times".to_owned(),
        )
    })?;
    let clip_end = *valid_times.last().ok_or_else(|| {
        RetargetError::Unsupported(
            "selected animation has no finite keyframe times".to_owned(),
        )
    })?;
    let duration = (clip_end - clip_start).max(0.0);
    let estimated_frames =
        (duration as f64 * options.sample_rate as f64).ceil();
    if !estimated_frames.is_finite() || estimated_frames > 1_000_000.0 {
        return Err(RetargetError::Unsupported(
            "selected animation would require more than 1,000,000 baked frames"
                .to_owned(),
        ));
    }
    let frame_count =
        ((duration * options.sample_rate).ceil() as usize + 1).max(2);
    let mut times = (0..frame_count)
        .map(|index| (index as f32 / options.sample_rate).min(duration))
        .collect::<Vec<_>>();
    if let Some(last) = times.last_mut() {
        *last = duration;
    }
    let mut frames = Vec::with_capacity(times.len());
    for time in &times {
        let pose = source_runtime
            .sample_nodes(source_clip_index, clip_start + *time)
            .map_err(|error| RetargetError::Source(error.to_string()))?;
        frames.push(
            pose.into_iter()
                .map(|node| Transform {
                    position: node.world_translation,
                    rotation: node.world_rotation,
                    scale: node.world_scale,
                })
                .collect::<Vec<_>>(),
        );
    }
    let rest = source_runtime
        .nodes
        .iter()
        .map(|node| Transform {
            position: node.translation,
            rotation: node.rotation,
            scale: node.scale,
        })
        .collect::<Vec<_>>();
    let rest = world_from_locals(&rest, &source_descriptor.nodes)?;
    retarget_frames(
        &source_descriptor,
        &target_descriptor,
        &resolved,
        &rest,
        &frames,
        &times,
        options,
        name,
    )
}

impl SkeletonDescriptor {
    pub fn from_runtime(
        runtime: &AnimationRuntime,
        document: &GlbDocument,
        skin_index: usize,
        animated_nodes: &HashSet<usize>,
        file_sha256: String,
        up_axis: String,
        forward_axis: String,
        unit: String,
    ) -> Result<Self, RetargetError> {
        let skin = document
            .skin_data_at(skin_index)
            .map_err(|error| RetargetError::Source(error.to_string()))?;
        if runtime.nodes.len() != skin.nodes.len() {
            return Err(RetargetError::Source(
                "runtime and document node counts differ".to_owned(),
            ));
        }
        Self::from_skin(
            &skin,
            SourceKind::Glb,
            file_sha256,
            String::new(),
            up_axis,
            forward_axis,
            unit,
            animated_nodes,
        )
    }
}

fn retarget_frames(
    source: &SkeletonDescriptor,
    target: &SkeletonDescriptor,
    mapping: &ResolvedMapping,
    source_rest: &[Transform],
    source_frames: &[Vec<Transform>],
    times: &[f32],
    options: RetargetOptions,
    name: impl Into<String>,
) -> Result<crate::modules::bvh::RetargetClip, RetargetError> {
    validate_source_root_transform(options)?;
    if source_frames.len() < 2 || times.len() != source_frames.len() {
        return Err(RetargetError::Source(
            "retargeting requires at least two source frames".to_owned(),
        ));
    }
    if source_rest.len() != source.nodes.len()
        || source_frames
            .iter()
            .any(|frame| frame.len() != source.nodes.len())
    {
        return Err(RetargetError::Source(
            "source pose node count does not match the skeleton".to_owned(),
        ));
    }
    if source_rest
        .iter()
        .any(|transform| !transform_is_finite(*transform))
        || source_frames
            .iter()
            .flatten()
            .any(|transform| !transform_is_finite(*transform))
        || times.iter().any(|time| !time.is_finite())
        || times.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(RetargetError::Source(
            "source pose contains non-finite or unordered values".to_owned(),
        ));
    }
    let source_rest = source_rest
        .iter()
        .copied()
        .map(|transform| apply_source_root_transform(transform, options))
        .collect::<Vec<_>>();
    let source_frames = source_frames
        .iter()
        .map(|frame| {
            frame
                .iter()
                .copied()
                .map(|transform| {
                    apply_source_root_transform(transform, options)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let target_rest = target_rest_world(target)?;
    let source_basis =
        CoordinateBasis::from_axes(&source.up_axis, &source.forward_axis)?;
    let target_basis =
        CoordinateBasis::from_axes(&target.up_axis, &target.forward_axis)?;
    let unit_conversion =
        unit_to_meters(&source.unit)? / unit_to_meters(&target.unit)?;
    let heading = if options.normalize_initial_heading {
        initial_heading(
            source_frames[0][mapping.source_root].rotation,
            &source_basis,
            &target_basis,
        )
    } else {
        identity_quaternion()
    };
    let target_order = hierarchy_order(&target.nodes)?;
    let root_motion = mapping
        .root_motion
        .or_else(|| Some((mapping.source_root, mapping.target_root, 1.0)));
    let root_start = source_frames[0]
        .get(
            root_motion
                .map(|value| value.0)
                .unwrap_or(mapping.source_root),
        )
        .map(|transform| transform.position)
        .ok_or_else(|| {
            RetargetError::Source("source root is invalid".to_owned())
        })?;
    let mut output_targets = mapping
        .bones
        .iter()
        .map(|bone| bone.target)
        .collect::<Vec<_>>();
    if options.root_motion {
        if let Some((_, root_target, _)) = root_motion {
            if !output_targets.contains(&root_target) {
                output_targets.push(root_target);
            }
        }
    }
    let mut channels = output_targets
        .iter()
        .map(|target_index| AnimationChannelData {
            node: *target_index,
            rotations: Vec::with_capacity(times.len()),
            translations: None,
        })
        .collect::<Vec<_>>();
    let channel_by_target = output_targets
        .iter()
        .copied()
        .enumerate()
        .map(|(index, target)| (target, index))
        .collect::<HashMap<_, _>>();
    let mut target_world = target_rest.clone();
    let root_translation_channel =
        root_motion.filter(|_| options.root_motion).and_then(
            |(_, target_root, _)| channel_by_target.get(&target_root).copied(),
        );
    if let Some(channel_index) = root_translation_channel {
        channels[channel_index].translations =
            Some(Vec::with_capacity(times.len()));
    }
    for source_frame in &source_frames {
        target_world.clone_from(&target_rest);
        let mut desired_world_rotations = HashMap::new();
        for bone in &mapping.bones {
            let source_delta = quat_mul(
                source_frame[bone.source].rotation,
                quat_inverse(source_rest[bone.source].rotation),
            );
            let canonical_delta = source_basis.convert_rotation(source_delta);
            let mut target_delta =
                target_basis.inverse_convert_rotation(canonical_delta);
            target_delta = if options.normalize_initial_heading {
                if bone.source == mapping.source_root {
                    quat_mul(heading, target_delta)
                } else {
                    quat_mul(
                        quat_mul(heading, target_delta),
                        quat_inverse(heading),
                    )
                }
            } else {
                target_delta
            };
            let calibrated_rest = quat_mul(
                target_rest[bone.target].rotation,
                bone.rotation_offset,
            );
            desired_world_rotations.insert(
                bone.target,
                quat_normalize(quat_mul(target_delta, calibrated_rest)),
            );
        }
        for index in &target_order {
            if let Some(rotation) = desired_world_rotations.get(index) {
                target_world[*index].rotation = *rotation;
            }
            if let Some(parent) = target.nodes[*index].parent {
                target_world[*index].position = add(
                    target_world[parent].position,
                    quat_rotate(
                        target_world[parent].rotation,
                        multiply_vec3(
                            target_world[parent].scale,
                            target.nodes[*index].translation,
                        ),
                    ),
                );
                target_world[*index].scale = multiply_vec3(
                    target_world[parent].scale,
                    target.nodes[*index].scale,
                );
            } else {
                target_world[*index].position =
                    target.nodes[*index].translation;
                target_world[*index].scale = target.nodes[*index].scale;
            }
            if let Some(parent) = target.nodes[*index].parent {
                let parent_world = target_world[parent];
                target_world[*index].rotation = desired_world_rotations
                    .get(index)
                    .copied()
                    .unwrap_or(target_rest[*index].rotation);
                let local_rotation = quat_mul(
                    quat_inverse(parent_world.rotation),
                    target_world[*index].rotation,
                );
                if let Some(channel_index) = channel_by_target.get(index) {
                    channels[*channel_index]
                        .rotations
                        .push(quat_normalize(local_rotation));
                }
            } else if let Some(channel_index) = channel_by_target.get(index) {
                let local_rotation = desired_world_rotations
                    .get(index)
                    .copied()
                    .unwrap_or(target.nodes[*index].rotation);
                channels[*channel_index]
                    .rotations
                    .push(quat_normalize(local_rotation));
            }
        }
        if let Some((source_root, target_root, configured_scale)) = root_motion
        {
            if options.root_motion {
                let translation_scale = configured_scale;
                let source_delta =
                    sub(source_frame[source_root].position, root_start);
                let canonical_delta = source_basis.convert_vector(source_delta);
                let mut target_delta =
                    target_basis.inverse_convert_vector(canonical_delta);
                target_delta = quat_rotate(heading, target_delta);
                target_delta = multiply_scalar(
                    target_delta,
                    translation_scale * unit_conversion,
                );
                let parent_world = target.nodes[target_root]
                    .parent
                    .map(|parent| target_world[parent])
                    .unwrap_or(Transform {
                        position: [0.0; 3],
                        rotation: identity_quaternion(),
                        scale: [1.0; 3],
                    });
                let local_delta = quat_rotate(
                    quat_inverse(parent_world.rotation),
                    target_delta,
                );
                let local_delta = [
                    if parent_world.scale[0].abs() > f32::EPSILON {
                        local_delta[0] / parent_world.scale[0]
                    } else {
                        local_delta[0]
                    },
                    if parent_world.scale[1].abs() > f32::EPSILON {
                        local_delta[1] / parent_world.scale[1]
                    } else {
                        local_delta[1]
                    },
                    if parent_world.scale[2].abs() > f32::EPSILON {
                        local_delta[2] / parent_world.scale[2]
                    } else {
                        local_delta[2]
                    },
                ];
                let local_translation =
                    add(target.nodes[target_root].translation, local_delta);
                if let Some(channel_index) = root_translation_channel {
                    if let Some(translations) =
                        channels[channel_index].translations.as_mut()
                    {
                        translations.push(local_translation);
                    }
                }
            }
        }
    }
    if let Some(channel_index) = root_translation_channel {
        if channels[channel_index]
            .translations
            .as_ref()
            .is_none_or(|values| values.len() != times.len())
        {
            return Err(RetargetError::Target(
                "root motion track did not produce one value per frame"
                    .to_owned(),
            ));
        }
    }
    if channels.iter().any(|channel| {
        channel
            .rotations
            .iter()
            .any(|rotation| !rotation.iter().all(|value| value.is_finite()))
            || channel.translations.as_ref().is_some_and(|translations| {
                translations.iter().any(|translation| {
                    !translation.iter().all(|value| value.is_finite())
                })
            })
    }) {
        return Err(RetargetError::Target(
            "retargeted pose contains non-finite values".to_owned(),
        ));
    }
    Ok(crate::modules::bvh::RetargetClip {
        name: name.into(),
        times: times.to_vec(),
        channels,
    })
}

fn validate_endpoint(
    endpoint: &MappingEndpoint,
    label: &str,
) -> Result<(), RetargetError> {
    if endpoint.root.node.trim().is_empty()
        || endpoint.up_axis.trim().is_empty()
        || endpoint.forward_axis.trim().is_empty()
        || endpoint.unit.trim().is_empty()
    {
        return Err(RetargetError::Mapping(format!(
            "{label} endpoint root, axes, and unit are required"
        )));
    }
    CoordinateBasis::from_axes(&endpoint.up_axis, &endpoint.forward_axis)?;
    validate_sha256(&endpoint.skeleton_sha256, &format!("{label} skeleton"))?;
    if !endpoint.file_sha256.is_empty() {
        validate_sha256(&endpoint.file_sha256, &format!("{label} file"))?;
    }
    let unit = endpoint.unit.to_ascii_lowercase();
    if !matches!(
        unit.as_str(),
        "m" | "meter"
            | "meters"
            | "cm"
            | "centimeter"
            | "centimeters"
            | "mm"
            | "millimeter"
            | "millimeters"
    ) {
        return Err(RetargetError::Mapping(format!(
            "unsupported {label} unit '{}'",
            endpoint.unit
        )));
    }
    Ok(())
}

fn validate_node_reference(
    reference: &NodeRef,
    label: &str,
) -> Result<(), RetargetError> {
    if reference.node.trim().is_empty() || reference.path.is_empty() {
        return Err(RetargetError::Mapping(format!(
            "{label} node name and complete path are required"
        )));
    }
    if reference.path.last().map(String::as_str)
        != Some(reference.node.as_str())
    {
        return Err(RetargetError::Mapping(format!(
            "{label} node '{}' does not match the final path component",
            reference.node
        )));
    }
    if reference
        .path
        .iter()
        .any(|component| component.trim().is_empty())
    {
        return Err(RetargetError::Mapping(format!(
            "{label} node '{}' has an empty path component",
            reference.node
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), RetargetError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(RetargetError::Mapping(format!(
            "{label} fingerprint must be a 64-character hexadecimal SHA-256"
        )));
    }
    Ok(())
}

fn validate_identity(
    endpoint: &MappingEndpoint,
    descriptor: &SkeletonDescriptor,
    label: &str,
    report: &mut MappingValidationReport,
) {
    if !endpoint.file_sha256.is_empty() {
        if descriptor.file_sha256.is_empty() {
            report.warnings.push(format!(
                "{label} file fingerprint could not be verified; validating skeleton fingerprint"
            ));
        } else if !endpoint
            .file_sha256
            .eq_ignore_ascii_case(&descriptor.file_sha256)
        {
            report.warnings.push(format!(
                "{label} file fingerprint differs; validating skeleton fingerprint"
            ));
        }
    }
    if !endpoint
        .skeleton_sha256
        .eq_ignore_ascii_case(&descriptor.skeleton_sha256)
    {
        report
            .errors
            .push(format!("{label} skeleton fingerprint does not match"));
    }
    if !axes_equal(&endpoint.up_axis, &descriptor.up_axis) {
        report.errors.push(format!(
            "{label} up axis '{}' does not match the loaded skeleton's '{}'",
            endpoint.up_axis, descriptor.up_axis
        ));
    }
    if !axes_equal(&endpoint.forward_axis, &descriptor.forward_axis) {
        report.errors.push(format!(
            "{label} forward axis '{}' does not match the loaded skeleton's '{}'",
            endpoint.forward_axis, descriptor.forward_axis
        ));
    }
    if !units_equal(&endpoint.unit, &descriptor.unit) {
        report.errors.push(format!(
            "{label} unit '{}' does not match the loaded skeleton's '{}'",
            endpoint.unit, descriptor.unit
        ));
    }
    if endpoint.skin != descriptor.skin {
        report.errors.push(format!(
            "{label} selected Skin does not match the loaded skeleton"
        ));
    }
}

fn resolve_node(
    reference: &NodeRef,
    descriptor: &SkeletonDescriptor,
    label: &str,
    report: &mut MappingValidationReport,
) -> Option<usize> {
    match resolve_node_no_report(reference, descriptor) {
        Some(index) => Some(index),
        None => {
            report.errors.push(format!(
                "{label} node '{}' with path {:?} was not found or is ambiguous",
                reference.node, reference.path
            ));
            None
        }
    }
}

fn resolve_node_required(
    reference: &NodeRef,
    descriptor: &SkeletonDescriptor,
    label: &str,
) -> Result<usize, RetargetError> {
    resolve_node_no_report(reference, descriptor).ok_or_else(|| {
        RetargetError::Mapping(format!(
            "{label} node '{}' with path {:?} was not found or is ambiguous",
            reference.node, reference.path
        ))
    })
}

fn resolve_node_no_report(
    reference: &NodeRef,
    descriptor: &SkeletonDescriptor,
) -> Option<usize> {
    let mut candidates = descriptor
        .nodes
        .iter()
        .filter(|node| node.index < descriptor.nodes.len())
        .filter(|node| node.name == reference.node)
        .filter(|node| reference.path.is_empty() || node.path == reference.path)
        .map(|node| node.index)
        .collect::<Vec<_>>();
    candidates.retain(|candidate| *candidate == reference.index);
    if candidates.len() == 1 {
        candidates.into_iter().next()
    } else {
        None
    }
}

fn unique_name_lookup(
    descriptor: &SkeletonDescriptor,
) -> HashMap<String, usize> {
    let mut lookup = HashMap::new();
    let mut duplicates = HashSet::new();
    for node in &descriptor.nodes {
        if duplicates.contains(&node.name) {
            continue;
        }
        if lookup.contains_key(&node.name) {
            lookup.remove(&node.name);
            duplicates.insert(node.name.clone());
        } else {
            lookup.insert(node.name.clone(), node.index);
        }
    }
    lookup
}

fn is_ancestor(
    descriptor: &SkeletonDescriptor,
    ancestor: usize,
    node: usize,
) -> bool {
    if ancestor >= descriptor.nodes.len() || node >= descriptor.nodes.len() {
        return false;
    }
    let mut current = descriptor.nodes[node].parent;
    while let Some(index) = current {
        if index >= descriptor.nodes.len() {
            return false;
        }
        if index == ancestor {
            return true;
        }
        current = descriptor.nodes[index].parent;
    }
    false
}

fn is_skeleton_root(descriptor: &SkeletonDescriptor, index: usize) -> bool {
    if index >= descriptor.nodes.len() {
        return false;
    }
    let mut current = descriptor.nodes[index].parent;
    while let Some(parent) = current {
        if parent >= descriptor.nodes.len() {
            return false;
        }
        if descriptor.nodes[parent].is_skin_joint {
            return false;
        }
        current = descriptor.nodes[parent].parent;
    }
    true
}

fn hierarchy_order(
    nodes: &[SkeletonNode],
) -> Result<Vec<usize>, RetargetError> {
    let mut order = Vec::with_capacity(nodes.len());
    let mut visiting = vec![false; nodes.len()];
    for index in 0..nodes.len() {
        visit_node(index, nodes, &mut visiting, &mut order)?;
    }
    Ok(order)
}

fn visit_node(
    index: usize,
    nodes: &[SkeletonNode],
    visiting: &mut [bool],
    order: &mut Vec<usize>,
) -> Result<(), RetargetError> {
    if visiting[index] {
        return Err(RetargetError::Mapping(
            "skeleton hierarchy contains a cycle".to_owned(),
        ));
    }
    if order.contains(&index) {
        return Ok(());
    }
    visiting[index] = true;
    if let Some(parent) = nodes[index].parent {
        if parent >= nodes.len() {
            return Err(RetargetError::Mapping(format!(
                "node {} has invalid parent {}",
                index, parent
            )));
        }
        visit_node(parent, nodes, visiting, order)?;
    }
    visiting[index] = false;
    order.push(index);
    Ok(())
}

fn target_rest_world(
    descriptor: &SkeletonDescriptor,
) -> Result<Vec<Transform>, RetargetError> {
    let locals = descriptor
        .nodes
        .iter()
        .map(|node| Transform {
            position: node.translation,
            rotation: node.rotation,
            scale: node.scale,
        })
        .collect::<Vec<_>>();
    world_from_locals(&locals, &descriptor.nodes)
}

fn world_from_locals(
    locals: &[Transform],
    nodes: &[SkeletonNode],
) -> Result<Vec<Transform>, RetargetError> {
    let order = hierarchy_order(nodes)?;
    let mut world = vec![
        Transform {
            position: [0.0; 3],
            rotation: identity_quaternion(),
            scale: [1.0; 3],
        };
        locals.len()
    ];
    for index in order {
        world[index] = if let Some(parent) = nodes[index].parent {
            let parent_world = world[parent];
            Transform {
                position: add(
                    parent_world.position,
                    quat_rotate(
                        parent_world.rotation,
                        multiply_vec3(
                            parent_world.scale,
                            locals[index].position,
                        ),
                    ),
                ),
                rotation: quat_normalize(quat_mul(
                    parent_world.rotation,
                    locals[index].rotation,
                )),
                scale: multiply_vec3(parent_world.scale, locals[index].scale),
            }
        } else {
            locals[index]
        };
        if !transform_is_finite(world[index]) {
            return Err(RetargetError::Target(format!(
                "world transform for node {index} is non-finite"
            )));
        }
    }
    Ok(world)
}

fn bvh_rest_transforms(
    document: &BvhDocument,
) -> Result<Vec<Transform>, RetargetError> {
    let values = document
        .rest_transforms_for_retarget()
        .map_err(|error| RetargetError::Source(error.to_string()))?;
    Ok(values
        .into_iter()
        .map(|(position, rotation, scale)| Transform {
            position,
            rotation,
            scale,
        })
        .collect())
}

fn bvh_frame_transforms(
    document: &BvhDocument,
    frame: &[f32],
) -> Result<Vec<Transform>, RetargetError> {
    let values = document
        .frame_transforms_for_retarget(frame)
        .map_err(|error| RetargetError::Source(error.to_string()))?;
    Ok(values
        .into_iter()
        .map(|(position, rotation, scale)| Transform {
            position,
            rotation,
            scale,
        })
        .collect())
}

#[derive(Debug, Clone, Copy)]
struct CoordinateBasis {
    matrix: [[f32; 3]; 3],
}

impl CoordinateBasis {
    fn from_axes(
        up_axis: &str,
        forward_axis: &str,
    ) -> Result<Self, RetargetError> {
        let up = parse_axis(up_axis).ok_or_else(|| {
            RetargetError::Mapping(format!("unsupported up axis '{up_axis}'"))
        })?;
        let forward = parse_axis(forward_axis).ok_or_else(|| {
            RetargetError::Mapping(format!(
                "unsupported forward axis '{forward_axis}'"
            ))
        })?;
        if dot(up, forward).abs() > 1.0e-5 {
            return Err(RetargetError::Mapping(
                "up and forward axes must be perpendicular".to_owned(),
            ));
        }
        let right = cross(forward, up);
        if length(right) <= f32::EPSILON {
            return Err(RetargetError::Mapping(
                "coordinate basis is degenerate".to_owned(),
            ));
        }
        Ok(Self {
            matrix: [right, up, multiply_scalar(forward, -1.0)],
        })
    }

    fn convert_vector(self, value: [f32; 3]) -> [f32; 3] {
        [
            dot(self.matrix[0], value),
            dot(self.matrix[1], value),
            dot(self.matrix[2], value),
        ]
    }

    fn inverse_convert_vector(self, value: [f32; 3]) -> [f32; 3] {
        [
            self.matrix[0][0] * value[0]
                + self.matrix[1][0] * value[1]
                + self.matrix[2][0] * value[2],
            self.matrix[0][1] * value[0]
                + self.matrix[1][1] * value[1]
                + self.matrix[2][1] * value[2],
            self.matrix[0][2] * value[0]
                + self.matrix[1][2] * value[1]
                + self.matrix[2][2] * value[2],
        ]
    }

    fn convert_rotation(self, rotation: [f32; 4]) -> [f32; 4] {
        let source = quaternion_matrix(rotation);
        let basis = self.matrix;
        let result = multiply_matrix(
            multiply_matrix(basis, source),
            transpose_matrix(basis),
        );
        quaternion_from_matrix(result)
    }

    fn inverse_convert_rotation(self, rotation: [f32; 4]) -> [f32; 4] {
        let basis = transpose_matrix(self.matrix);
        let result = multiply_matrix(
            multiply_matrix(basis, quaternion_matrix(rotation)),
            transpose_matrix(basis),
        );
        quaternion_from_matrix(result)
    }
}

fn initial_heading(
    root_rotation: [f32; 4],
    source_basis: &CoordinateBasis,
    target_basis: &CoordinateBasis,
) -> [f32; 4] {
    let canonical_root = source_basis.convert_rotation(root_rotation);
    let source_forward = quat_rotate(canonical_root, [0.0, 0.0, -1.0]);
    let source_forward = target_basis.inverse_convert_vector(source_forward);
    let target_forward = target_basis.inverse_convert_vector([0.0, 0.0, -1.0]);
    let target_up = target_basis.inverse_convert_vector([0.0, 1.0, 0.0]);
    let target_up =
        multiply_scalar(target_up, 1.0 / length(target_up).max(f32::EPSILON));
    let source_horizontal = sub(
        source_forward,
        multiply_scalar(target_up, dot(source_forward, target_up)),
    );
    let target_horizontal = sub(
        target_forward,
        multiply_scalar(target_up, dot(target_forward, target_up)),
    );
    let source_length = length(source_horizontal);
    let target_length = length(target_horizontal);
    if source_length <= 1.0e-5 || target_length <= 1.0e-5 {
        return identity_quaternion();
    }
    let source = multiply_scalar(source_horizontal, 1.0 / source_length);
    let target = multiply_scalar(target_horizontal, 1.0 / target_length);
    let angle =
        dot(cross(source, target), target_up).atan2(dot(source, target));
    axis_quaternion(target_up, angle)
}

fn node_path(
    nodes: &[crate::modules::glb::SkinNode],
    index: usize,
) -> Result<Vec<String>, RetargetError> {
    let mut path = Vec::new();
    let mut current = Some(index);
    let mut seen = HashSet::new();
    while let Some(current_index) = current {
        if !seen.insert(current_index) {
            return Err(RetargetError::Mapping(
                "node hierarchy contains a cycle".to_owned(),
            ));
        }
        let node = nodes.get(current_index).ok_or_else(|| {
            RetargetError::Mapping(format!(
                "node {current_index} is out of range"
            ))
        })?;
        path.push(node.name.clone());
        current = node.parent;
    }
    path.reverse();
    Ok(path)
}

fn bvh_node_path(
    document: &BvhDocument,
    index: usize,
) -> Result<Vec<String>, RetargetError> {
    let mut path = Vec::new();
    let mut current = Some(index);
    let mut seen = HashSet::new();
    while let Some(current_index) = current {
        if !seen.insert(current_index) {
            return Err(RetargetError::Mapping(
                "BVH hierarchy contains a cycle".to_owned(),
            ));
        }
        let node = document.joints.get(current_index).ok_or_else(|| {
            RetargetError::Mapping(format!(
                "BVH joint {current_index} is out of range"
            ))
        })?;
        path.push(node.name.clone());
        current = node.parent;
    }
    path.reverse();
    Ok(path)
}

fn skeleton_hash(
    kind: SourceKind,
    skin: Option<&SkinRef>,
    nodes: &[SkeletonNode],
) -> String {
    let mut payload = format!("{kind:?}|{skin:?}|");
    for node in nodes {
        payload.push_str(&format!(
            "{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{};",
            node.index,
            node.name,
            node.path,
            node.parent,
            node.translation,
            node.rotation,
            node.scale,
            node.children,
            node.is_skin_joint
        ));
    }
    sha256_hex(payload.as_bytes())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Convert a BVH world-space position into the GLB canonical Y-up, -Z-forward
/// metre space used by the preview.  The same basis and unit rules are used by
/// the retargeting core, so source and target overlays remain comparable.
pub fn convert_bvh_position_to_glb(
    value: [f32; 3],
    up_axis: &str,
    forward_axis: &str,
    unit: &str,
) -> Result<[f32; 3], RetargetError> {
    if value.iter().any(|component| !component.is_finite()) {
        return Err(RetargetError::Source(
            "BVH preview position contains non-finite values".to_owned(),
        ));
    }
    let source_basis = CoordinateBasis::from_axes(up_axis, forward_axis)?;
    let target_basis = CoordinateBasis::from_axes("Y", "-Z")?;
    let canonical = source_basis.convert_vector(value);
    Ok(multiply_scalar(
        target_basis.inverse_convert_vector(canonical),
        unit_to_meters(unit)?,
    ))
}

/// Convert the editor's Z-Y-X Euler root control into an XYZW quaternion.
/// Keeping this conversion in the retarget domain makes sampled GLB source
/// poses match the editor preview's root orientation.
pub fn euler_rotation_quaternion(
    degrees: [f32; 3],
) -> Result<[f32; 4], RetargetError> {
    if degrees.iter().any(|value| !value.is_finite()) {
        return Err(RetargetError::Mapping(
            "Euler rotation angles must be finite".to_owned(),
        ));
    }
    let rotation_x = axis_quaternion([1.0, 0.0, 0.0], degrees[0].to_radians());
    let rotation_y = axis_quaternion([0.0, 1.0, 0.0], degrees[1].to_radians());
    let rotation_z = axis_quaternion([0.0, 0.0, 1.0], degrees[2].to_radians());
    Ok(quat_mul(rotation_z, quat_mul(rotation_y, rotation_x)))
}

fn finite_array<const N: usize>(values: [f32; N]) -> Value {
    Value::Array(values.into_iter().map(|value| json!(value)).collect())
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

fn unit_to_meters(unit: &str) -> Result<f32, RetargetError> {
    match unit.to_ascii_lowercase().as_str() {
        "m" | "meter" | "meters" => Ok(1.0),
        "cm" | "centimeter" | "centimeters" => Ok(0.01),
        "mm" | "millimeter" | "millimeters" => Ok(0.001),
        other => Err(RetargetError::Mapping(format!(
            "unsupported unit '{other}'"
        ))),
    }
}

fn axes_equal(left: &str, right: &str) -> bool {
    parse_axis(left) == parse_axis(right)
}

fn units_equal(left: &str, right: &str) -> bool {
    match (unit_to_meters(left), unit_to_meters(right)) {
        (Ok(left), Ok(right)) => (left - right).abs() <= f32::EPSILON,
        _ => left.eq_ignore_ascii_case(right),
    }
}

fn identity_quaternion() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn one() -> f32 {
    1.0
}

fn normalize_quaternion(value: [f32; 4]) -> [f32; 4] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if length <= f32::EPSILON || !length.is_finite() {
        identity_quaternion()
    } else {
        value.map(|component| component / length)
    }
}

fn quat_normalize(value: [f32; 4]) -> [f32; 4] {
    normalize_quaternion(value)
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    normalize_quaternion([
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ])
}

fn quat_inverse(value: [f32; 4]) -> [f32; 4] {
    let normalized = normalize_quaternion(value);
    [
        -normalized[0],
        -normalized[1],
        -normalized[2],
        normalized[3],
    ]
}

fn quat_rotate(rotation: [f32; 4], vector: [f32; 3]) -> [f32; 3] {
    // Rotate the vector directly.  Multiplying by a pure quaternion and
    // normalizing the intermediate product would incorrectly normalize the
    // vector's length, which breaks offsets and root motion distances.
    let q = normalize_quaternion(rotation);
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

fn axis_quaternion(axis: [f32; 3], radians: f32) -> [f32; 4] {
    let half = radians * 0.5;
    let (sin, cos) = half.sin_cos();
    [axis[0] * sin, axis[1] * sin, axis[2] * sin, cos]
}

fn quaternion_matrix(q: [f32; 4]) -> [[f32; 3]; 3] {
    let q = normalize_quaternion(q);
    let [x, y, z, w] = q;
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

fn quaternion_from_matrix(matrix: [[f32; 3]; 3]) -> [f32; 4] {
    let trace = matrix[0][0] + matrix[1][1] + matrix[2][2];
    if trace > 0.0 {
        let root = (trace + 1.0).sqrt() * 2.0;
        return normalize_quaternion([
            (matrix[2][1] - matrix[1][2]) / root,
            (matrix[0][2] - matrix[2][0]) / root,
            (matrix[1][0] - matrix[0][1]) / root,
            0.25 * root,
        ]);
    }
    if matrix[0][0] > matrix[1][1] && matrix[0][0] > matrix[2][2] {
        let root =
            (1.0 + matrix[0][0] - matrix[1][1] - matrix[2][2]).sqrt() * 2.0;
        return normalize_quaternion([
            0.25 * root,
            (matrix[0][1] + matrix[1][0]) / root,
            (matrix[0][2] + matrix[2][0]) / root,
            (matrix[2][1] - matrix[1][2]) / root,
        ]);
    }
    if matrix[1][1] > matrix[2][2] {
        let root =
            (1.0 + matrix[1][1] - matrix[0][0] - matrix[2][2]).sqrt() * 2.0;
        return normalize_quaternion([
            (matrix[0][1] + matrix[1][0]) / root,
            0.25 * root,
            (matrix[1][2] + matrix[2][1]) / root,
            (matrix[0][2] - matrix[2][0]) / root,
        ]);
    }
    let root = (1.0 + matrix[2][2] - matrix[0][0] - matrix[1][1]).sqrt() * 2.0;
    normalize_quaternion([
        (matrix[0][2] + matrix[2][0]) / root,
        (matrix[1][2] + matrix[2][1]) / root,
        0.25 * root,
        (matrix[1][0] - matrix[0][1]) / root,
    ])
}

fn transpose_matrix(matrix: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    [
        [matrix[0][0], matrix[1][0], matrix[2][0]],
        [matrix[0][1], matrix[1][1], matrix[2][1]],
        [matrix[0][2], matrix[1][2], matrix[2][2]],
    ]
}

fn multiply_matrix(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    [0, 1, 2].map(|row| {
        [0, 1, 2].map(|column| {
            (0..3).map(|index| a[row][index] * b[index][column]).sum()
        })
    })
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn multiply_vec3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn multiply_scalar(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
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

fn transform_is_finite(value: Transform) -> bool {
    value.position.iter().all(|component| component.is_finite())
        && value.rotation.iter().all(|component| component.is_finite())
        && value.scale.iter().all(|component| component.is_finite())
}

fn validate_source_root_transform(
    options: RetargetOptions,
) -> Result<(), RetargetError> {
    if !options.source_root_scale.is_finite()
        || options.source_root_scale <= 0.0
    {
        return Err(RetargetError::Mapping(
            "source root scale must be finite and greater than zero".to_owned(),
        ));
    }
    if options
        .source_root_rotation
        .iter()
        .any(|value| !value.is_finite())
        || options
            .source_root_translation
            .iter()
            .any(|value| !value.is_finite())
    {
        return Err(RetargetError::Mapping(
            "source root transform must contain finite values".to_owned(),
        ));
    }
    let length = options
        .source_root_rotation
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return Err(RetargetError::Mapping(
            "source root rotation must be a non-zero quaternion".to_owned(),
        ));
    }
    Ok(())
}

fn apply_source_root_transform(
    value: Transform,
    options: RetargetOptions,
) -> Transform {
    let rotation = normalize_quaternion(options.source_root_rotation);
    Transform {
        position: add(
            options.source_root_translation,
            quat_rotate(
                rotation,
                multiply_scalar(value.position, options.source_root_scale),
            ),
        ),
        rotation: quat_mul(rotation, value.rotation),
        scale: multiply_scalar(value.scale, options.source_root_scale),
    }
}

#[cfg(test)]
#[path = "retarget_tests.rs"]
mod tests;
