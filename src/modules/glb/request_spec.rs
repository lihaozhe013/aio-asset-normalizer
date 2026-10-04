//! JSON job files and the option vocabulary shared with the command flags.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::modules::glb::{
    AnimationOutputMode, BatchNameSelector, GlbBatchRecipe, GlbExportPreset,
    RootMotionRemovalMode,
};

/// Sentinel that selects the automatic resolver instead of an exact name.
pub const AUTOMATIC_NAME: &str = "auto";

pub fn name_selector(value: &str) -> BatchNameSelector {
    if value.eq_ignore_ascii_case(AUTOMATIC_NAME) {
        BatchNameSelector::AutomaticUnique
    } else {
        BatchNameSelector::Exact(value.to_owned())
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum PresetArg {
    PreserveAll,
    Character,
    Skeleton,
}

impl Default for PresetArg {
    fn default() -> Self {
        Self::PreserveAll
    }
}

impl From<PresetArg> for GlbExportPreset {
    fn from(value: PresetArg) -> Self {
        match value {
            PresetArg::PreserveAll => GlbExportPreset::PreserveAll,
            PresetArg::Character => GlbExportPreset::CharacterPackage,
            PresetArg::Skeleton => GlbExportPreset::SkeletonAnimation,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum AnimationOutputArg {
    Combined,
    Split,
}

impl Default for AnimationOutputArg {
    fn default() -> Self {
        Self::Combined
    }
}

impl From<AnimationOutputArg> for AnimationOutputMode {
    fn from(value: AnimationOutputArg) -> Self {
        match value {
            AnimationOutputArg::Combined => AnimationOutputMode::Combined,
            AnimationOutputArg::Split => AnimationOutputMode::Split,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum RootMotionModeArg {
    HorizontalXz,
    AllTranslation,
}

impl Default for RootMotionModeArg {
    fn default() -> Self {
        Self::HorizontalXz
    }
}

impl From<RootMotionModeArg> for RootMotionRemovalMode {
    fn from(value: RootMotionModeArg) -> Self {
        match value {
            RootMotionModeArg::HorizontalXz => {
                RootMotionRemovalMode::HorizontalXZ
            }
            RootMotionModeArg::AllTranslation => {
                RootMotionRemovalMode::AllTranslation
            }
        }
    }
}

/// Resolved recipe options that both the CLI flags and job files produce.
#[derive(Debug, Clone)]
pub struct RecipeOptions {
    pub preset: GlbExportPreset,
    pub skin: BatchNameSelector,
    pub animation_output: AnimationOutputMode,
    pub remove_root_motion: bool,
    pub root_motion_mode: RootMotionRemovalMode,
    pub root_motion_node: BatchNameSelector,
}

impl RecipeOptions {
    pub fn to_recipe(&self) -> GlbBatchRecipe {
        GlbBatchRecipe {
            preset: self.preset,
            skin: self.skin.clone(),
            root_motion_node: self.root_motion_node.clone(),
            animation_output: self.animation_output,
            remove_root_motion: self.remove_root_motion,
            root_motion_removal_mode: self.root_motion_mode,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeSpec {
    #[serde(default)]
    pub preset: PresetArg,
    #[serde(default = "automatic_name")]
    pub skin: String,
    #[serde(default)]
    pub animation_output: AnimationOutputArg,
    #[serde(default)]
    pub remove_root_motion: bool,
    #[serde(default)]
    pub root_motion_mode: RootMotionModeArg,
    #[serde(default = "automatic_name")]
    pub root_motion_node: String,
}

impl Default for RecipeSpec {
    fn default() -> Self {
        Self {
            preset: PresetArg::default(),
            skin: automatic_name(),
            animation_output: AnimationOutputArg::default(),
            remove_root_motion: false,
            root_motion_mode: RootMotionModeArg::default(),
            root_motion_node: automatic_name(),
        }
    }
}

impl RecipeSpec {
    pub fn to_options(&self) -> RecipeOptions {
        RecipeOptions {
            preset: self.preset.into(),
            skin: name_selector(&self.skin),
            animation_output: self.animation_output.into(),
            remove_root_motion: self.remove_root_motion,
            root_motion_mode: self.root_motion_mode.into(),
            root_motion_node: name_selector(&self.root_motion_node),
        }
    }
}

fn automatic_name() -> String {
    AUTOMATIC_NAME.to_owned()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportJobFile {
    #[serde(default)]
    pub command: Option<String>,
    pub inputs: Vec<PathBuf>,
    #[serde(default)]
    pub input_root: Option<PathBuf>,
    pub output_root: PathBuf,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub recipe: Option<RecipeSpec>,
    #[serde(default)]
    pub selection: Option<SelectionSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditSpec {
    #[serde(default)]
    pub rotate_roots_degrees: [f32; 3],
    #[serde(default = "unit_scale")]
    pub scale_roots: f32,
    #[serde(default)]
    pub translate_roots: [f32; 3],
    #[serde(default)]
    pub trim: Option<TrimSpec>,
    #[serde(default)]
    pub animation_rate: Option<RateSpec>,
    #[serde(default)]
    pub smart_loop: Option<SmartLoopSpec>,
    #[serde(default = "enabled")]
    pub bake_root_transform: bool,
    #[serde(default)]
    pub textures: Vec<TextureReplacement>,
}

fn unit_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrimSpec {
    pub animation: usize,
    pub start: f32,
    pub end: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RateSpec {
    pub animation: usize,
    pub rate: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SmartLoopSpec {
    pub animation: usize,
    pub transition_seconds: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditJobFile {
    #[serde(default)]
    pub command: Option<String>,
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub edits: EditSpec,
    #[serde(default)]
    pub export: Option<RecipeSpec>,
    #[serde(default)]
    pub selection: Option<SelectionSpec>,
}

fn enabled() -> bool {
    true
}
impl Default for EditSpec {
    fn default() -> Self {
        Self {
            rotate_roots_degrees: [0.0; 3],
            scale_roots: 1.0,
            translate_roots: [0.0; 3],
            trim: None,
            animation_rate: None,
            smart_loop: None,
            bake_root_transform: true,
            textures: Vec::new(),
        }
    }
}
impl EditSpec {
    pub fn from_export_edits(
        value: &crate::modules::glb::pipeline::ExportEdits,
    ) -> Self {
        Self {
            rotate_roots_degrees: value.orientation_euler_degrees,
            scale_roots: value.root_scale,
            translate_roots: value.root_translation,
            bake_root_transform: value.bake_root_transform,
            trim: value.trim.map(|(animation, start, end)| TrimSpec {
                animation,
                start,
                end,
            }),
            animation_rate: value
                .animation_rate
                .map(|(animation, rate)| RateSpec { animation, rate }),
            smart_loop: value.smart_loop.map(
                |(animation, transition_seconds)| SmartLoopSpec {
                    animation,
                    transition_seconds,
                },
            ),
            textures: Vec::new(),
        }
    }

    pub fn export_edits(&self) -> crate::modules::glb::pipeline::ExportEdits {
        crate::modules::glb::pipeline::ExportEdits {
            orientation_euler_degrees: self.rotate_roots_degrees,
            root_scale: self.scale_roots,
            root_translation: self.translate_roots,
            bake_root_transform: self.bake_root_transform,
            trim: self.trim.as_ref().map(|v| (v.animation, v.start, v.end)),
            animation_rate: self
                .animation_rate
                .as_ref()
                .map(|v| (v.animation, v.rate)),
            smart_loop: self
                .smart_loop
                .as_ref()
                .map(|v| (v.animation, v.transition_seconds)),
        }
    }
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TextureSlotSpec {
    BaseColor,
    Normal,
    MetallicRoughness,
    Occlusion,
    Emissive,
}
impl From<TextureSlotSpec> for crate::modules::glb::TextureSlot {
    fn from(value: TextureSlotSpec) -> Self {
        match value {
            TextureSlotSpec::BaseColor => Self::BaseColor,
            TextureSlotSpec::Normal => Self::Normal,
            TextureSlotSpec::MetallicRoughness => Self::MetallicRoughness,
            TextureSlotSpec::Occlusion => Self::Occlusion,
            TextureSlotSpec::Emissive => Self::Emissive,
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextureReplacement {
    pub mesh: usize,
    pub primitive: usize,
    pub slot: TextureSlotSpec,
    pub image: PathBuf,
    #[serde(default = "enabled")]
    pub duplicate_shared_material: bool,
}
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct SelectionSpec {
    #[serde(default)]
    pub preset: PresetArg,
    #[serde(default)]
    pub scene: Option<usize>,
    #[serde(default)]
    pub skin: Option<usize>,
    #[serde(default)]
    pub nodes: Option<std::collections::BTreeSet<usize>>,
    #[serde(default)]
    pub primitives: Option<
        std::collections::BTreeMap<usize, std::collections::BTreeSet<usize>>,
    >,
    #[serde(default)]
    pub animations: Option<std::collections::BTreeSet<usize>>,
    #[serde(default)]
    pub animation_output: AnimationOutputArg,
    #[serde(default)]
    pub remove_root_motion: bool,
    #[serde(default)]
    pub root_motion_mode: RootMotionModeArg,
    #[serde(default)]
    pub root_motion_node: Option<usize>,
}
impl SelectionSpec {
    pub fn from_selection(
        value: &crate::modules::glb::GlbExportSelection,
    ) -> Self {
        use crate::modules::glb::{
            AnimationOutputMode, GlbExportPreset, RootMotionRemovalMode,
        };
        Self {
            preset: match value.preset {
                GlbExportPreset::PreserveAll => PresetArg::PreserveAll,
                GlbExportPreset::CharacterPackage => PresetArg::Character,
                GlbExportPreset::SkeletonAnimation => PresetArg::Skeleton,
            },
            scene: Some(value.scene_index),
            skin: value.skin_index,
            nodes: Some(value.selected_nodes.clone()),
            primitives: Some(value.selected_primitives.clone()),
            animations: Some(value.selected_animations.clone()),
            animation_output: match value.animation_output {
                AnimationOutputMode::Combined => AnimationOutputArg::Combined,
                AnimationOutputMode::Split => AnimationOutputArg::Split,
            },
            remove_root_motion: value.remove_root_motion,
            root_motion_mode: match value.root_motion_removal_mode {
                RootMotionRemovalMode::HorizontalXZ => {
                    RootMotionModeArg::HorizontalXz
                }
                RootMotionRemovalMode::AllTranslation => {
                    RootMotionModeArg::AllTranslation
                }
            },
            root_motion_node: value.root_motion_node_override,
        }
    }

    pub fn resolve(
        &self,
        document: &crate::modules::glb::GlbDocument,
    ) -> Result<
        crate::modules::glb::GlbExportSelection,
        crate::modules::glb::GlbError,
    > {
        let catalog = document.export_catalog()?;
        if self.skin.is_some_and(|v| v >= catalog.skins.len())
            || self
                .nodes
                .as_ref()
                .is_some_and(|v| v.iter().any(|i| *i >= catalog.nodes.len()))
            || self.animations.as_ref().is_some_and(|v| {
                v.iter().any(|i| *i >= catalog.animations.len())
            })
            || self
                .root_motion_node
                .is_some_and(|v| v >= catalog.nodes.len())
        {
            return Err(crate::modules::glb::GlbError::Invalid(
                "Explicit selection contains an invalid index".into(),
            ));
        }
        if let Some(primitives) = &self.primitives {
            for (mesh, indices) in primitives {
                let mesh = catalog.meshes.get(*mesh).ok_or_else(|| {
                    crate::modules::glb::GlbError::Invalid(
                        "Explicit selection contains an invalid mesh".into(),
                    )
                })?;
                if indices.iter().any(|i| *i >= mesh.primitives.len()) {
                    return Err(crate::modules::glb::GlbError::Invalid(
                        "Explicit selection contains an invalid primitive"
                            .into(),
                    ));
                }
            }
        }
        let mut result = document.default_export_selection()?;
        result.preset = self.preset.into();
        if let Some(scene) = self.scene {
            result.scene_index = scene;
            result.selected_nodes = document
                .export_catalog()?
                .scenes
                .get(scene)
                .ok_or_else(|| {
                    crate::modules::glb::GlbError::Invalid(format!(
                        "Scene {scene} does not exist"
                    ))
                })?
                .roots
                .iter()
                .copied()
                .collect();
        }
        if let Some(skin) = self.skin {
            result.skin_index = Some(skin);
        }
        if let Some(nodes) = &self.nodes {
            result.selected_nodes = nodes.clone();
        }
        if let Some(primitives) = &self.primitives {
            result.selected_primitives = primitives.clone();
        }
        if let Some(animations) = &self.animations {
            result.selected_animations = animations.clone();
        }
        result.animation_output = self.animation_output.into();
        result.remove_root_motion = self.remove_root_motion;
        result.root_motion_removal_mode = self.root_motion_mode.into();
        result.root_motion_node_override = self.root_motion_node;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_edits_and_empty_edits_have_identity_defaults() {
        let omitted: EditJobFile =
            serde_json::from_str(r#"{"input":"a.glb","output":"b.glb"}"#)
                .unwrap();
        let empty: EditJobFile = serde_json::from_str(
            r#"{"input":"a.glb","output":"b.glb","edits":{}}"#,
        )
        .unwrap();
        assert_eq!(omitted.edits.scale_roots, 1.0);
        assert_eq!(
            serde_json::to_value(omitted.edits).unwrap(),
            serde_json::to_value(empty.edits).unwrap()
        );
    }
    #[test]
    fn jobs_reject_unknown_fields() {
        assert!(serde_json::from_str::<EditJobFile>(
            r#"{"input":"a.glb","output":"b.glb","unknown":true}"#
        )
        .is_err());
        assert!(serde_json::from_str::<EditJobFile>(
            r#"{"input":"a.glb","output":"b.glb","edits":{"scale":2}}"#
        )
        .is_err());
    }
    #[test]
    fn automatic_name_is_case_insensitive_but_exact_names_are_preserved() {
        assert!(matches!(
            name_selector("AUTO"),
            BatchNameSelector::AutomaticUnique
        ));
        assert!(
            matches!(name_selector("Armature"), BatchNameSelector::Exact(name) if name == "Armature")
        );
    }
}
