//! Shared retarget export pipeline.
//!
//! Both the desktop application and the CLI generate a target animation clip
//! from a BVH or GLB source and write a retargeted GLB through these helpers so
//! validation, key reduction, animation replacement, and atomic export stay in
//! one place.

use std::collections::HashSet;
use std::path::Path;

use crate::modules::bvh::{BvhDocument, BvhError, RetargetClip};
use crate::modules::glb::{
    AnimationClipData, AnimationRuntime, GlbDocument, GlbError,
    GlbExportReport, GlbExportSelection, SkinData,
};
use crate::modules::retarget::{
    self, RetargetError, RetargetOptions, SkeletonDescriptor, SkeletonMapping,
};

#[derive(Debug)]
pub enum RetargetExportError {
    Retarget(RetargetError),
    Glb(GlbError),
    Bvh(BvhError),
}

impl std::fmt::Display for RetargetExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retarget(error) => error.fmt(formatter),
            Self::Glb(error) => error.fmt(formatter),
            Self::Bvh(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetargetExportError {}

impl From<RetargetError> for RetargetExportError {
    fn from(error: RetargetError) -> Self {
        Self::Retarget(error)
    }
}

impl From<GlbError> for RetargetExportError {
    fn from(error: GlbError) -> Self {
        Self::Glb(error)
    }
}

impl From<BvhError> for RetargetExportError {
    fn from(error: BvhError) -> Self {
        Self::Bvh(error)
    }
}

/// Build a retargeted clip from a BVH motion source.
pub fn retarget_clip_from_bvh(
    source: &BvhDocument,
    target_skin: &SkinData,
    mapping: &SkeletonMapping,
    options: RetargetOptions,
    name: impl Into<String>,
    reduce_keys: Option<f32>,
) -> Result<RetargetClip, RetargetExportError> {
    let mut clip =
        retarget::retarget_bvh(source, target_skin, mapping, options, name)?;
    reduce_clip_keys(&mut clip, reduce_keys)?;
    Ok(clip)
}

/// Build a retargeted clip from one animation of an animated GLB snapshot.
///
/// `source_dir` resolves external buffers when the snapshot is not a
/// self-contained GLB; the snapshot bytes themselves are used for the mapping
/// file fingerprint.
pub fn retarget_clip_from_glb(
    source_document: &GlbDocument,
    source_dir: Option<&Path>,
    clip_index: usize,
    target_skin: &SkinData,
    mapping: &SkeletonMapping,
    options: RetargetOptions,
    name: impl Into<String>,
    reduce_keys: Option<f32>,
) -> Result<RetargetClip, RetargetExportError> {
    let source_bytes = source_document.to_bytes()?;
    let runtime =
        AnimationRuntime::from_bytes_skeleton_only(&source_bytes, source_dir)
            .map_err(|error| {
            RetargetExportError::Retarget(RetargetError::Source(
                error.to_string(),
            ))
        })?;
    let effective_mapping = mapping_for_glb_snapshot(
        mapping,
        &runtime,
        source_document,
        clip_index,
        &source_bytes,
    )?;
    let mut clip = retarget::retarget_glb(
        &runtime,
        source_document,
        clip_index,
        target_skin,
        &effective_mapping,
        options,
        name,
    )?;
    reduce_clip_keys(&mut clip, reduce_keys)?;
    Ok(clip)
}

/// Replace every animation in `target` with the retargeted clip.
pub fn apply_retarget_clip(
    target: &mut GlbDocument,
    clip: RetargetClip,
) -> Result<(), GlbError> {
    target.replace_animations(&AnimationClipData {
        name: clip.name,
        times: clip.times,
        channels: clip.channels,
    })
}

/// Apply the clip, prune the target to the export selection, and write it
/// atomically.
pub fn export_retargeted_glb(
    target: &GlbDocument,
    clip: RetargetClip,
    selection: &GlbExportSelection,
    path: &Path,
) -> Result<GlbExportReport, RetargetExportError> {
    let mut output = target.clone();
    apply_retarget_clip(&mut output, clip)?;
    let report = crate::modules::glb::pipeline::export_selection_atomic(
        &output, selection, path,
    )?;
    Ok(report)
}

fn reduce_clip_keys(
    clip: &mut RetargetClip,
    tolerance: Option<f32>,
) -> Result<(), RetargetExportError> {
    if let Some(tolerance) = tolerance {
        clip.reduce_keys(tolerance)?;
    }
    Ok(())
}

/// Rebuild the source fingerprint in `mapping` from the concrete runtime that
/// will be retargeted, so validation compares the same skeleton and file.
fn mapping_for_glb_snapshot(
    mapping: &SkeletonMapping,
    runtime: &AnimationRuntime,
    source_document: &GlbDocument,
    clip_index: usize,
    source_bytes: &[u8],
) -> Result<SkeletonMapping, RetargetExportError> {
    let clip = runtime.clips.get(clip_index).ok_or_else(|| {
        RetargetExportError::Retarget(RetargetError::Source(format!(
            "Animation {clip_index} does not exist"
        )))
    })?;
    let animated_nodes = clip
        .channels
        .iter()
        .map(|channel| channel.node)
        .collect::<HashSet<_>>();
    let descriptor = SkeletonDescriptor::from_runtime(
        runtime,
        source_document,
        mapping
            .source
            .skin
            .as_ref()
            .map(|skin| skin.index)
            .unwrap_or(0),
        &animated_nodes,
        retarget::sha256_hex(source_bytes),
        mapping.source.up_axis.clone(),
        mapping.source.forward_axis.clone(),
        mapping.source.unit.clone(),
    )?;
    let mut effective = mapping.clone();
    effective.source.file_sha256 = descriptor.file_sha256;
    effective.source.skeleton_sha256 = descriptor.skeleton_sha256;
    Ok(effective)
}
