//! `retarget` subcommands: prompt, suggest, validate, and run.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand, ValueEnum};
use serde_json::{json, Value};

use aio_asset_normalizer::modules::glb::{
    AnimationRuntime, GlbDocument, GlbExportPreset, GlbExportSelection,
    SkinData,
};
use aio_asset_normalizer::modules::bvh::BvhDocument;
use aio_asset_normalizer::modules::retarget::{
    self, RetargetOptions, SkeletonDescriptor, SkeletonMapping,
    SourceKind,
};
use aio_asset_normalizer::modules::retarget_export;

use crate::output::{self, CliError};
use crate::util::{file_sha256, same_path};

#[derive(Args)]
pub struct RetargetArgs {
    #[command(subcommand)]
    pub command: RetargetCommand,
}

#[derive(Subcommand)]
pub enum RetargetCommand {
    /// Write a deterministic agent prompt for authoring a Mapping v2
    Prompt(PromptArgs),
    /// List name-match suggestions for a BVH source
    Suggest(SuggestArgs),
    /// Validate a Mapping against concrete source and target skeletons
    Validate(ValidateArgs),
    /// Retarget a BVH or GLB animation onto a target GLB
    Run(RunArgs),
}

#[derive(Args, Clone)]
pub struct SourceArgs {
    /// BVH or animated GLB source
    #[arg(long, value_name = "FILE")]
    pub source: PathBuf,
    /// Animation index inside a GLB source
    #[arg(long, value_name = "INDEX", default_value_t = 0)]
    pub source_animation: usize,
    /// Skin index inside a GLB source
    #[arg(long, value_name = "INDEX", default_value_t = 0)]
    pub source_skin: usize,
    /// Source up axis
    #[arg(long, value_name = "AXIS", default_value = "Y")]
    pub source_up_axis: String,
    /// Source forward axis
    #[arg(long, value_name = "AXIS", default_value = "-Z")]
    pub source_forward_axis: String,
    /// Source unit; defaults to cm for BVH and m for GLB
    #[arg(long, value_name = "UNIT")]
    pub source_unit: Option<String>,
}

#[derive(Args, Clone)]
pub struct TargetArgs {
    /// Target Skinned GLB
    #[arg(long, value_name = "FILE")]
    pub target: PathBuf,
    /// Skin index inside the target GLB
    #[arg(long, value_name = "INDEX", default_value_t = 0)]
    pub skin: usize,
    /// Target up axis
    #[arg(long, value_name = "AXIS", default_value = "Y")]
    pub target_up_axis: String,
    /// Target forward axis
    #[arg(long, value_name = "AXIS", default_value = "-Z")]
    pub target_forward_axis: String,
    /// Target unit
    #[arg(long, value_name = "UNIT", default_value = "m")]
    pub target_unit: String,
}

#[derive(Args)]
pub struct PromptArgs {
    #[command(flatten)]
    pub source: SourceArgs,
    #[command(flatten)]
    pub target: TargetArgs,
    /// Destination Markdown prompt
    #[arg(long, value_name = "FILE")]
    pub out: PathBuf,
    /// Replace an existing prompt file
    #[arg(long)]
    pub overwrite: bool,
    /// Existing Mapping to include as a candidate
    #[arg(long, value_name = "FILE")]
    pub mapping: Option<PathBuf>,
}

#[derive(Args)]
pub struct SuggestArgs {
    #[command(flatten)]
    pub source: SourceArgs,
    #[command(flatten)]
    pub target: TargetArgs,
    /// Optional file for the suggestion list
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,
    /// Replace an existing suggestion file
    #[arg(long)]
    pub overwrite: bool,
}

#[derive(Args)]
pub struct ValidateArgs {
    #[command(flatten)]
    pub source: SourceArgs,
    #[command(flatten)]
    pub target: TargetArgs,
    /// Mapping v2 JSON to validate
    #[arg(long, value_name = "FILE")]
    pub mapping: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RetargetPresetArg {
    Character,
    Skeleton,
}

#[derive(Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub source: SourceArgs,
    #[command(flatten)]
    pub target: TargetArgs,
    /// Mapping v2 JSON (legacy v1 is accepted for BVH sources)
    #[arg(long, value_name = "FILE")]
    pub mapping: PathBuf,
    /// Destination GLB
    #[arg(long, value_name = "FILE")]
    pub out: PathBuf,
    /// Name of the generated animation clip
    #[arg(long, value_name = "NAME", default_value = "Retargeted")]
    pub clip_name: String,
    /// Baked animation sampling rate
    #[arg(long, value_name = "HZ", default_value_t = 60.0)]
    pub sample_rate: f32,
    /// Do not transfer root translation motion
    #[arg(long)]
    pub no_root_motion: bool,
    /// Normalize the initial heading
    #[arg(long)]
    pub normalize_heading: bool,
    /// Remove redundant sampled keys below TOLERANCE
    #[arg(long, value_name = "TOLERANCE")]
    pub reduce_keys: Option<f32>,
    /// Target package: full character or skeleton plus animation only
    #[arg(long, value_enum, default_value = "character")]
    pub preset: RetargetPresetArg,
    /// Replace an existing output
    #[arg(long)]
    pub overwrite: bool,
}

pub fn run(args: RetargetArgs) -> i32 {
    match args.command {
        RetargetCommand::Prompt(args) => prompt(&args),
        RetargetCommand::Suggest(args) => suggest(&args),
        RetargetCommand::Validate(args) => validate(&args),
        RetargetCommand::Run(args) => retarget(&args),
    }
}

// ---- loading ---------------------------------------------------------------

enum Source {
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
    fn descriptor(&self) -> &SkeletonDescriptor {
        match self {
            Self::Bvh { descriptor, .. } | Self::Glb { descriptor, .. } => {
                descriptor
            }
        }
    }
}

struct Target {
    document: GlbDocument,
    skin: SkinData,
    descriptor: SkeletonDescriptor,
}

fn is_bvh(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bvh"))
}

fn load_source(args: &SourceArgs) -> Result<Source, CliError> {
    let default_unit = if is_bvh(&args.source) { "cm" } else { "m" };
    let unit = args
        .source_unit
        .clone()
        .unwrap_or_else(|| default_unit.to_owned());

    if is_bvh(&args.source) {
        let document =
            BvhDocument::load(&args.source).map_err(output::bvh_error)?;
        let descriptor = SkeletonDescriptor::from_bvh(
            &document,
            file_sha256(&args.source),
            args.source_up_axis.clone(),
            args.source_forward_axis.clone(),
            unit,
        )
        .map_err(output::retarget_error)?;
        return Ok(Source::Bvh {
            document,
            descriptor,
        });
    }

    let document =
        GlbDocument::load(&args.source).map_err(output::glb_error)?;
    let bytes = std::fs::read(&args.source)
        .map_err(|error| CliError::io(error.to_string()))?;
    let runtime = AnimationRuntime::from_bytes_skeleton_only(
        &bytes,
        args.source.parent(),
    )
    .map_err(|error| CliError::validation(error.to_string()))?;
    let clip_index = args.source_animation;
    let clip = runtime.clips.get(clip_index).ok_or_else(|| {
        CliError::validation(format!(
            "source animation {clip_index} does not exist"
        ))
    })?;
    if !clip.is_playable() {
        return Err(CliError::validation(format!(
            "source animation {clip_index} is unsupported: {}",
            clip.unsupported.join(", ")
        )));
    }
    let animated_nodes = clip
        .channels
        .iter()
        .map(|channel| channel.node)
        .collect::<HashSet<_>>();
    let descriptor = SkeletonDescriptor::from_runtime(
        &runtime,
        &document,
        args.source_skin,
        &animated_nodes,
        file_sha256(&args.source),
        args.source_up_axis.clone(),
        args.source_forward_axis.clone(),
        unit,
    )
    .map_err(output::retarget_error)?;
    Ok(Source::Glb {
        document,
        runtime,
        clip_index,
        descriptor,
    })
}

fn load_target(args: &TargetArgs) -> Result<Target, CliError> {
    let document =
        GlbDocument::load(&args.target).map_err(output::glb_error)?;
    let skin = document
        .skin_data_at(args.skin)
        .map_err(output::glb_error)?;
    let descriptor = SkeletonDescriptor::from_skin(
        &skin,
        SourceKind::Glb,
        file_sha256(&args.target),
        String::new(),
        args.target_up_axis.clone(),
        args.target_forward_axis.clone(),
        args.target_unit.clone(),
        &HashSet::new(),
    )
    .map_err(output::retarget_error)?;
    Ok(Target {
        document,
        skin,
        descriptor,
    })
}

fn load_mapping(
    path: &Path,
    source: &Source,
    target: &Target,
) -> Result<SkeletonMapping, CliError> {
    match retarget::load_mapping(path) {
        Ok(mapping) => Ok(mapping),
        Err(v2_error) => {
            if let Source::Bvh { .. } = source {
                if let Ok(legacy) =
                    aio_asset_normalizer::modules::bvh::load_mapping(path)
                {
                    return retarget::from_legacy_bvh_mapping(
                        &legacy,
                        source.descriptor(),
                        &target.descriptor,
                    )
                    .map_err(output::retarget_error);
                }
            }
            Err(output::retarget_error(v2_error))
        }
    }
}

// ---- handlers --------------------------------------------------------------

fn prompt(args: &PromptArgs) -> i32 {
    const COMMAND: &str = "retarget.prompt";
    let fail = |error: CliError| output::emit_failure(COMMAND, json!({}), &error);

    let source = match load_source(&args.source) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let target = match load_target(&args.target) {
        Ok(target) => target,
        Err(error) => return fail(error),
    };
    let candidate = match &args.mapping {
        Some(path) => match load_mapping(path, &source, &target) {
            Ok(mapping) => Some(mapping),
            Err(error) => return fail(error),
        },
        None => None,
    };

    let prompt = match &source {
        Source::Bvh {
            document,
            descriptor,
        } => retarget::build_bvh_agent_prompt(
            document,
            descriptor,
            &target.descriptor,
            candidate.as_ref(),
        ),
        Source::Glb {
            runtime,
            clip_index,
            descriptor,
            ..
        } => {
            let Some(clip) = runtime.clips.get(*clip_index) else {
                return fail(CliError::validation(
                    "source animation is unavailable",
                ));
            };
            retarget::build_agent_prompt(
                descriptor,
                &target.descriptor,
                Some(clip),
                candidate.as_ref(),
            )
        }
    };
    let prompt = match prompt {
        Ok(prompt) => prompt,
        Err(error) => return fail(output::retarget_error(error)),
    };

    if same_path(&args.out, &args.source.source) {
        return fail(CliError::validation(
            "prompt output must not replace the source file",
        ));
    }
    if args.out.exists() && !args.overwrite {
        return fail(CliError::validation(format!(
            "prompt already exists: {} (pass --overwrite)",
            args.out.display()
        )));
    }
    if let Err(error) = retarget::save_agent_prompt(&args.out, &prompt) {
        return fail(output::retarget_error(error));
    }

    output::emit_success(
        COMMAND,
        json!({
            "source": args.source.source.display().to_string(),
            "target": args.target.target.display().to_string(),
            "output": args.out.display().to_string(),
            "bytes": prompt.len(),
            "lines": prompt.lines().count(),
        }),
        Vec::new(),
    )
}

fn suggest(args: &SuggestArgs) -> i32 {
    const COMMAND: &str = "retarget.suggest";
    let fail = |error: CliError| output::emit_failure(COMMAND, json!({}), &error);

    let source = match load_source(&args.source) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let target = match load_target(&args.target) {
        Ok(target) => target,
        Err(error) => return fail(error),
    };

    let Source::Bvh { document, .. } = &source else {
        return fail(CliError::validation(
            "name suggestions require a BVH source; use retarget prompt for GLB sources",
        ));
    };
    let suggestions = document.suggest_mapping(&target.skin);
    let value = json!({
        "source": args.source.source.display().to_string(),
        "target": args.target.target.display().to_string(),
        "skin_index": args.target.skin,
        "suggestions": suggestions
            .iter()
            .map(|suggestion| json!({
                "source_joint": suggestion.source_joint,
                "target_node": suggestion.target_node,
                "confidence": match suggestion.confidence {
                    aio_asset_normalizer::modules::bvh::SuggestionConfidence::Exact => "exact",
                    aio_asset_normalizer::modules::bvh::SuggestionConfidence::Normalized => "normalized",
                },
            }))
            .collect::<Vec<_>>(),
    });

    if let Some(out) = &args.out {
        if out.exists() && !args.overwrite {
            return fail(CliError::validation(format!(
                "suggestion file already exists: {} (pass --overwrite)",
                out.display()
            )));
        }
        let text = match serde_json::to_string_pretty(&value) {
            Ok(text) => text,
            Err(error) => return fail(CliError::validation(error.to_string())),
        };
        if let Err(error) = std::fs::write(out, text) {
            return fail(CliError::io(error.to_string()));
        }
    }

    output::emit_success(COMMAND, value, Vec::new())
}

fn validate(args: &ValidateArgs) -> i32 {
    const COMMAND: &str = "retarget.validate";
    let fail = |error: CliError| output::emit_failure(COMMAND, json!({}), &error);

    let source = match load_source(&args.source) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let target = match load_target(&args.target) {
        Ok(target) => target,
        Err(error) => return fail(error),
    };
    let mapping = match load_mapping(&args.mapping, &source, &target) {
        Ok(mapping) => mapping,
        Err(error) => return fail(error),
    };

    let report = retarget::validate_mapping(
        &mapping,
        source.descriptor(),
        &target.descriptor,
    );
    let value = json!({
        "mapping": args.mapping.display().to_string(),
        "valid": report.is_valid(),
        "mapped_count": report.mapped_count,
        "unmapped_source_nodes": report.unmapped_source_nodes,
        "errors": report.errors,
        "warnings": report.warnings,
    });

    if report.is_valid() {
        output::emit_success(COMMAND, value, report.warnings)
    } else {
        let message = if report.errors.is_empty() {
            "mapping maps no source nodes".to_owned()
        } else {
            report.errors.join("; ")
        };
        output::emit_failure(COMMAND, value, &CliError::validation(message))
    }
}

fn retarget(args: &RunArgs) -> i32 {
    const COMMAND: &str = "retarget.run";
    let fail = |error: CliError| output::emit_failure(COMMAND, json!({}), &error);

    let source = match load_source(&args.source) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let target = match load_target(&args.target) {
        Ok(target) => target,
        Err(error) => return fail(error),
    };
    let mapping = match load_mapping(&args.mapping, &source, &target) {
        Ok(mapping) => mapping,
        Err(error) => return fail(error),
    };

    let report = retarget::validate_mapping(
        &mapping,
        source.descriptor(),
        &target.descriptor,
    );
    if !report.is_valid() {
        let message = if report.errors.is_empty() {
            "mapping maps no source nodes".to_owned()
        } else {
            report.errors.join("; ")
        };
        return output::emit_failure(
            COMMAND,
            json!({
                "valid": false,
                "errors": report.errors,
                "warnings": report.warnings,
            }),
            &CliError::validation(message),
        );
    }

    let options = RetargetOptions {
        root_motion: !args.no_root_motion,
        normalize_initial_heading: args.normalize_heading,
        sample_rate: args.sample_rate,
        ..RetargetOptions::default()
    };

    let clip = match &source {
        Source::Bvh { document, .. } => {
            retarget_export::retarget_clip_from_bvh(
                document,
                &target.skin,
                &mapping,
                options,
                args.clip_name.clone(),
                args.reduce_keys,
            )
        }
        Source::Glb {
            document,
            clip_index,
            ..
        } => retarget_export::retarget_clip_from_glb(
            document,
            args.source.source.parent(),
            *clip_index,
            &target.skin,
            &mapping,
            options,
            args.clip_name.clone(),
            args.reduce_keys,
        ),
    };
    let clip = match clip {
        Ok(clip) => clip,
        Err(error) => return fail(output::retarget_export_error(error)),
    };

    if same_path(&args.out, &args.source.source)
        || same_path(&args.out, &args.target.target)
    {
        return fail(CliError::validation(
            "output must not replace the source or target GLB",
        ));
    }
    if args.out.exists() && !args.overwrite {
        return fail(CliError::validation(format!(
            "output already exists: {} (pass --overwrite)",
            args.out.display()
        )));
    }

    let selection = GlbExportSelection {
        preset: match args.preset {
            RetargetPresetArg::Character => GlbExportPreset::CharacterPackage,
            RetargetPresetArg::Skeleton => GlbExportPreset::SkeletonAnimation,
        },
        skin_index: Some(args.target.skin),
        selected_animations: std::collections::BTreeSet::from([0]),
        ..GlbExportSelection::default()
    };

    let exported = retarget_export::export_retargeted_glb(
        &target.document,
        clip,
        &selection,
        &args.out,
    );
    let exported = match exported {
        Ok(report) => report,
        Err(error) => return fail(output::retarget_export_error(error)),
    };

    if let Err(error) = GlbDocument::load(&args.out) {
        return fail(output::glb_error(error));
    }

    output::emit_success(
        COMMAND,
        json!({
            "source": args.source.source.display().to_string(),
            "target": args.target.target.display().to_string(),
            "output": args.out.display().to_string(),
            "preset": match args.preset {
                RetargetPresetArg::Character => "character",
                RetargetPresetArg::Skeleton => "skeleton",
            },
            "report": report_json(&exported),
        }),
        report.warnings,
    )
}

fn report_json(
    report: &aio_asset_normalizer::modules::glb::GlbExportReport,
) -> Value {
    json!({
        "source_animations": report.source.animations,
        "output_animations": report.output.animations,
        "output_nodes": report.output.nodes,
        "output_meshes": report.output.meshes,
        "output_bin_bytes": report.output_bin_bytes,
        "output_glb_bytes": report.output_glb_bytes,
    })
}