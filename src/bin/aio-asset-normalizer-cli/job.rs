//! JSON job files and the option vocabulary shared with the command flags.

use std::path::Path;

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

pub use aio_asset_normalizer::modules::operations::spec::{
    EditJobFile, ExportJobFile,
};

pub fn load_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    label: &str,
) -> Result<T, crate::output::CliError> {
    aio_asset_normalizer::modules::operations::load_json(path, label)
        .map_err(crate::output::operation_error)
}
