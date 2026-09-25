//! JSON job files and the option vocabulary shared with the command flags.

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Deserialize;

use aio_asset_normalizer::modules::glb::{
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
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
            RootMotionModeArg::HorizontalXz => RootMotionRemovalMode::HorizontalXZ,
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Deserialize)]
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
    pub recipe: RecipeSpec,
}

#[derive(Debug, Default, Clone, Deserialize)]
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
}

fn unit_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrimSpec {
    pub animation: usize,
    pub start: f32,
    pub end: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateSpec {
    pub animation: usize,
    pub rate: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmartLoopSpec {
    pub animation: usize,
    pub transition_seconds: f32,
}

#[derive(Debug, Deserialize)]
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
    pub export: RecipeSpec,
}

/// Read and parse a JSON job file with a command-specific error label.
pub fn load_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    label: &str,
) -> Result<T, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {label} {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| {
        format!("invalid {label} {}: {error}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_name_maps_to_the_automatic_selector() {
        assert_eq!(name_selector("auto"), BatchNameSelector::AutomaticUnique);
        assert_eq!(name_selector("AUTO"), BatchNameSelector::AutomaticUnique);
        assert_eq!(
            name_selector("Armature"),
            BatchNameSelector::Exact("Armature".to_owned())
        );
    }

    #[test]
    fn export_job_defaults_to_preserve_all_combined() {
        let job: ExportJobFile = serde_json::from_str(
            r#"{"inputs":["a.glb"],"output_root":"out"}"#,
        )
        .unwrap();
        assert_eq!(job.recipe.preset, PresetArg::PreserveAll);
        assert_eq!(job.recipe.skin, "auto");
        assert_eq!(job.recipe.animation_output, AnimationOutputArg::Combined);
        assert!(!job.overwrite);
    }

    #[test]
    fn edit_job_rejects_unknown_fields() {
        let error = serde_json::from_str::<EditJobFile>(
            r#"{"input":"a.glb","output":"b.glb","unknown":1}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown"));
    }
}