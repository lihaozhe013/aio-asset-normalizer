//! CLI adapters for shared skeleton context and retarget requests.
use crate::output::{self, CliError};
use aio_asset_normalizer::modules::operations::spec::PresetArg;
use aio_asset_normalizer::modules::operations::{self, retarget as core};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Args)]
pub struct RetargetArgs {
    #[command(subcommand)]
    pub command: RetargetCommand,
}
#[derive(Subcommand)]
pub enum RetargetCommand {
    Prompt(PromptArgs),
    Suggest(SuggestArgs),
    Validate(ValidateArgs),
    Run(RunArgs),
}
#[derive(Args)]
pub struct ContextArgs {
    #[arg(long)]
    pub job: Option<PathBuf>,
    #[arg(long)]
    pub source: Option<PathBuf>,
    #[arg(long)]
    pub target: Option<PathBuf>,
    #[arg(long)]
    pub source_animation: Option<usize>,
    #[arg(long)]
    pub source_skin: Option<usize>,
    #[arg(long)]
    pub skin: Option<usize>,
    #[arg(long, allow_hyphen_values = true)]
    pub source_up_axis: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    pub source_forward_axis: Option<String>,
    #[arg(long)]
    pub source_unit: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    pub target_up_axis: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    pub target_forward_axis: Option<String>,
    #[arg(long)]
    pub target_unit: Option<String>,
}
impl ContextArgs {
    fn resolve(&self, command: &str) -> Result<core::RetargetJob, CliError> {
        if let Some(path) = &self.job {
            let request: core::RetargetJob =
                operations::load_json(path, "retarget job")
                    .map_err(output::operation_error)?;
            if request.command.as_deref().is_some_and(|v| v != command) {
                return Err(CliError::validation(
                    "Retarget job declares a different command",
                ));
            }
            return Ok(request);
        }
        let mut request = core::RetargetJob::new(
            self.source.clone().ok_or_else(|| {
                CliError::usage("--source is required without --job")
            })?,
            self.target.clone().ok_or_else(|| {
                CliError::usage("--target is required without --job")
            })?,
        );
        if let Some(value) = self.source_animation {
            request.source_animation = value;
        }
        if let Some(value) = self.source_skin {
            request.source_skin = value;
        }
        if let Some(value) = self.skin {
            request.skin = value;
        }
        if let Some(value) = &self.source_up_axis {
            request.source_up_axis = value.clone();
        }
        if let Some(value) = &self.source_forward_axis {
            request.source_forward_axis = value.clone();
        }
        request.source_unit = self.source_unit.clone();
        if let Some(value) = &self.target_up_axis {
            request.target_up_axis = value.clone();
        }
        if let Some(value) = &self.target_forward_axis {
            request.target_forward_axis = value.clone();
        }
        if let Some(value) = &self.target_unit {
            request.target_unit = value.clone();
        }
        Ok(request)
    }
}
#[derive(Args)]
pub struct PromptArgs {
    #[command(flatten)]
    pub context: ContextArgs,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub mapping: Option<PathBuf>,
    #[arg(long)]
    pub overwrite: bool,
    #[arg(long)]
    pub dry_run: bool,
}
#[derive(Args)]
pub struct SuggestArgs {
    #[command(flatten)]
    pub context: ContextArgs,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub overwrite: bool,
}
#[derive(Args)]
pub struct ValidateArgs {
    #[command(flatten)]
    pub context: ContextArgs,
    #[arg(long)]
    pub mapping: Option<PathBuf>,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum RetargetPresetArg {
    Character,
    Skeleton,
}
#[derive(Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub context: ContextArgs,
    #[arg(long)]
    pub mapping: Option<PathBuf>,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub clip_name: Option<String>,
    #[arg(long)]
    pub sample_rate: Option<f32>,
    #[arg(long)]
    pub no_root_motion: bool,
    #[arg(long)]
    pub normalize_heading: bool,
    #[arg(long)]
    pub reduce_keys: Option<f32>,
    #[arg(long, value_enum)]
    pub preset: Option<RetargetPresetArg>,
    #[arg(long)]
    pub overwrite: bool,
    #[arg(long)]
    pub dry_run: bool,
}
pub fn run(args: RetargetArgs) -> i32 {
    match args.command {
        RetargetCommand::Prompt(v) => prompt(&v),
        RetargetCommand::Suggest(v) => suggest(&v),
        RetargetCommand::Validate(v) => validate(&v),
        RetargetCommand::Run(v) => retarget(&v),
    }
}
fn load(
    request: &core::RetargetJob,
) -> Result<(core::Source, core::Target), CliError> {
    Ok((
        core::load_source(request).map_err(output::operation_error)?,
        core::load_target(request).map_err(output::operation_error)?,
    ))
}
fn mapping(
    request: &core::RetargetJob,
    source: &core::Source,
    target: &core::Target,
) -> Result<aio_asset_normalizer::modules::retarget::SkeletonMapping, CliError>
{
    core::load_mapping(
        request.mapping.as_deref().ok_or_else(|| {
            CliError::usage("--mapping or job mapping is required")
        })?,
        source,
        target,
    )
    .map_err(output::operation_error)
}
fn protected_write(
    request: &core::RetargetJob,
    bytes: &[u8],
) -> Result<(), CliError> {
    let path = request
        .out
        .as_deref()
        .ok_or_else(|| CliError::usage("--out or job out is required"))?;
    let mut sources = vec![request.source.as_path(), request.target.as_path()];
    if let Some(mapping) = request.mapping.as_deref() {
        sources.push(mapping);
    }
    operations::check_output(path, &sources, request.overwrite)
        .map_err(output::operation_error)?;
    aio_asset_normalizer::modules::atomic_file::write(
        path,
        bytes,
        request.overwrite,
    )
    .map_err(|e| CliError::io(e.to_string()))
}
fn prompt(args: &PromptArgs) -> i32 {
    const COMMAND: &str = "retarget.prompt";
    let work = (|| {
        let mut request = args.context.resolve(COMMAND)?;
        if args.context.job.is_none() {
            request.out = args.out.clone();
            request.mapping = args.mapping.clone();
            request.overwrite = args.overwrite;
        }
        let (source, target) = load(&request)?;
        let candidate = if request.mapping.is_some() {
            Some(mapping(&request, &source, &target)?)
        } else {
            None
        };
        let prompt = core::prompt(&source, &target, candidate.as_ref())
            .map_err(output::operation_error)?;
        let out = request
            .out
            .as_deref()
            .ok_or_else(|| CliError::usage("--out or job out is required"))?;
        let sources: Vec<_> = [
            Some(request.source.as_path()),
            Some(request.target.as_path()),
            request.mapping.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        operations::check_output(out, &sources, request.overwrite)
            .map_err(output::operation_error)?;
        if !args.dry_run {
            protected_write(&request, prompt.as_bytes())?;
        }
        Ok::<_, CliError>(
            json!({"source":request.source,"target":request.target,"output":out,"bytes":prompt.len(),"lines":prompt.lines().count(),"dry_run":args.dry_run,"prompt":prompt}),
        )
    })();
    match work {
        Ok(value) => output::emit_success(COMMAND, value, vec![]),
        Err(e) => output::emit_failure(COMMAND, json!({}), &e),
    }
}
fn suggest(args: &SuggestArgs) -> i32 {
    const COMMAND: &str = "retarget.suggest";
    let work = (|| {
        let mut request = args.context.resolve(COMMAND)?;
        if args.context.job.is_none() {
            request.out = args.out.clone();
            request.overwrite = args.overwrite;
        }
        let (source, target) = load(&request)?;
        let core::Source::Bvh { document, .. } = &source else {
            return Err(CliError::validation("Name suggestions require a BVH source; use retarget prompt for GLB"));
        };
        let value = json!({"source":request.source,"target":request.target,"skin_index":request.skin,
            "suggestions":document.suggest_mapping(&target.skin).iter().map(|v| json!({"source_joint":v.source_joint,"target_node":v.target_node,
                "confidence": match v.confidence { aio_asset_normalizer::modules::bvh::SuggestionConfidence::Exact => "exact", aio_asset_normalizer::modules::bvh::SuggestionConfidence::Normalized => "normalized" }})).collect::<Vec<_>>() });
        if request.out.is_some() {
            let bytes = serde_json::to_vec_pretty(&value)
                .map_err(|e| CliError::validation(e.to_string()))?;
            protected_write(&request, &bytes)?;
        }
        Ok::<_, CliError>(value)
    })();
    match work {
        Ok(value) => output::emit_success(COMMAND, value, vec![]),
        Err(e) => output::emit_failure(COMMAND, json!({}), &e),
    }
}
fn validate(args: &ValidateArgs) -> i32 {
    const COMMAND: &str = "retarget.validate";
    let work = (|| {
        let mut request = args.context.resolve(COMMAND)?;
        if args.context.job.is_none() {
            request.mapping = args.mapping.clone();
        }
        let (source, target) = load(&request)?;
        let report = core::validate(
            &source,
            &target,
            &mapping(&request, &source, &target)?,
        );
        Ok::<_, CliError>((
            json!({"mapping":request.mapping,"valid":report.is_valid(),"mapped_count":report.mapped_count,"unmapped_source_nodes":report.unmapped_source_nodes,"errors":report.errors,"warnings":report.warnings}),
            report,
        ))
    })();
    match work {
        Ok((value, report)) if report.is_valid() => {
            output::emit_success(COMMAND, value, report.warnings)
        }
        Ok((value, report)) => output::emit_failure(
            COMMAND,
            value,
            &CliError::validation(report.errors.join("; ")),
        ),
        Err(e) => output::emit_failure(COMMAND, json!({}), &e),
    }
}
fn retarget(args: &RunArgs) -> i32 {
    const COMMAND: &str = "retarget.run";
    let work = (|| {
        let mut request = args.context.resolve(COMMAND)?;
        if args.context.job.is_none() {
            request.mapping = args.mapping.clone();
            request.out = args.out.clone();
            request.overwrite = args.overwrite;
            if let Some(value) = &args.clip_name {
                request.clip_name = value.clone();
            }
            if let Some(value) = args.sample_rate {
                request.sample_rate = value;
            }
            request.root_motion = !args.no_root_motion;
            request.normalize_heading = args.normalize_heading;
            request.reduce_keys = args.reduce_keys;
            if let Some(value) = args.preset {
                request.preset = match value {
                    RetargetPresetArg::Character => PresetArg::Character,
                    RetargetPresetArg::Skeleton => PresetArg::Skeleton,
                };
            }
        }
        let (source, target) = load(&request)?;
        let mapping = mapping(&request, &source, &target)?;
        let validation = core::validate(&source, &target, &mapping);
        let job = core::prepare(&source, &target, &mapping, &request)
            .map_err(output::operation_error)?;
        let report = if args.dry_run {
            operations::glb::preview_job(&job)
        } else {
            operations::glb::execute_job(&job, request.overwrite)
        }
        .map_err(output::operation_error)?;
        Ok::<_, CliError>((
            json!({"source":request.source,"target":request.target,"output":job.path,"preset":request.preset,"dry_run":args.dry_run,"report":report_json(&report)}),
            validation.warnings,
        ))
    })();
    match work {
        Ok((value, warnings)) => output::emit_success(COMMAND, value, warnings),
        Err(e) => output::emit_failure(COMMAND, json!({}), &e),
    }
}
fn report_json(
    report: &aio_asset_normalizer::modules::glb::GlbExportReport,
) -> Value {
    let mut value = json!(report);
    if let Some(fields) = value.as_object_mut() {
        fields.insert(
            "source_animations".into(),
            json!(report.source.animations),
        );
        fields.insert(
            "output_animations".into(),
            json!(report.output.animations),
        );
        fields.insert("output_nodes".into(), json!(report.output.nodes));
        fields.insert("output_meshes".into(), json!(report.output.meshes));
    }
    value
}
