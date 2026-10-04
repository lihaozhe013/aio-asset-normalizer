//! Shared current-file preparation used by desktop state and JSON jobs.
use super::spec::{EditSpec, RecipeSpec, SelectionSpec};
use super::{check_output, OperationError};
use crate::modules::glb::pipeline::{
    apply_export_edits, build_export_jobs, ExportEdits, ExportJob,
};
use crate::modules::glb::{GlbDocument, GlbExportSelection, PrimitiveTarget};
use std::path::Path;

pub fn edit_snapshot(
    document: &GlbDocument,
    edits: &EditSpec,
) -> Result<GlbDocument, OperationError> {
    let mut snapshot = document.clone();
    for replacement in &edits.textures {
        snapshot.replace_texture(
            PrimitiveTarget {
                mesh: replacement.mesh,
                primitive: replacement.primitive,
            },
            replacement.slot.into(),
            &replacement.image,
            replacement.duplicate_shared_material,
        )?;
    }
    apply_export_edits(&mut snapshot, &edits.export_edits())?;
    Ok(snapshot)
}

pub fn resolve_selection(
    document: &GlbDocument,
    recipe: Option<&RecipeSpec>,
    selection: Option<&SelectionSpec>,
) -> Result<GlbExportSelection, OperationError> {
    match (recipe, selection) {
        (Some(_), Some(_)) => Err(OperationError::validation(
            "Recipe and explicit selection are mutually exclusive",
        )),
        (_, Some(selection)) => Ok(selection.resolve(document)?),
        (recipe, None) => Ok(recipe
            .cloned()
            .unwrap_or_default()
            .to_options()
            .to_recipe()
            .resolve(document)?),
    }
}

/// Validate pending operations against the actual export rather than renderer state.
pub fn validate_edits(
    selection: &GlbExportSelection,
    edits: &ExportEdits,
) -> Result<(), OperationError> {
    use crate::modules::glb::GlbExportPreset;
    if selection.preset == GlbExportPreset::PreserveAll {
        return Ok(());
    }
    if edits.smart_loop.is_some() && selection.remove_root_motion {
        return Err(OperationError::validation(
            "Smart Loop and root-motion removal cannot be combined",
        ));
    }
    for animation in [
        edits.trim.map(|v| v.0),
        edits.animation_rate.map(|v| v.0),
        edits.smart_loop.map(|v| v.0),
    ]
    .into_iter()
    .flatten()
    {
        if !selection.selected_animations.contains(&animation) {
            return Err(OperationError::validation(format!("Edited animation {animation} must be included in the export selection")));
        }
    }
    Ok(())
}

pub fn prepare_export(
    document: &GlbDocument,
    edits: &ExportEdits,
    selection: &GlbExportSelection,
    output: &Path,
    sources: &[&Path],
    overwrite: bool,
) -> Result<Vec<ExportJob>, OperationError> {
    validate_edits(selection, edits)?;
    let validation = document.validate_export_selection(selection);
    if !validation.is_valid() {
        return Err(OperationError::validation(validation.errors.join("; ")));
    }
    let jobs = build_export_jobs(document, selection, output)?;
    for job in &jobs {
        check_output(&job.path, sources, overwrite)?;
        preview_job(job)?;
    }
    Ok(jobs)
}
pub use crate::modules::glb::pipeline::{execute_job, preview_job};
