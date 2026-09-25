//! Shared GLB export pipeline steps.
//!
//! The desktop application and the CLI apply the same pending edits, split
//! animation output, and export selection so both front ends stay identical.

use std::path::PathBuf;

use super::batch_runner::clean_filename_component;
use super::{
    AnimationOutputMode, EditOperation, GlbDocument, GlbError, GlbExportReport,
    GlbExportSelection, RootTransformPreview, SmartLoopOptions,
};

/// Pending current-file edits that are applied to a document clone before an
/// export is written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportEdits {
    pub orientation_euler_degrees: [f32; 3],
    pub root_scale: f32,
    pub root_translation: [f32; 3],
    pub trim: Option<(usize, f32, f32)>,
    pub animation_rate: Option<(usize, f32)>,
    pub smart_loop: Option<(usize, f32)>,
    /// Whether the root orientation, scale, and translation are baked into the
    /// clone. Retarget source snapshots keep the animated TRS channels instead.
    pub bake_root_transform: bool,
}

impl Default for ExportEdits {
    fn default() -> Self {
        Self {
            orientation_euler_degrees: [0.0; 3],
            root_scale: 1.0,
            root_translation: [0.0; 3],
            trim: None,
            animation_rate: None,
            smart_loop: None,
            bake_root_transform: true,
        }
    }
}

/// Apply pending current-file edits to `document` in place.
pub fn apply_export_edits(
    document: &mut GlbDocument,
    edits: &ExportEdits,
) -> Result<(), GlbError> {
    if edits.bake_root_transform {
        RootTransformPreview {
            euler_degrees: edits.orientation_euler_degrees,
            scale: edits.root_scale,
            translation: edits.root_translation,
        }
        .to_matrix()?;
    }

    if let Some((animation, start, end)) = edits.trim {
        document.apply(EditOperation::TrimAnimation {
            animation,
            start,
            end,
        })?;
    }

    if edits.bake_root_transform
        && edits
            .orientation_euler_degrees
            .iter()
            .any(|value| value.abs() > f32::EPSILON)
    {
        document.apply(EditOperation::RotateRoots {
            euler_degrees: edits.orientation_euler_degrees,
        })?;
    }
    if edits.bake_root_transform
        && (edits.root_scale - 1.0).abs() > f32::EPSILON
    {
        document.apply(EditOperation::ScaleRoots {
            factor: edits.root_scale,
        })?;
    }
    if edits.bake_root_transform
        && edits
            .root_translation
            .iter()
            .any(|value| value.abs() > f32::EPSILON)
    {
        document.apply(EditOperation::TranslateRoots {
            offset: edits.root_translation,
        })?;
    }

    if let Some((animation, rate)) = edits.animation_rate {
        if !rate.is_finite() || rate <= 0.0 {
            return Err(GlbError::Invalid(
                "Animation rate must be finite and greater than zero"
                    .to_owned(),
            ));
        }
        if (rate - 1.0).abs() > f32::EPSILON {
            document
                .apply(EditOperation::ScaleAnimationRate { animation, rate })?;
        }
    }

    if let Some((animation, transition_seconds)) = edits.smart_loop {
        document.smart_loop_animation(
            animation,
            SmartLoopOptions { transition_seconds },
        )?;
    }

    Ok(())
}

/// One concrete output document, selection, and destination path.
#[derive(Debug, Clone)]
pub struct ExportJob {
    pub document: GlbDocument,
    pub selection: GlbExportSelection,
    pub path: PathBuf,
}

/// Expand one export selection into concrete jobs. Split animation output
/// produces one job per selected animation with sanitized file names.
pub fn build_export_jobs(
    document: &GlbDocument,
    selection: &GlbExportSelection,
    base_path: &std::path::Path,
) -> Result<Vec<ExportJob>, GlbError> {
    if selection.animation_output == AnimationOutputMode::Combined {
        return Ok(vec![ExportJob {
            document: document.clone(),
            selection: selection.clone(),
            path: base_path.to_path_buf(),
        }]);
    }

    if selection.selected_animations.is_empty() {
        return Err(GlbError::Invalid(
            "Split animation output requires at least one selected animation"
                .to_owned(),
        ));
    }
    let names = document.animation_names();
    let stem = base_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("animation");
    let parent = base_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut used_names = std::collections::BTreeSet::new();
    let mut jobs = Vec::new();
    for animation_index in &selection.selected_animations {
        let animation_name = names
            .get(*animation_index)
            .cloned()
            .unwrap_or_else(|| format!("animation-{animation_index}"));
        let cleaned = clean_filename_component(&animation_name);
        let mut suffix = cleaned.clone();
        let mut count = 1;
        while !used_names.insert(suffix.to_lowercase()) {
            count += 1;
            suffix = format!("{cleaned}-{count}");
        }
        let path = parent.join(format!("{stem}--{suffix}.glb"));
        let mut split_selection = selection.clone();
        split_selection.selected_animations =
            std::collections::BTreeSet::from([*animation_index]);
        split_selection.animation_output = AnimationOutputMode::Combined;
        jobs.push(ExportJob {
            document: document.clone(),
            selection: split_selection,
            path,
        });
    }
    Ok(jobs)
}

/// Prune a document clone to the selection and write it atomically.
pub fn export_selection_atomic(
    document: &GlbDocument,
    selection: &GlbExportSelection,
    path: &std::path::Path,
) -> Result<GlbExportReport, GlbError> {
    let mut output = document.clone();
    let report = output.prune_for_export(selection)?;
    output.export_atomic(path)?;
    Ok(report)
}

pub fn format_export_report(report: &GlbExportReport) -> String {
    format!(
        "scenes {} -> {}, nodes {} -> {}, meshes {} -> {}, skins {} -> {}, animations {} -> {}, removed channels {}, root motion channels {}, BIN {} -> {} bytes, GLB {} -> {} bytes",
        report.source.scenes,
        report.output.scenes,
        report.source.nodes,
        report.output.nodes,
        report.source.meshes,
        report.output.meshes,
        report.source.skins,
        report.output.skins,
        report.source.animations,
        report.output.animations,
        report.removed_animation_channels,
        report.root_motion_channels_modified,
        report.source_bin_bytes,
        report.output_bin_bytes,
        report.source_glb_bytes,
        report.output_glb_bytes,
    )
}
