//! Skeleton context and retarget preparation shared by both front ends.
use super::spec::{EditSpec, PresetArg, SelectionSpec};
use super::{check_output, file_sha256, OperationError};
use crate::modules::{
    bvh::BvhDocument,
    glb::{AnimationRuntime, GlbDocument, GlbExportSelection, SkinData},
    retarget::{
        self, MappingValidationReport, RetargetOptions, SkeletonDescriptor,
        SkeletonMapping, SourceKind,
    },
    retarget_export,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetargetJob {
    #[serde(default)]
    pub command: Option<String>,
    pub source: PathBuf,
    pub target: PathBuf,
    #[serde(default)]
    pub source_animation: usize,
    #[serde(default)]
    pub source_skin: usize,
    #[serde(default)]
    pub skin: usize,
    #[serde(default = "up")]
    pub source_up_axis: String,
    #[serde(default = "forward")]
    pub source_forward_axis: String,
    #[serde(default)]
    pub source_unit: Option<String>,
    #[serde(default = "up")]
    pub target_up_axis: String,
    #[serde(default = "forward")]
    pub target_forward_axis: String,
    #[serde(default = "meter")]
    pub target_unit: String,
    #[serde(default)]
    pub source_edits: EditSpec,
    #[serde(default)]
    pub mapping: Option<PathBuf>,
    #[serde(default)]
    pub out: Option<PathBuf>,
    #[serde(default = "clip_name")]
    pub clip_name: String,
    #[serde(default = "sample_rate")]
    pub sample_rate: f32,
    #[serde(default = "enabled")]
    pub root_motion: bool,
    #[serde(default)]
    pub normalize_heading: bool,
    #[serde(default)]
    pub reduce_keys: Option<f32>,
    #[serde(default = "character")]
    pub preset: PresetArg,
    #[serde(default)]
    pub selection: Option<SelectionSpec>,
    #[serde(default)]
    pub overwrite: bool,
}
fn up() -> String {
    "Y".into()
}
fn forward() -> String {
    "-Z".into()
}
fn meter() -> String {
    "m".into()
}
fn clip_name() -> String {
    "Retargeted".into()
}
fn sample_rate() -> f32 {
    RetargetOptions::default().sample_rate
}
fn enabled() -> bool {
    true
}
fn character() -> PresetArg {
    PresetArg::Character
}
impl RetargetJob {
    pub fn for_snapshot_export(
        source: PathBuf,
        target: PathBuf,
        out: PathBuf,
        settings: SnapshotExportSettings,
    ) -> Self {
        let mut job = Self::new(source, target);
        job.out = Some(out);
        job.source_skin = settings.source_skin;
        job.skin = settings.target_skin;
        job.source_up_axis = settings.source_axes[0].clone();
        job.source_forward_axis = settings.source_axes[1].clone();
        job.source_unit = Some(settings.source_axes[2].clone());
        job.source_edits = settings.edits;
        job.clip_name = settings.clip_name;
        job.root_motion = settings.options.root_motion;
        job.normalize_heading = settings.options.normalize_initial_heading;
        job.sample_rate = settings.options.sample_rate;
        job.selection =
            Some(SelectionSpec::from_selection(&settings.selection));
        job.reduce_keys = settings.reduce_keys;
        job.overwrite = settings.overwrite;
        job
    }
    pub fn new(source: PathBuf, target: PathBuf) -> Self {
        Self {
            command: None,
            source,
            target,
            source_animation: 0,
            source_skin: 0,
            skin: 0,
            source_up_axis: up(),
            source_forward_axis: forward(),
            source_unit: None,
            target_up_axis: up(),
            target_forward_axis: forward(),
            target_unit: meter(),
            source_edits: EditSpec::default(),
            mapping: None,
            out: None,
            clip_name: clip_name(),
            sample_rate: sample_rate(),
            root_motion: true,
            normalize_heading: false,
            reduce_keys: None,
            preset: character(),
            selection: None,
            overwrite: false,
        }
    }
    pub fn options(&self) -> Result<RetargetOptions, OperationError> {
        crate::modules::glb::RootTransformPreview {
            euler_degrees: self.source_edits.rotate_roots_degrees,
            scale: self.source_edits.scale_roots,
            translation: self.source_edits.translate_roots,
        }
        .to_matrix()?;
        Ok(RetargetOptions {
            root_motion: self.root_motion,
            normalize_initial_heading: self.normalize_heading,
            sample_rate: self.sample_rate,
            source_root_rotation: retarget::euler_rotation_quaternion(
                self.source_edits.rotate_roots_degrees,
            )?,
            source_root_scale: self.source_edits.scale_roots,
            source_root_translation: self.source_edits.translate_roots,
            ..RetargetOptions::default()
        })
    }
}
/// Pure adapter input: no window, renderer, or widget state enters the core.
pub struct SnapshotExportSettings {
    pub source_skin: usize,
    pub target_skin: usize,
    pub source_axes: [String; 3],
    pub edits: EditSpec,
    pub options: RetargetOptions,
    pub clip_name: String,
    pub selection: GlbExportSelection,
    pub reduce_keys: Option<f32>,
    pub overwrite: bool,
}
pub enum Source {
    Bvh {
        document: BvhDocument,
        descriptor: SkeletonDescriptor,
    },
    Glb {
        document: GlbDocument,
        runtime: AnimationRuntime,
        clip_index: usize,
        descriptor: SkeletonDescriptor,
    },
}
impl Source {
    pub fn descriptor(&self) -> &SkeletonDescriptor {
        match self {
            Self::Bvh { descriptor, .. } | Self::Glb { descriptor, .. } => {
                descriptor
            }
        }
    }
}
pub struct Target {
    pub document: GlbDocument,
    pub skin: SkinData,
    pub descriptor: SkeletonDescriptor,
}

pub fn glb_source(
    document: &GlbDocument,
    directory: Option<&Path>,
    clip_index: usize,
    skin: usize,
    edits: &EditSpec,
    axes: [&str; 3],
) -> Result<Source, OperationError> {
    let mut pending = edits.clone();
    pending.bake_root_transform = false;
    let document = super::glb::edit_snapshot(document, &pending)?;
    let bytes = document.to_bytes()?;
    let runtime = AnimationRuntime::from_bytes_skeleton_only(&bytes, directory)
        .map_err(|e| OperationError::validation(e.to_string()))?;
    let clip = runtime.clips.get(clip_index).ok_or_else(|| {
        OperationError::validation(format!(
            "Source animation {clip_index} does not exist"
        ))
    })?;
    if !clip.is_playable() {
        return Err(OperationError::validation(format!(
            "Source animation is unsupported: {}",
            clip.unsupported.join(", ")
        )));
    }
    let animated = clip.channels.iter().map(|c| c.node).collect::<HashSet<_>>();
    let descriptor = SkeletonDescriptor::from_runtime(
        &runtime,
        &document,
        skin,
        &animated,
        retarget::sha256_hex(&bytes),
        axes[0].into(),
        axes[1].into(),
        axes[2].into(),
    )?;
    Ok(Source::Glb {
        document,
        runtime,
        clip_index,
        descriptor,
    })
}
pub fn load_source(job: &RetargetJob) -> Result<Source, OperationError> {
    match job
        .source
        .extension()
        .and_then(|v| v.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("bvh") => {
            let edits = &job.source_edits;
            if !edits.textures.is_empty()
                || edits.animation_rate.is_some()
                || edits.smart_loop.is_some()
            {
                return Err(OperationError::validation("BVH sources support trim and root transforms, not GLB texture or animation edits"));
            }
            let mut document = BvhDocument::load(&job.source)?;
            if let Some(trim) = &edits.trim {
                if trim.animation != 0 {
                    return Err(OperationError::validation(
                        "BVH trim animation must be zero",
                    ));
                }
                document.trim(trim.start, trim.end)?;
            }
            let descriptor = SkeletonDescriptor::from_bvh(
                &document,
                file_sha256(&job.source)?,
                job.source_up_axis.clone(),
                job.source_forward_axis.clone(),
                job.source_unit.clone().unwrap_or_else(|| "cm".into()),
            )?;
            Ok(Source::Bvh {
                document,
                descriptor,
            })
        }
        Some("glb") => glb_source(
            &GlbDocument::load(&job.source)?,
            job.source.parent(),
            job.source_animation,
            job.source_skin,
            &job.source_edits,
            [
                &job.source_up_axis,
                &job.source_forward_axis,
                job.source_unit.as_deref().unwrap_or("m"),
            ],
        ),
        _ => Err(OperationError::validation(
            "Retarget source must be .bvh or .glb",
        )),
    }
}
pub fn target(
    document: GlbDocument,
    skin: usize,
    fingerprint: String,
    axes: [&str; 3],
) -> Result<Target, OperationError> {
    let skin = document.skin_data_at(skin)?;
    let descriptor = SkeletonDescriptor::from_skin(
        &skin,
        SourceKind::Glb,
        fingerprint,
        String::new(),
        axes[0],
        axes[1],
        axes[2],
        &HashSet::new(),
    )?;
    Ok(Target {
        document,
        skin,
        descriptor,
    })
}
pub fn load_target(job: &RetargetJob) -> Result<Target, OperationError> {
    target(
        GlbDocument::load(&job.target)?,
        job.skin,
        file_sha256(&job.target)?,
        [
            &job.target_up_axis,
            &job.target_forward_axis,
            &job.target_unit,
        ],
    )
}
pub fn load_mapping(
    path: &Path,
    source: &Source,
    target: &Target,
) -> Result<SkeletonMapping, OperationError> {
    match retarget::load_mapping(path) {
        Ok(mapping) => Ok(mapping),
        Err(error) => {
            if matches!(source, Source::Bvh { .. }) {
                if let Ok(legacy) = crate::modules::bvh::load_mapping(path) {
                    return Ok(retarget::from_legacy_bvh_mapping(
                        &legacy,
                        source.descriptor(),
                        &target.descriptor,
                    )?);
                }
            }
            Err(error.into())
        }
    }
}
pub fn prompt(
    source: &Source,
    target: &Target,
    candidate: Option<&SkeletonMapping>,
) -> Result<String, OperationError> {
    Ok(match source {
        Source::Bvh {
            document,
            descriptor,
        } => retarget::build_bvh_agent_prompt(
            document,
            descriptor,
            &target.descriptor,
            candidate,
        )?,
        Source::Glb {
            runtime,
            clip_index,
            descriptor,
            ..
        } => retarget::build_agent_prompt(
            descriptor,
            &target.descriptor,
            runtime.clips.get(*clip_index),
            candidate,
        )?,
    })
}
pub fn validate(
    source: &Source,
    target: &Target,
    mapping: &SkeletonMapping,
) -> MappingValidationReport {
    retarget::validate_mapping(mapping, source.descriptor(), &target.descriptor)
}
pub fn prepare(
    source: &Source,
    target: &Target,
    mapping: &SkeletonMapping,
    job: &RetargetJob,
) -> Result<crate::modules::glb::pipeline::ExportJob, OperationError> {
    let validation = validate(source, target, mapping);
    if !validation.is_valid() {
        return Err(OperationError::validation(validation.errors.join("; ")));
    }
    let clip = match source {
        Source::Bvh { document, .. } => {
            retarget_export::retarget_clip_from_bvh(
                document,
                &target.skin,
                mapping,
                job.options()?,
                &job.clip_name,
                job.reduce_keys,
            )?
        }
        Source::Glb {
            document,
            clip_index,
            ..
        } => retarget_export::retarget_clip_from_glb(
            document,
            job.source.parent(),
            *clip_index,
            &target.skin,
            mapping,
            job.options()?,
            &job.clip_name,
            job.reduce_keys,
        )?,
    };
    let mut document = target.document.clone();
    retarget_export::apply_retarget_clip(&mut document, clip)?;
    let selection = if let Some(spec) = &job.selection {
        spec.resolve(&document)?
    } else {
        GlbExportSelection {
            preset: job.preset.into(),
            skin_index: Some(job.skin),
            selected_animations: std::collections::BTreeSet::from([0]),
            ..document.default_export_selection()?
        }
    };
    if selection.animation_output
        != crate::modules::glb::AnimationOutputMode::Combined
    {
        return Err(OperationError::validation(
            "Retarget output contains one combined clip",
        ));
    }
    let output = job.out.as_ref().ok_or_else(|| {
        OperationError::validation("Retarget output is required")
    })?;
    let mut inputs = vec![job.source.as_path(), job.target.as_path()];
    if let Some(mapping) = job.mapping.as_deref() {
        inputs.push(mapping);
    }
    check_output(output, &inputs, job.overwrite)?;
    let jobs = super::glb::prepare_export(
        &document,
        &Default::default(),
        &selection,
        output,
        &inputs,
        job.overwrite,
    )?;
    jobs.into_iter().next().ok_or_else(|| {
        OperationError::validation("Retarget produced no output job")
    })
}

pub fn document_fingerprint(
    document: &GlbDocument,
) -> Result<String, OperationError> {
    match document.source_path.as_deref() {
        Some(path) if !document.dirty => file_sha256(path),
        _ => Ok(retarget::sha256_hex(&document.to_bytes()?)),
    }
}
pub fn bvh_source(
    document: BvhDocument,
    axes: [&str; 3],
) -> Result<Source, OperationError> {
    let hash = match document.source_path.as_deref() {
        Some(path) => file_sha256(path)?,
        None => retarget::sha256_hex(document.to_text()?.as_bytes()),
    };
    let descriptor = SkeletonDescriptor::from_bvh(
        &document,
        hash,
        axes[0].to_owned(),
        axes[1].to_owned(),
        axes[2].to_owned(),
    )?;
    Ok(Source::Bvh {
        document,
        descriptor,
    })
}
