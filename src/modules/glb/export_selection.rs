//! GLB export profiles, resource selection, and reference-safe compaction.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::{json, Value};

use super::root_motion::RootMotionPlan;
use super::{GlbDocument, GlbError, GlbSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlbExportPreset {
    PreserveAll,
    CharacterPackage,
    SkeletonAnimation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationOutputMode {
    Combined,
    Split,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbExportSelection {
    pub preset: GlbExportPreset,
    pub scene_index: usize,
    pub skin_index: Option<usize>,
    pub selected_nodes: BTreeSet<usize>,
    pub selected_primitives: BTreeMap<usize, BTreeSet<usize>>,
    pub selected_animations: BTreeSet<usize>,
    pub animation_output: AnimationOutputMode,
    pub remove_root_motion: bool,
    pub root_motion_node_override: Option<usize>,
}

impl Default for GlbExportSelection {
    fn default() -> Self {
        Self {
            preset: GlbExportPreset::PreserveAll,
            scene_index: 0,
            skin_index: None,
            selected_nodes: BTreeSet::new(),
            selected_primitives: BTreeMap::new(),
            selected_animations: BTreeSet::new(),
            animation_output: AnimationOutputMode::Combined,
            remove_root_motion: false,
            root_motion_node_override: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbExportScene {
    pub index: usize,
    pub name: String,
    pub roots: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbExportNode {
    pub index: usize,
    pub name: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbExportPrimitive {
    pub index: usize,
    pub material: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbExportMesh {
    pub index: usize,
    pub name: String,
    pub primitives: Vec<GlbExportPrimitive>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbExportSkin {
    pub index: usize,
    pub name: String,
    pub joint_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbExportAnimation {
    pub index: usize,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbExportCatalog {
    pub scenes: Vec<GlbExportScene>,
    pub nodes: Vec<GlbExportNode>,
    pub meshes: Vec<GlbExportMesh>,
    pub skins: Vec<GlbExportSkin>,
    pub animations: Vec<GlbExportAnimation>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlbExportValidation {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl GlbExportValidation {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlbExportReport {
    pub source: GlbSummary,
    pub output: GlbSummary,
    pub source_bin_bytes: usize,
    pub output_bin_bytes: usize,
    pub source_glb_bytes: usize,
    pub output_glb_bytes: usize,
    pub removed_animation_channels: usize,
    pub root_motion_channels_modified: usize,
}

#[derive(Debug, Clone)]
struct SelectionContext {
    scene_index: usize,
    render_nodes: BTreeSet<usize>,
    required_nodes: BTreeSet<usize>,
    skin_index: Option<usize>,
    mesh_primitives: BTreeMap<usize, BTreeSet<usize>>,
    animation_indices: Vec<usize>,
    root_motion_plan: Option<RootMotionPlan>,
}

#[derive(Debug, Clone, Copy)]
enum ExtensionPolicy {
    Safe,
    Removed,
    Unsupported,
}

impl GlbDocument {
    pub fn binary_size(&self) -> usize {
        self.bin.as_ref().map(Vec::len).unwrap_or_default()
    }

    /// Build the read-only resource catalog used by the export selector.
    pub fn export_catalog(&self) -> Result<GlbExportCatalog, GlbError> {
        let scenes = array_or_empty(&self.json, "scenes")?;
        let nodes = array_or_empty(&self.json, "nodes")?;
        let meshes = array_or_empty(&self.json, "meshes")?;
        let skins = array_or_empty(&self.json, "skins")?;
        let animations = array_or_empty(&self.json, "animations")?;
        let parents = build_parents(nodes)?;

        let scenes = scenes
            .iter()
            .enumerate()
            .map(|(index, scene)| {
                let roots = scene
                    .get("nodes")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .map(|value| {
                                read_index(
                                    value,
                                    &format!("Scene {index} root"),
                                )
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                for root in &roots {
                    if *root >= nodes.len() {
                        return Err(GlbError::Invalid(format!(
                            "Scene {index} references missing node {root}"
                        )));
                    }
                }
                Ok(GlbExportScene {
                    index,
                    name: object_name(scene, "Scene", index)?,
                    roots,
                })
            })
            .collect::<Result<Vec<_>, GlbError>>()?;

        let nodes = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                Ok(GlbExportNode {
                    index,
                    name: object_name(node, "Node", index)?,
                    parent: parents[index],
                    children: node
                        .get("children")
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .map(|value| {
                                    read_index(
                                        value,
                                        &format!("Node {index} child"),
                                    )
                                })
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .transpose()?
                        .unwrap_or_default(),
                    mesh: optional_object_index(node, "mesh")?,
                    skin: optional_object_index(node, "skin")?,
                })
            })
            .collect::<Result<Vec<_>, GlbError>>()?;

        let meshes = meshes
            .iter()
            .enumerate()
            .map(|(index, mesh)| {
                let primitives = mesh
                    .get("primitives")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Mesh {index} has no primitives array"
                        ))
                    })?
                    .iter()
                    .enumerate()
                    .map(|(primitive_index, primitive)| {
                        Ok(GlbExportPrimitive {
                            index: primitive_index,
                            material: optional_object_index(
                                primitive, "material",
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, GlbError>>()?;
                Ok(GlbExportMesh {
                    index,
                    name: object_name(mesh, "Mesh", index)?,
                    primitives,
                })
            })
            .collect::<Result<Vec<_>, GlbError>>()?;

        let skins = skins
            .iter()
            .enumerate()
            .map(|(index, skin)| {
                let joint_count = skin
                    .get("joints")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Skin {index} has no joints array"
                        ))
                    })?
                    .len();
                Ok(GlbExportSkin {
                    index,
                    name: object_name(skin, "Skin", index)?,
                    joint_count,
                })
            })
            .collect::<Result<Vec<_>, GlbError>>()?;

        let animations = animations
            .iter()
            .enumerate()
            .map(|(index, animation)| {
                Ok(GlbExportAnimation {
                    index,
                    name: object_name(animation, "Animation", index)?,
                })
            })
            .collect::<Result<Vec<_>, GlbError>>()?;

        Ok(GlbExportCatalog {
            scenes,
            nodes,
            meshes,
            skins,
            animations,
        })
    }

    /// Create a useful initial selection for a newly loaded document.
    pub fn default_export_selection(
        &self,
    ) -> Result<GlbExportSelection, GlbError> {
        let catalog = self.export_catalog()?;
        let scene_index = self
            .json
            .get("scene")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|index| *index < catalog.scenes.len())
            .unwrap_or(0);
        let selected_nodes = catalog
            .scenes
            .get(scene_index)
            .map(|scene| scene.roots.iter().copied().collect())
            .unwrap_or_default();
        Ok(GlbExportSelection {
            preset: GlbExportPreset::PreserveAll,
            scene_index,
            skin_index: (!catalog.skins.is_empty()).then_some(0),
            selected_nodes,
            selected_primitives: BTreeMap::new(),
            selected_animations: catalog
                .animations
                .iter()
                .map(|animation| animation.index)
                .collect(),
            animation_output: AnimationOutputMode::Combined,
            remove_root_motion: false,
            root_motion_node_override: None,
        })
    }

    /// Validate an export selection without mutating the source document.
    pub fn validate_export_selection(
        &self,
        selection: &GlbExportSelection,
    ) -> GlbExportValidation {
        let mut validation = GlbExportValidation::default();
        if selection.animation_output == AnimationOutputMode::Split
            && selection.preset == GlbExportPreset::PreserveAll
        {
            validation.errors.push(
                "Split animation output requires Character Package or Skeleton Animation"
                    .to_owned(),
            );
        }
        if selection.animation_output == AnimationOutputMode::Split
            && selection.selected_animations.is_empty()
        {
            validation.errors.push(
                "Split animation output requires at least one selected animation"
                    .to_owned(),
            );
        }
        if selection.preset == GlbExportPreset::PreserveAll {
            return validation;
        }
        match self.prepare_selection(selection) {
            Ok(context) => {
                if let Some(plan) = context.root_motion_plan {
                    validation.warnings.extend(plan.warnings);
                }
                validation
            }
            Err(error) => {
                validation.errors.push(error.to_string());
                validation
            }
        }
    }

    /// Compact the document according to a validated export selection.
    pub fn prune_for_export(
        &mut self,
        selection: &GlbExportSelection,
    ) -> Result<GlbExportReport, GlbError> {
        let validation = self.validate_export_selection(selection);
        if !validation.is_valid() {
            return Err(GlbError::Invalid(validation.errors.join("; ")));
        }
        let source = self.summary();
        let source_bin_bytes =
            self.bin.as_ref().map(Vec::len).unwrap_or_default();
        let source_glb_bytes = self.to_bytes()?.len();
        if selection.preset == GlbExportPreset::PreserveAll {
            return Ok(GlbExportReport {
                source: source.clone(),
                output: source,
                source_bin_bytes,
                output_bin_bytes: source_bin_bytes,
                source_glb_bytes,
                output_glb_bytes: source_glb_bytes,
                removed_animation_channels: 0,
                root_motion_channels_modified: 0,
            });
        }

        let context = self.prepare_selection(selection)?;
        let mut working = self.clone();
        let root_motion_channels_modified = context
            .root_motion_plan
            .as_ref()
            .map(|plan| working.apply_root_motion(plan))
            .transpose()?
            .unwrap_or_default();
        let (json, bin, removed_animation_channels) =
            working.build_compacted_document(&context)?;
        let mut candidate = working;
        candidate.json = json;
        candidate.bin = bin;
        candidate.dirty = true;

        let bytes = candidate.to_bytes()?;
        let reparsed = Self::from_bytes(&bytes, None)?;
        super::AnimationRuntime::from_bytes_skeleton_only(&bytes, None)
            .map_err(|error| {
                GlbError::Invalid(format!(
                    "Animation runtime validation failed: {error}"
                ))
            })?;
        let output = reparsed.summary();
        let output_bin_bytes =
            candidate.bin.as_ref().map(Vec::len).unwrap_or_default();
        *self = candidate;
        Ok(GlbExportReport {
            source,
            output,
            source_bin_bytes,
            output_bin_bytes,
            source_glb_bytes,
            output_glb_bytes: bytes.len(),
            removed_animation_channels,
            root_motion_channels_modified,
        })
    }

    pub fn preview_export_selection(
        &self,
        selection: &GlbExportSelection,
    ) -> Result<GlbExportReport, GlbError> {
        let mut preview = self.clone();
        preview.prune_for_export(selection)
    }

    fn prepare_selection(
        &self,
        selection: &GlbExportSelection,
    ) -> Result<SelectionContext, GlbError> {
        let scenes = array_or_empty(&self.json, "scenes")?;
        let nodes = array_or_empty(&self.json, "nodes")?;
        let meshes = array_or_empty(&self.json, "meshes")?;
        let skins = array_or_empty(&self.json, "skins")?;
        let animations = array_or_empty(&self.json, "animations")?;
        let parents = build_parents(nodes)?;
        let scene = scenes.get(selection.scene_index).ok_or_else(|| {
            GlbError::Invalid(format!(
                "Export Scene {} does not exist",
                selection.scene_index
            ))
        })?;
        let scene_nodes = collect_scene_nodes(scene, nodes)?;

        let mut render_nodes = BTreeSet::new();
        let mut required_nodes = BTreeSet::new();
        if selection.preset == GlbExportPreset::CharacterPackage {
            if selection.selected_nodes.is_empty() {
                return Err(GlbError::Invalid(
                    "Character Package requires at least one selected node"
                        .to_owned(),
                ));
            }
            for node in &selection.selected_nodes {
                if !scene_nodes.contains(node) {
                    return Err(GlbError::Invalid(format!(
                        "Selected node {node} is not part of export Scene {}",
                        selection.scene_index
                    )));
                }
                add_node_and_ancestors(*node, &parents, &mut required_nodes)?;
                collect_descendants(*node, nodes, &mut render_nodes)?;
            }
        }

        let skin_index = selection.skin_index;
        if selection.preset == GlbExportPreset::SkeletonAnimation
            && skin_index.is_none()
        {
            return Err(GlbError::Invalid(
                "Skeleton Animation requires a selected Skin".to_owned(),
            ));
        }
        if let Some(skin_index) = skin_index {
            if skin_index >= skins.len() {
                return Err(GlbError::Invalid(format!(
                    "Selected Skin {skin_index} does not exist"
                )));
            }
            self.skin_data_at(skin_index)?;
            let joints = skins[skin_index]
                .get("joints")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Skin {skin_index} has no joints array"
                    ))
                })?;
            if joints.is_empty() {
                return Err(GlbError::Invalid(format!(
                    "Skin {skin_index} has no joints"
                )));
            }
        }

        required_nodes.extend(render_nodes.iter().copied());
        if let Some(skin_index) = skin_index {
            let skin = skins.get(skin_index).ok_or_else(|| {
                GlbError::Invalid(format!("Missing Skin {skin_index}"))
            })?;
            let joints = skin
                .get("joints")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Skin {skin_index} has no joints array"
                    ))
                })?;
            for joint in joints {
                let joint = read_index(joint, "Skin joint")?;
                add_node_and_ancestors(joint, &parents, &mut required_nodes)?;
            }
            if let Some(skeleton) = skin.get("skeleton") {
                let skeleton = read_index(skeleton, "Skin skeleton")?;
                add_node_and_ancestors(
                    skeleton,
                    &parents,
                    &mut required_nodes,
                )?;
            }
        }

        let animation_indices = selection
            .selected_animations
            .iter()
            .copied()
            .collect::<Vec<_>>();
        if selection.preset == GlbExportPreset::SkeletonAnimation
            && animation_indices.is_empty()
        {
            return Err(GlbError::Invalid(
                "Skeleton Animation requires at least one animation".to_owned(),
            ));
        }
        for animation_index in &animation_indices {
            let animation =
                animations.get(*animation_index).ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Selected animation {animation_index} does not exist"
                    ))
                })?;
            let channels = animation
                .get("channels")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Animation {animation_index} has no channels array"
                    ))
                })?;
            if channels.is_empty() {
                return Err(GlbError::Invalid(format!(
                    "Selected animation {animation_index} has no channels"
                )));
            }
            for (channel_index, channel) in channels.iter().enumerate() {
                let target = channel.get("target").ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Animation {animation_index} channel {channel_index} has no target"
                    ))
                })?;
                let target_node = target
                    .get("node")
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {animation_index} channel {channel_index} has no target node"
                        ))
                    })
                    .and_then(|value| {
                        read_index(
                            value,
                            &format!(
                                "Animation {animation_index} channel {channel_index} target node"
                            ),
                        )
                    })?;
                if target_node >= nodes.len() {
                    return Err(GlbError::Invalid(format!(
                        "Animation {animation_index} channel {channel_index} targets missing node {target_node}"
                    )));
                }
                let path = target
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {animation_index} channel {channel_index} has no target path"
                        ))
                    })?;
                if selection.preset == GlbExportPreset::SkeletonAnimation
                    && path == "weights"
                {
                    return Err(GlbError::Unsupported(format!(
                        "Skeleton Animation cannot export Morph Target channel {animation_index}:{channel_index}"
                    )));
                }
                add_node_and_ancestors(
                    target_node,
                    &parents,
                    &mut required_nodes,
                )?;
            }
        }

        let mut referenced_skins = BTreeSet::new();
        for node_index in &render_nodes {
            let node = nodes.get(*node_index).ok_or_else(|| {
                GlbError::Invalid(format!("Missing selected node {node_index}"))
            })?;
            if let Some(skin) = node.get("skin") {
                let skin =
                    read_index(skin, &format!("Node {node_index} skin"))?;
                referenced_skins.insert(skin);
                if Some(skin) != skin_index {
                    return Err(GlbError::Invalid(format!(
                        "Selected node {node_index} references Skin {skin}; choose that Skin or remove the node"
                    )));
                }
            }
        }
        if referenced_skins.len() > 1 {
            return Err(GlbError::Invalid(
                "Character Package cannot combine multiple Skins".to_owned(),
            ));
        }

        let mut mesh_primitives = BTreeMap::new();
        for node_index in &render_nodes {
            let node = nodes.get(*node_index).ok_or_else(|| {
                GlbError::Invalid(format!("Missing selected node {node_index}"))
            })?;
            let Some(mesh_index) = node.get("mesh") else {
                continue;
            };
            let mesh_index =
                read_index(mesh_index, &format!("Node {node_index} mesh"))?;
            let mesh = meshes.get(mesh_index).ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Node {node_index} references missing Mesh {mesh_index}"
                ))
            })?;
            let primitives = mesh
                .get("primitives")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Mesh {mesh_index} has no primitives array"
                ))
            })?;
            let selected = selection
                .selected_primitives
                .get(&mesh_index)
                .cloned()
                .unwrap_or_else(|| (0..primitives.len()).collect());
            if selected.is_empty() {
                return Err(GlbError::Invalid(format!(
                    "Mesh {mesh_index} has no selected Primitives"
                )));
            }
            for primitive in &selected {
                if *primitive >= primitives.len() {
                    return Err(GlbError::Invalid(format!(
                        "Selected Mesh {mesh_index} Primitive {primitive} does not exist"
                    )));
                }
            }
            mesh_primitives.entry(mesh_index).or_insert(selected);
        }

        for (mesh_index, primitive_indices) in &selection.selected_primitives {
            let mesh = meshes.get(*mesh_index).ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Selected Mesh {mesh_index} does not exist"
                ))
            })?;
            let primitives = mesh
                .get("primitives")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Mesh {mesh_index} has no primitives array"
                ))
            })?;
            for primitive in primitive_indices {
                if *primitive >= primitives.len() {
                    return Err(GlbError::Invalid(format!(
                        "Selected Mesh {mesh_index} Primitive {primitive} does not exist"
                    )));
                }
            }
        }

        let root_motion_plan =
            self.plan_root_motion(selection, &animation_indices, skin_index)?;

        Ok(SelectionContext {
            scene_index: selection.scene_index,
            render_nodes,
            required_nodes,
            skin_index,
            mesh_primitives,
            animation_indices,
            root_motion_plan,
        })
    }

    fn build_compacted_document(
        &self,
        context: &SelectionContext,
    ) -> Result<(Value, Option<Vec<u8>>, usize), GlbError> {
        let scenes = array_or_empty(&self.json, "scenes")?;
        let nodes = array_or_empty(&self.json, "nodes")?;
        let meshes = array_or_empty(&self.json, "meshes")?;
        let skins = array_or_empty(&self.json, "skins")?;
        let animations = array_or_empty(&self.json, "animations")?;
        let materials = array_or_empty(&self.json, "materials")?;
        let textures = array_or_empty(&self.json, "textures")?;
        let images = array_or_empty(&self.json, "images")?;
        let samplers = array_or_empty(&self.json, "samplers")?;
        let accessors = array_or_empty(&self.json, "accessors")?;
        let buffer_views = array_or_empty(&self.json, "bufferViews")?;
        let parents = build_parents(nodes)?;

        let node_indices =
            context.required_nodes.iter().copied().collect::<Vec<_>>();
        let node_map = index_map(&node_indices);
        let skin_indices = context.skin_index.into_iter().collect::<Vec<_>>();
        let skin_map = index_map(&skin_indices);
        let mesh_indices =
            context.mesh_primitives.keys().copied().collect::<Vec<_>>();
        let mesh_map = index_map(&mesh_indices);

        let mut material_indices = BTreeSet::new();
        let mut accessor_indices = BTreeSet::new();
        let mut extension_buffer_views = BTreeSet::new();
        for mesh_index in &mesh_indices {
            let mesh = meshes.get(*mesh_index).ok_or_else(|| {
                GlbError::Invalid(format!("Missing Mesh {mesh_index}"))
            })?;
            let primitive_values = mesh
                .get("primitives")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Mesh {mesh_index} has no primitives array"
                    ))
                })?;
            let primitive_indices =
                context.mesh_primitives.get(mesh_index).ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Mesh {mesh_index} selection disappeared"
                    ))
                })?;
            for primitive_index in primitive_indices {
                let primitive = primitive_values
                    .get(*primitive_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                        "Missing Mesh {mesh_index} Primitive {primitive_index}"
                    ))
                    })?;
                collect_primitive_dependencies(
                    primitive,
                    &mut accessor_indices,
                    &mut material_indices,
                    &mut extension_buffer_views,
                )?;
            }
        }

        if let Some(skin_index) = context.skin_index {
            if let Some(skin) = skins.get(skin_index) {
                if let Some(accessor) = skin.get("inverseBindMatrices") {
                    accessor_indices.insert(read_index(
                        accessor,
                        &format!("Skin {skin_index} inverseBindMatrices"),
                    )?);
                }
            }
        }

        let mut animation_sampler_maps = BTreeMap::new();
        let mut removed_animation_channels = 0;
        for animation_index in &context.animation_indices {
            let animation =
                animations.get(*animation_index).ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Missing animation {animation_index}"
                    ))
                })?;
            let channels = animation
                .get("channels")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Animation {animation_index} has no channels array"
                    ))
                })?;
            let samplers_for_animation = animation
                .get("samplers")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Animation {animation_index} has no samplers array"
                    ))
                })?;
            let mut sampler_indices = BTreeSet::new();
            for channel in channels {
                let target_node = channel
                    .get("target")
                    .and_then(|target| target.get("node"))
                    .map(|value| read_index(value, "Animation target node"))
                    .transpose()?
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {animation_index} channel has no target node"
                        ))
                    })?;
                if !context.required_nodes.contains(&target_node) {
                    removed_animation_channels += 1;
                    continue;
                }
                let sampler_index = channel
                    .get("sampler")
                    .map(|value| read_index(value, "Animation sampler"))
                    .transpose()?
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {animation_index} channel has no sampler"
                        ))
                    })?;
                if sampler_index >= samplers_for_animation.len() {
                    return Err(GlbError::Invalid(format!(
                        "Animation {animation_index} references missing sampler {sampler_index}"
                    )));
                }
                sampler_indices.insert(sampler_index);
            }
            if sampler_indices.is_empty() {
                return Err(GlbError::Invalid(format!(
                    "Animation {animation_index} has no channels after selection"
                )));
            }
            let sampler_map =
                index_map(&sampler_indices.iter().copied().collect::<Vec<_>>());
            for sampler_index in &sampler_indices {
                let sampler = samplers_for_animation
                    .get(*sampler_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {animation_index} sampler {sampler_index} is missing"
                        ))
                    })?;
                for key in ["input", "output"] {
                    let accessor = sampler.get(key).ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {animation_index} sampler {sampler_index} has no {key} accessor"
                        ))
                    })?;
                    accessor_indices.insert(read_index(
                        accessor,
                        &format!(
                            "Animation {animation_index} sampler {sampler_index} {key}"
                        ),
                    )?);
                }
            }
            animation_sampler_maps.insert(*animation_index, sampler_map);
        }

        if let Some(materials) = self.json.get("materials") {
            if !materials.is_array() {
                return Err(GlbError::Invalid(
                    "GLB materials field is not an array".to_owned(),
                ));
            }
        }
        let material_indices = material_indices.into_iter().collect::<Vec<_>>();
        for material_index in &material_indices {
            materials.get(*material_index).ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Primitive references missing Material {material_index}"
                ))
            })?;
        }
        let mut texture_indices = BTreeSet::new();
        for material_index in &material_indices {
            let material = materials.get(*material_index).ok_or_else(|| {
                GlbError::Invalid(format!("Missing Material {material_index}"))
            })?;
            collect_material_texture_indices(material, &mut texture_indices)?;
        }
        let texture_indices = texture_indices.into_iter().collect::<Vec<_>>();
        let mut image_indices = BTreeSet::new();
        let mut sampler_indices = BTreeSet::new();
        for texture_index in &texture_indices {
            let texture = textures.get(*texture_index).ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Material references missing Texture {texture_index}"
                ))
            })?;
            if let Some(source) = texture.get("source") {
                image_indices.insert(read_index(
                    source,
                    &format!("Texture {texture_index} source"),
                )?);
            }
            if let Some(source) = texture
                .get("extensions")
                .and_then(Value::as_object)
                .and_then(|extensions| extensions.get("KHR_texture_basisu"))
                .and_then(|extension| extension.get("source"))
            {
                image_indices.insert(read_index(
                    source,
                    &format!(
                        "Texture {texture_index} KHR_texture_basisu source"
                    ),
                )?);
            }
            if let Some(source) = texture
                .get("extensions")
                .and_then(Value::as_object)
                .and_then(|extensions| extensions.get("EXT_texture_webp"))
                .and_then(|extension| extension.get("source"))
            {
                image_indices.insert(read_index(
                    source,
                    &format!("Texture {texture_index} EXT_texture_webp source"),
                )?);
            }
            if let Some(sampler) = texture.get("sampler") {
                sampler_indices.insert(read_index(
                    sampler,
                    &format!("Texture {texture_index} sampler"),
                )?);
            }
        }
        let image_indices = image_indices.into_iter().collect::<Vec<_>>();
        for image_index in &image_indices {
            let image = images.get(*image_index).ok_or_else(|| {
                GlbError::Invalid(format!("Missing Image {image_index}"))
            })?;
            if let Some(uri) = image.get("uri").and_then(Value::as_str) {
                if !uri.starts_with("data:") {
                    return Err(GlbError::Unsupported(format!(
                        "Compact export requires embedded Image {image_index}; external URI is not supported"
                    )));
                }
            }
        }

        for accessor_index in &accessor_indices {
            let accessor = accessors.get(*accessor_index).ok_or_else(|| {
                GlbError::Invalid(format!("Missing Accessor {accessor_index}"))
            })?;
            if let Some(view) = accessor.get("bufferView") {
                extension_buffer_views.insert(read_index(
                    view,
                    &format!("Accessor {accessor_index} bufferView"),
                )?);
            }
            if let Some(sparse) = accessor.get("sparse") {
                if let Some(view) = sparse
                    .get("indices")
                    .and_then(|indices| indices.get("bufferView"))
                {
                    extension_buffer_views.insert(read_index(
                        view,
                        &format!("Accessor {accessor_index} sparse indices"),
                    )?);
                }
                if let Some(view) = sparse
                    .get("values")
                    .and_then(|values| values.get("bufferView"))
                {
                    extension_buffer_views.insert(read_index(
                        view,
                        &format!("Accessor {accessor_index} sparse values"),
                    )?);
                }
            }
        }
        for image_index in &image_indices {
            if let Some(view) = images
                .get(*image_index)
                .and_then(|image| image.get("bufferView"))
            {
                extension_buffer_views.insert(read_index(
                    view,
                    &format!("Image {image_index} bufferView"),
                )?);
            }
        }

        let accessor_indices = accessor_indices.into_iter().collect::<Vec<_>>();
        let accessor_map = index_map(&accessor_indices);
        let view_indices =
            extension_buffer_views.into_iter().collect::<Vec<_>>();
        let view_map = index_map(&view_indices);
        let material_map = index_map(&material_indices);
        let texture_map = index_map(&texture_indices);
        let image_map = index_map(&image_indices);
        let sampler_map =
            index_map(&sampler_indices.into_iter().collect::<Vec<_>>());

        let (new_buffer_views, new_bin) =
            self.compact_buffer_views(buffer_views, &view_indices, &view_map)?;
        let has_buffer_views = !new_buffer_views.is_empty();
        let mut output = self.json.clone();

        let output_scenes = build_output_scenes(
            scenes,
            context.scene_index,
            &context.required_nodes,
            &parents,
            &node_map,
        )?;
        set_array(&mut output, "scenes", output_scenes)?;
        output["scene"] = json!(0);

        let output_nodes = node_indices
            .iter()
            .map(|old_index| {
                let source = nodes.get(*old_index).ok_or_else(|| {
                    GlbError::Invalid(format!("Missing Node {old_index}"))
                })?;
                let mut node = source.clone();
                if let Some(children) = source.get("children") {
                    let children = children
                        .as_array()
                        .ok_or_else(|| {
                            GlbError::Invalid(format!(
                                "Node {old_index} children is not an array"
                            ))
                        })?;
                    let mut remapped_children = Vec::new();
                    for child in children {
                        let child = read_index(child, "Node child")?;
                        if context.required_nodes.contains(&child) {
                            let mapped = node_map.get(&child).copied().ok_or_else(
                                || {
                                    GlbError::Invalid(format!(
                                        "Node {old_index} child {child} was not retained"
                                    ))
                                },
                            )?;
                            remapped_children.push(json!(mapped));
                        }
                    }
                    let children = remapped_children;
                    if children.is_empty() {
                        node.as_object_mut()
                            .map(|object| object.remove("children"));
                    } else {
                        node["children"] = Value::Array(children);
                    }
                }
                let keep_mesh = context.render_nodes.contains(old_index);
                if keep_mesh {
                    if let Some(mesh) = source.get("mesh") {
                        let mesh = read_index(mesh, "Node mesh")?;
                        let mapped =
                            mesh_map.get(&mesh).copied().ok_or_else(|| {
                                GlbError::Invalid(format!(
                                    "Node {old_index} mesh was not retained"
                                ))
                            })?;
                        node["mesh"] = json!(mapped);
                    }
                    if let Some(skin) = source.get("skin") {
                        let skin = read_index(skin, "Node skin")?;
                        let mapped =
                            skin_map.get(&skin).copied().ok_or_else(|| {
                                GlbError::Invalid(format!(
                                    "Node {old_index} Skin was not retained"
                                ))
                            })?;
                        node["skin"] = json!(mapped);
                    }
                } else if let Some(object) = node.as_object_mut() {
                    object.remove("mesh");
                    object.remove("skin");
                    object.remove("camera");
                    object.remove("weights");
                }
                if let Some(object) = node.as_object_mut() {
                    object.remove("camera");
                }
                remap_node_extensions(&mut node)?;
                Ok(node)
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        set_array(&mut output, "nodes", output_nodes)?;

        let output_meshes = mesh_indices
            .iter()
            .map(|old_mesh_index| {
                let mut mesh = meshes
                    .get(*old_mesh_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!("Missing Mesh {old_mesh_index}"))
                    })?
                    .clone();
                let primitives = mesh
                    .get("primitives")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Mesh {old_mesh_index} has no primitives array"
                        ))
                    })?;
                let kept = context
                    .mesh_primitives
                    .get(old_mesh_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Mesh {old_mesh_index} selection disappeared"
                        ))
                    })?;
                let mut output_primitives = Vec::new();
                for old_primitive_index in kept {
                    let mut primitive = primitives
                        .get(*old_primitive_index)
                        .ok_or_else(|| {
                            GlbError::Invalid(format!(
                                "Missing Mesh {old_mesh_index} Primitive {old_primitive_index}"
                            ))
                        })?
                        .clone();
                    remap_primitive(
                        &mut primitive,
                        &accessor_map,
                        &material_map,
                        &view_map,
                    )?;
                    output_primitives.push(primitive);
                }
                mesh["primitives"] = Value::Array(output_primitives);
                Ok(mesh)
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        if output_meshes.is_empty() {
            remove_key(&mut output, "meshes");
        } else {
            set_array(&mut output, "meshes", output_meshes)?;
        }

        if let Some(skin_index) = context.skin_index {
            let mut skin = skins
                .get(skin_index)
                .ok_or_else(|| {
                    GlbError::Invalid(format!("Missing Skin {skin_index}"))
                })?
                .clone();
            remap_skin(&mut skin, &node_map, &accessor_map)?;
            set_array(&mut output, "skins", vec![skin])?;
        } else {
            remove_key(&mut output, "skins");
        }

        let output_animations = context
            .animation_indices
            .iter()
            .map(|old_animation_index| {
                let animation = animations.get(*old_animation_index).ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Missing animation {old_animation_index}"
                    ))
                })?;
                let sampler_map = animation_sampler_maps
                    .get(old_animation_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {old_animation_index} sampler map is missing"
                        ))
                    })?;
                let samplers = animation
                    .get("samplers")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {old_animation_index} has no samplers array"
                        ))
                    })?;
                let output_samplers = sampler_map
                    .keys()
                    .map(|old_sampler_index| {
                        let mut sampler = samplers
                            .get(*old_sampler_index)
                            .ok_or_else(|| {
                                GlbError::Invalid(format!(
                                    "Missing animation sampler {old_sampler_index}"
                                ))
                            })?
                            .clone();
                        remap_sampler(&mut sampler, &accessor_map)?;
                        Ok(sampler)
                    })
                    .collect::<Result<Vec<_>, GlbError>>()?;
                let output_channels = animation
                    .get("channels")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Animation {old_animation_index} has no channels array"
                        ))
                    })?
                    .iter()
                    .map(|channel| {
                        let target_node = channel
                            .get("target")
                            .and_then(|target| target.get("node"))
                            .map(|value| read_index(value, "Animation target node"))
                            .transpose()?
                            .ok_or_else(|| {
                                GlbError::Invalid(format!(
                                    "Animation {old_animation_index} channel has no target node"
                                ))
                            })?;
                        let sampler = channel
                            .get("sampler")
                            .map(|value| read_index(value, "Animation sampler"))
                            .transpose()?
                            .ok_or_else(|| {
                                GlbError::Invalid(format!(
                                    "Animation {old_animation_index} channel has no sampler"
                                ))
                            })?;
                        let mut channel = channel.clone();
                        channel["sampler"] = json!(sampler_map
                            .get(&sampler)
                            .copied()
                            .ok_or_else(|| {
                                GlbError::Invalid(format!(
                                    "Animation {old_animation_index} channel references an unused sampler"
                                ))
                            })?);
                        channel["target"]["node"] = json!(node_map
                            .get(&target_node)
                            .copied()
                            .ok_or_else(|| {
                                GlbError::Invalid(format!(
                                    "Animation {old_animation_index} target node was not retained"
                                ))
                            })?);
                        Ok(channel)
                    })
                    .collect::<Result<Vec<_>, GlbError>>()?;
                let mut animation = animation.clone();
                animation["samplers"] = Value::Array(output_samplers);
                animation["channels"] = Value::Array(output_channels);
                Ok(animation)
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        if output_animations.is_empty() {
            remove_key(&mut output, "animations");
        } else {
            set_array(&mut output, "animations", output_animations)?;
        }

        let output_accessors = accessor_indices
            .iter()
            .map(|old_index| {
                let mut accessor = accessors
                    .get(*old_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Missing Accessor {old_index}"
                        ))
                    })?
                    .clone();
                remap_accessor(&mut accessor, &view_map)?;
                Ok(accessor)
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        if output_accessors.is_empty() {
            remove_key(&mut output, "accessors");
        } else {
            set_array(&mut output, "accessors", output_accessors)?;
        }
        if new_buffer_views.is_empty() {
            remove_key(&mut output, "bufferViews");
            remove_key(&mut output, "buffers");
        } else {
            set_array(&mut output, "bufferViews", new_buffer_views)?;
            let buffers = array_or_empty(&self.json, "buffers")?;
            let mut buffer = buffers.first().cloned().ok_or_else(|| {
                GlbError::Invalid(
                    "GLB has bufferViews but no buffer 0 definition".to_owned(),
                )
            })?;
            if let Some(object) = buffer.as_object_mut() {
                object.remove("uri");
                object.insert("byteLength".to_owned(), json!(new_bin.len()));
            }
            set_array(&mut output, "buffers", vec![buffer])?;
        }

        let output_materials = material_indices
            .iter()
            .map(|old_index| {
                let mut material = materials
                    .get(*old_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Missing Material {old_index}"
                        ))
                    })?
                    .clone();
                remap_material(&mut material, &texture_map)?;
                Ok(material)
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        if output_materials.is_empty() {
            remove_key(&mut output, "materials");
        } else {
            set_array(&mut output, "materials", output_materials)?;
        }

        let output_textures = texture_indices
            .iter()
            .map(|old_index| {
                let mut texture = textures
                    .get(*old_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Missing Texture {old_index}"
                        ))
                    })?
                    .clone();
                remap_texture(&mut texture, &image_map, &sampler_map)?;
                Ok(texture)
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        if output_textures.is_empty() {
            remove_key(&mut output, "textures");
        } else {
            set_array(&mut output, "textures", output_textures)?;
        }

        let output_images = image_indices
            .iter()
            .map(|old_index| {
                let mut image = images
                    .get(*old_index)
                    .ok_or_else(|| {
                        GlbError::Invalid(format!("Missing Image {old_index}"))
                    })?
                    .clone();
                if let Some(view) = image.get("bufferView") {
                    let view = read_index(view, "Image bufferView")?;
                    image["bufferView"] = json!(view_map
                        .get(&view)
                        .copied()
                        .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Image {old_index} bufferView was not retained"
                        ))
                    })?);
                }
                Ok(image)
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        if output_images.is_empty() {
            remove_key(&mut output, "images");
        } else {
            set_array(&mut output, "images", output_images)?;
        }

        let output_samplers = sampler_map
            .keys()
            .map(|old_index| {
                samplers.get(*old_index).cloned().ok_or_else(|| {
                    GlbError::Invalid(format!("Missing sampler {old_index}"))
                })
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        if output_samplers.is_empty() {
            remove_key(&mut output, "samplers");
        } else {
            set_array(&mut output, "samplers", output_samplers)?;
        }

        remove_key(&mut output, "cameras");
        remove_camera_and_light_extensions(&mut output)?;
        validate_compact_extensions(&output)?;
        recompute_extension_lists(&mut output);
        let compact_bin = has_buffer_views.then_some(new_bin);
        Ok((output, compact_bin, removed_animation_channels))
    }

    fn compact_buffer_views(
        &self,
        source_views: &[Value],
        view_indices: &[usize],
        view_map: &BTreeMap<usize, usize>,
    ) -> Result<(Vec<Value>, Vec<u8>), GlbError> {
        if view_indices.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let bin = self.bin.as_deref().ok_or_else(|| {
            GlbError::Invalid(
                "Selected resources require a GLB BIN chunk, but none exists"
                    .to_owned(),
            )
        })?;
        let buffers = array_or_empty(&self.json, "buffers")?;
        let buffer_zero = buffers.first().ok_or_else(|| {
            GlbError::Invalid("GLB has no buffer 0 definition".to_owned())
        })?;
        if buffer_zero.get("uri").is_some() {
            return Err(GlbError::Unsupported(
                "Compact export requires an embedded GLB buffer".to_owned(),
            ));
        }
        let mut output_views = vec![Value::Null; view_indices.len()];
        let mut output_bin = Vec::new();
        for old_index in view_indices {
            let source = source_views.get(*old_index).ok_or_else(|| {
                GlbError::Invalid(format!("Missing bufferView {old_index}"))
            })?;
            let buffer = source
                .get("buffer")
                .map(|value| read_index(value, "bufferView buffer"))
                .transpose()?
                .unwrap_or(0);
            if buffer != 0 {
                return Err(GlbError::Unsupported(format!(
                    "Compact export does not support bufferView {old_index} on buffer {buffer}"
                )));
            }
            let offset = source
                .get("byteOffset")
                .map(|value| read_usize(value, "bufferView byteOffset"))
                .transpose()?
                .unwrap_or(0);
            let length = read_usize(
                source.get("byteLength").ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "bufferView {old_index} has no byteLength"
                    ))
                })?,
                "bufferView byteLength",
            )?;
            let end = offset.checked_add(length).ok_or_else(|| {
                GlbError::Invalid(format!(
                    "bufferView {old_index} range overflows"
                ))
            })?;
            let bytes = bin.get(offset..end).ok_or_else(|| {
                GlbError::Invalid(format!(
                    "bufferView {old_index} exceeds the GLB BIN chunk"
                ))
            })?;
            while !output_bin.len().is_multiple_of(4) {
                output_bin.push(0);
            }
            let new_offset = output_bin.len();
            output_bin.extend_from_slice(bytes);
            let mut view = source.clone();
            view["buffer"] = json!(0);
            view["byteOffset"] = json!(new_offset);
            view["byteLength"] = json!(length);
            if let Some(slot) = view_map.get(old_index) {
                output_views[*slot] = view;
            }
        }
        Ok((output_views, output_bin))
    }
}

fn array_or_empty<'a>(
    json: &'a Value,
    key: &str,
) -> Result<&'a [Value], GlbError> {
    match json.get(key) {
        None => Ok(&[]),
        Some(value) => value.as_array().map(Vec::as_slice).ok_or_else(|| {
            GlbError::Invalid(format!("GLB {key} field is not an array"))
        }),
    }
}

fn set_array(
    json: &mut Value,
    key: &str,
    values: Vec<Value>,
) -> Result<(), GlbError> {
    let object = json.as_object_mut().ok_or_else(|| {
        GlbError::Invalid("GLB JSON root is not an object".to_owned())
    })?;
    object.insert(key.to_owned(), Value::Array(values));
    Ok(())
}

fn remove_key(json: &mut Value, key: &str) {
    if let Some(object) = json.as_object_mut() {
        object.remove(key);
    }
}

fn read_index(value: &Value, context: &str) -> Result<usize, GlbError> {
    let value = value.as_u64().ok_or_else(|| {
        GlbError::Invalid(format!("{context} index is not an unsigned integer"))
    })?;
    usize::try_from(value).map_err(|_| {
        GlbError::Invalid(format!("{context} index is out of range"))
    })
}

fn read_usize(value: &Value, context: &str) -> Result<usize, GlbError> {
    read_index(value, context)
}

fn optional_object_index(
    object: &Value,
    key: &str,
) -> Result<Option<usize>, GlbError> {
    object
        .get(key)
        .map(|value| read_index(value, key))
        .transpose()
}

fn object_name(
    object: &Value,
    fallback: &str,
    index: usize,
) -> Result<String, GlbError> {
    match object.get("name") {
        None => Ok(format!("{fallback} {index}")),
        Some(value) => value.as_str().map(str::to_owned).ok_or_else(|| {
            GlbError::Invalid(format!(
                "{fallback} {index} name is not a string"
            ))
        }),
    }
}

fn build_parents(nodes: &[Value]) -> Result<Vec<Option<usize>>, GlbError> {
    let mut parents = vec![None; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        let Some(children) = node.get("children") else {
            continue;
        };
        let children = children.as_array().ok_or_else(|| {
            GlbError::Invalid(format!("Node {index} children is not an array"))
        })?;
        for child in children {
            let child = read_index(child, &format!("Node {index} child"))?;
            if child >= nodes.len() {
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
    for start in 0..nodes.len() {
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
    Ok(parents)
}

fn collect_scene_nodes(
    scene: &Value,
    nodes: &[Value],
) -> Result<BTreeSet<usize>, GlbError> {
    let roots = match scene.get("nodes") {
        None => &[] as &[Value],
        Some(value) => value.as_array().ok_or_else(|| {
            GlbError::Invalid("Scene nodes is not an array".to_owned())
        })?,
    };
    let mut result = BTreeSet::new();
    for root in roots {
        let root = read_index(root, "Scene root")?;
        collect_descendants(root, nodes, &mut result)?;
    }
    Ok(result)
}

fn collect_descendants(
    root: usize,
    nodes: &[Value],
    output: &mut BTreeSet<usize>,
) -> Result<(), GlbError> {
    if root >= nodes.len() {
        return Err(GlbError::Invalid(format!(
            "Node {root} is outside the node array"
        )));
    }
    if !output.insert(root) {
        return Ok(());
    }
    if let Some(children) = nodes[root].get("children") {
        let children = children.as_array().ok_or_else(|| {
            GlbError::Invalid(format!("Node {root} children is not an array"))
        })?;
        for child in children {
            collect_descendants(
                read_index(child, "Node child")?,
                nodes,
                output,
            )?;
        }
    }
    Ok(())
}

fn add_node_and_ancestors(
    node: usize,
    parents: &[Option<usize>],
    output: &mut BTreeSet<usize>,
) -> Result<(), GlbError> {
    if node >= parents.len() {
        return Err(GlbError::Invalid(format!(
            "Node {node} is outside the node array"
        )));
    }
    let mut current = Some(node);
    let mut seen = HashSet::new();
    while let Some(index) = current {
        if !seen.insert(index) {
            return Err(GlbError::Invalid(format!(
                "Node hierarchy contains a cycle at node {index}"
            )));
        }
        output.insert(index);
        current = parents[index];
    }
    Ok(())
}

fn index_map(indices: &[usize]) -> BTreeMap<usize, usize> {
    indices
        .iter()
        .enumerate()
        .map(|(new_index, old_index)| (*old_index, new_index))
        .collect()
}

fn collect_primitive_dependencies(
    primitive: &Value,
    accessors: &mut BTreeSet<usize>,
    materials: &mut BTreeSet<usize>,
    extension_views: &mut BTreeSet<usize>,
) -> Result<(), GlbError> {
    if let Some(attributes) = primitive.get("attributes") {
        let attributes = attributes.as_object().ok_or_else(|| {
            GlbError::Invalid(
                "Primitive attributes is not an object".to_owned(),
            )
        })?;
        for accessor in attributes.values() {
            accessors.insert(read_index(accessor, "Primitive attribute")?);
        }
    }
    if let Some(indices) = primitive.get("indices") {
        accessors.insert(read_index(indices, "Primitive indices")?);
    }
    if let Some(targets) = primitive.get("targets") {
        let targets = targets.as_array().ok_or_else(|| {
            GlbError::Invalid("Primitive targets is not an array".to_owned())
        })?;
        for target in targets {
            let target = target.as_object().ok_or_else(|| {
                GlbError::Invalid(
                    "Primitive target is not an object".to_owned(),
                )
            })?;
            for accessor in target.values() {
                accessors
                    .insert(read_index(accessor, "Morph target accessor")?);
            }
        }
    }
    if let Some(material) = primitive.get("material") {
        materials.insert(read_index(material, "Primitive material")?);
    }
    if let Some(extension) = primitive
        .get("extensions")
        .and_then(Value::as_object)
        .and_then(|extensions| extensions.get("KHR_draco_mesh_compression"))
        .and_then(|extension| extension.get("bufferView"))
    {
        extension_views.insert(read_index(
            extension,
            "KHR_draco_mesh_compression bufferView",
        )?);
    }
    if let Some(mappings) = primitive
        .get("extensions")
        .and_then(Value::as_object)
        .and_then(|extensions| extensions.get("KHR_materials_variants"))
        .and_then(|extension| extension.get("mappings"))
        .and_then(Value::as_array)
    {
        for mapping in mappings {
            if let Some(material) = mapping.get("material") {
                materials.insert(read_index(
                    material,
                    "KHR_materials_variants material",
                )?);
            }
        }
    }
    Ok(())
}

fn collect_material_texture_indices(
    material: &Value,
    textures: &mut BTreeSet<usize>,
) -> Result<(), GlbError> {
    fn visit(
        value: &Value,
        textures: &mut BTreeSet<usize>,
    ) -> Result<(), GlbError> {
        match value {
            Value::Object(object) => {
                if let Some(index) = object.get("index") {
                    textures.insert(read_index(index, "Material texture")?);
                }
                for (key, child) in object {
                    if key != "extras" {
                        visit(child, textures)?;
                    }
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, textures)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(material, textures)
}

fn remap_primitive(
    primitive: &mut Value,
    accessor_map: &BTreeMap<usize, usize>,
    material_map: &BTreeMap<usize, usize>,
    view_map: &BTreeMap<usize, usize>,
) -> Result<(), GlbError> {
    if let Some(attributes) = primitive
        .get_mut("attributes")
        .and_then(Value::as_object_mut)
    {
        for accessor in attributes.values_mut() {
            remap_value_index(accessor, accessor_map, "Primitive attribute")?;
        }
    }
    if let Some(indices) = primitive.get_mut("indices") {
        remap_value_index(indices, accessor_map, "Primitive indices")?;
    }
    if let Some(targets) =
        primitive.get_mut("targets").and_then(Value::as_array_mut)
    {
        for target in targets {
            if let Some(target) = target.as_object_mut() {
                for accessor in target.values_mut() {
                    remap_value_index(
                        accessor,
                        accessor_map,
                        "Morph target accessor",
                    )?;
                }
            }
        }
    }
    if let Some(material) = primitive.get_mut("material") {
        remap_value_index(material, material_map, "Primitive material")?;
    }
    if let Some(extension) = primitive
        .get_mut("extensions")
        .and_then(Value::as_object_mut)
    {
        if let Some(draco) = extension.get_mut("KHR_draco_mesh_compression") {
            if let Some(view) = draco.get_mut("bufferView") {
                remap_value_index(view, view_map, "KHR_draco bufferView")?;
            }
        }
        if let Some(variants) = extension.get_mut("KHR_materials_variants") {
            if let Some(mappings) =
                variants.get_mut("mappings").and_then(Value::as_array_mut)
            {
                for mapping in mappings {
                    if let Some(material) = mapping.get_mut("material") {
                        remap_value_index(
                            material,
                            material_map,
                            "KHR_materials_variants material",
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn remap_skin(
    skin: &mut Value,
    node_map: &BTreeMap<usize, usize>,
    accessor_map: &BTreeMap<usize, usize>,
) -> Result<(), GlbError> {
    if let Some(joints) = skin.get_mut("joints").and_then(Value::as_array_mut) {
        for joint in joints {
            remap_value_index(joint, node_map, "Skin joint")?;
        }
    }
    if let Some(skeleton) = skin.get_mut("skeleton") {
        remap_value_index(skeleton, node_map, "Skin skeleton")?;
    }
    if let Some(inverse_bind) = skin.get_mut("inverseBindMatrices") {
        remap_value_index(
            inverse_bind,
            accessor_map,
            "Skin inverseBindMatrices",
        )?;
    }
    Ok(())
}

fn remap_sampler(
    sampler: &mut Value,
    accessor_map: &BTreeMap<usize, usize>,
) -> Result<(), GlbError> {
    for key in ["input", "output"] {
        let value = sampler.get_mut(key).ok_or_else(|| {
            GlbError::Invalid(format!(
                "Animation sampler has no {key} accessor"
            ))
        })?;
        remap_value_index(value, accessor_map, "Animation accessor")?;
    }
    Ok(())
}

fn remap_accessor(
    accessor: &mut Value,
    view_map: &BTreeMap<usize, usize>,
) -> Result<(), GlbError> {
    if let Some(view) = accessor.get_mut("bufferView") {
        remap_value_index(view, view_map, "Accessor bufferView")?;
    }
    if let Some(sparse) = accessor.get_mut("sparse") {
        if let Some(view) = sparse
            .get_mut("indices")
            .and_then(|indices| indices.get_mut("bufferView"))
        {
            remap_value_index(view, view_map, "Sparse accessor indices")?;
        }
        if let Some(view) = sparse
            .get_mut("values")
            .and_then(|values| values.get_mut("bufferView"))
        {
            remap_value_index(view, view_map, "Sparse accessor values")?;
        }
    }
    Ok(())
}

fn remap_material(
    material: &mut Value,
    texture_map: &BTreeMap<usize, usize>,
) -> Result<(), GlbError> {
    fn visit(
        value: &mut Value,
        texture_map: &BTreeMap<usize, usize>,
    ) -> Result<(), GlbError> {
        match value {
            Value::Object(object) => {
                if let Some(index) = object.get_mut("index") {
                    remap_value_index(index, texture_map, "Material texture")?;
                }
                let keys = object.keys().cloned().collect::<Vec<_>>();
                for key in keys {
                    if key != "extras" {
                        if let Some(child) = object.get_mut(&key) {
                            visit(child, texture_map)?;
                        }
                    }
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, texture_map)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(material, texture_map)
}

fn remap_texture(
    texture: &mut Value,
    image_map: &BTreeMap<usize, usize>,
    sampler_map: &BTreeMap<usize, usize>,
) -> Result<(), GlbError> {
    if let Some(source) = texture.get_mut("source") {
        remap_value_index(source, image_map, "Texture source")?;
    }
    if let Some(sampler) = texture.get_mut("sampler") {
        remap_value_index(sampler, sampler_map, "Texture sampler")?;
    }
    if let Some(extensions) =
        texture.get_mut("extensions").and_then(Value::as_object_mut)
    {
        for name in ["KHR_texture_basisu", "EXT_texture_webp"] {
            if let Some(extension) = extensions.get_mut(name) {
                if let Some(source) = extension.get_mut("source") {
                    remap_value_index(
                        source,
                        image_map,
                        "Texture extension source",
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn remap_node_extensions(node: &mut Value) -> Result<(), GlbError> {
    if let Some(extensions) =
        node.get_mut("extensions").and_then(Value::as_object_mut)
    {
        extensions.remove("KHR_lights_punctual");
        if extensions.is_empty() {
            node.as_object_mut()
                .map(|object| object.remove("extensions"));
        }
    }
    Ok(())
}

fn remap_value_index(
    value: &mut Value,
    map: &BTreeMap<usize, usize>,
    context: &str,
) -> Result<(), GlbError> {
    let old = read_index(value, context)?;
    let new = map.get(&old).copied().ok_or_else(|| {
        GlbError::Invalid(format!("{context} {old} was not retained"))
    })?;
    *value = json!(new);
    Ok(())
}

fn build_output_scenes(
    scenes: &[Value],
    scene_index: usize,
    required_nodes: &BTreeSet<usize>,
    parents: &[Option<usize>],
    node_map: &BTreeMap<usize, usize>,
) -> Result<Vec<Value>, GlbError> {
    let mut roots = BTreeSet::new();
    for node in required_nodes {
        if parents[*node].is_none_or(|parent| !required_nodes.contains(&parent))
        {
            roots.insert(*node);
        }
    }
    let source = scenes.get(scene_index).ok_or_else(|| {
        GlbError::Invalid(format!("Missing export Scene {scene_index}"))
    })?;
    let mut scene = source.clone();
    let source_roots = match source.get("nodes") {
        None => &[] as &[Value],
        Some(value) => value.as_array().ok_or_else(|| {
            GlbError::Invalid("Scene nodes is not an array".to_owned())
        })?,
    };
    let mut ordered_roots = Vec::new();
    for root in source_roots {
        let root = read_index(root, "Scene root")?;
        if roots.remove(&root) {
            ordered_roots.push(root);
        }
    }
    ordered_roots.extend(roots);
    if ordered_roots.is_empty() {
        return Err(GlbError::Invalid(
            "Export selection does not produce a scene root".to_owned(),
        ));
    }
    scene["nodes"] = Value::Array(
        ordered_roots
            .into_iter()
            .map(|root| {
                node_map
                    .get(&root)
                    .copied()
                    .map(|index| json!(index))
                    .ok_or_else(|| {
                        GlbError::Invalid(format!(
                            "Scene root {root} was not retained"
                        ))
                    })
            })
            .collect::<Result<Vec<_>, GlbError>>()?,
    );
    Ok(vec![scene])
}

fn extension_policy(name: &str) -> ExtensionPolicy {
    if name == "KHR_lights_punctual" {
        ExtensionPolicy::Removed
    } else if matches!(
        name,
        "KHR_texture_basisu"
            | "EXT_texture_webp"
            | "KHR_draco_mesh_compression"
            | "KHR_materials_variants"
            | "KHR_texture_transform"
            | "KHR_mesh_quantization"
            | "KHR_materials_pbrSpecularGlossiness"
            | "KHR_materials_unlit"
            | "KHR_materials_clearcoat"
            | "KHR_materials_transmission"
            | "KHR_materials_sheen"
            | "KHR_materials_ior"
            | "KHR_materials_volume"
            | "KHR_materials_specular"
            | "KHR_materials_iridescence"
            | "KHR_materials_anisotropy"
            | "KHR_materials_emissive_strength"
            | "KHR_materials_dispersion"
    ) {
        ExtensionPolicy::Safe
    } else {
        ExtensionPolicy::Unsupported
    }
}

fn remove_camera_and_light_extensions(
    value: &mut Value,
) -> Result<(), GlbError> {
    if let Value::Object(object) = value {
        if let Some(extensions) =
            object.get_mut("extensions").and_then(Value::as_object_mut)
        {
            extensions.remove("KHR_lights_punctual");
            if extensions.is_empty() {
                object.remove("extensions");
            }
        }
        for (key, child) in object.iter_mut() {
            if key != "extras" {
                remove_camera_and_light_extensions(child)?;
            }
        }
    } else if let Value::Array(values) = value {
        for child in values {
            remove_camera_and_light_extensions(child)?;
        }
    }
    Ok(())
}

fn validate_compact_extensions(value: &Value) -> Result<(), GlbError> {
    fn visit(value: &Value, path: &str) -> Result<(), GlbError> {
        match value {
            Value::Object(object) => {
                if let Some(extensions) = object.get("extensions") {
                    let extensions =
                        extensions.as_object().ok_or_else(|| {
                            GlbError::Invalid(format!(
                                "GLB extensions at {path} are not an object"
                            ))
                        })?;
                    for name in extensions.keys() {
                        match extension_policy(name) {
                            ExtensionPolicy::Safe | ExtensionPolicy::Removed => {}
                            ExtensionPolicy::Unsupported => {
                                return Err(GlbError::Unsupported(format!(
                                    "Compact export cannot safely remap extension {name} at {path}"
                                )))
                            }
                        }
                    }
                }
                for (key, child) in object {
                    match key.as_str() {
                        "extras" => {}
                        "extensions" => {
                            if let Some(extensions) = child.as_object() {
                                for (name, extension) in extensions {
                                    visit(
                                        extension,
                                        &format!("{path}.extensions.{name}"),
                                    )?;
                                }
                            }
                        }
                        _ => visit(child, &format!("{path}.{key}"))?,
                    }
                }
            }
            Value::Array(values) => {
                for (index, child) in values.iter().enumerate() {
                    visit(child, &format!("{path}[{index}]"))?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(value, "root")
}

fn recompute_extension_lists(json: &mut Value) {
    let mut found = BTreeSet::new();
    fn collect(value: &Value, found: &mut BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(extensions) =
                    object.get("extensions").and_then(Value::as_object)
                {
                    found.extend(extensions.keys().cloned());
                    for extension in extensions.values() {
                        collect(extension, found);
                    }
                }
                for (key, child) in object {
                    if key != "extensions"
                        && key != "extras"
                        && key != "extensionsUsed"
                        && key != "extensionsRequired"
                    {
                        collect(child, found);
                    }
                }
            }
            Value::Array(values) => {
                for child in values {
                    collect(child, found);
                }
            }
            _ => {}
        }
    }
    collect(json, &mut found);
    if let Some(object) = json.as_object_mut() {
        if found.is_empty() {
            object.remove("extensionsUsed");
            object.remove("extensionsRequired");
        } else {
            object.insert(
                "extensionsUsed".to_owned(),
                Value::Array(
                    found.iter().cloned().map(Value::String).collect(),
                ),
            );
            if let Some(required) = object
                .get("extensionsRequired")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|name| found.contains(*name))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
            {
                if required.is_empty() {
                    object.remove("extensionsRequired");
                } else {
                    object.insert(
                        "extensionsRequired".to_owned(),
                        Value::Array(
                            required.into_iter().map(Value::String).collect(),
                        ),
                    );
                }
            }
        }
    }
}
