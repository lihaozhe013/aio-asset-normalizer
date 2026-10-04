//! `glb` subcommands: inspect, export, and job-driven edit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use aio_asset_normalizer::modules::glb::batch_runner::{
    run_export, run_preflight, BatchEntry, BatchFileStatus, BatchProgress,
    BatchRequest,
};
use aio_asset_normalizer::modules::glb::{
    GlbDocument, GlbExportCatalog, GlbExportReport, GlbSummary,
};
use aio_asset_normalizer::modules::logging::next_task_id;
use aio_asset_normalizer::modules::operations::glb as operations;

use crate::job::{
    self, AnimationOutputArg, EditJobFile, ExportJobFile, PresetArg,
    RecipeOptions, RootMotionModeArg,
};
use crate::output::{self, CliError};
use crate::util::common_parent;

#[derive(Args)]
pub struct GlbArgs {
    #[command(subcommand)]
    pub command: GlbCommand,
}

#[derive(Subcommand)]
pub enum GlbCommand {
    /// Print summary, catalog, and Skin hierarchies
    Inspect(InspectArgs),
    /// Batch-standardize GLB files through a shared recipe
    Export(ExportArgs),
    /// Apply one JSON edit job and export the result
    Edit(EditArgs),
}

#[derive(Args)]
pub struct InspectArgs {
    /// GLB files to inspect
    #[arg(value_name = "GLB", required = true)]
    pub inputs: Vec<PathBuf>,
}

#[derive(Args)]
pub struct ExportArgs {
    /// Source GLB files
    #[arg(value_name = "GLB")]
    pub inputs: Vec<PathBuf>,
    /// Directory the inputs are relative to; defaults to their shared parent
    #[arg(long, value_name = "DIR")]
    pub input_root: Option<PathBuf>,
    /// Recursively standardize every `.glb` below --input-root
    #[arg(long)]
    pub recursive: bool,
    /// Destination root directory
    #[arg(long, value_name = "DIR")]
    pub output_root: Option<PathBuf>,
    /// Export recipe
    #[arg(long, value_enum, default_value = "preserve-all")]
    pub preset: PresetArg,
    /// "auto" or an exact authored Skin name
    #[arg(long, default_value = job::AUTOMATIC_NAME, value_name = "NAME")]
    pub skin: String,
    /// One combined file or one file per animation
    #[arg(long, value_enum, default_value = "combined")]
    pub animation_output: AnimationOutputArg,
    /// Remove root translation motion from compact exports
    #[arg(long)]
    pub remove_root_motion: bool,
    /// Root motion axes to freeze
    #[arg(long, value_enum, default_value = "horizontal-xz")]
    pub root_motion_mode: RootMotionModeArg,
    /// "auto" or an exact authored node name
    #[arg(long, default_value = job::AUTOMATIC_NAME, value_name = "NAME")]
    pub root_motion_node: String,
    /// Replace existing outputs
    #[arg(long)]
    pub overwrite: bool,
    /// Validate and report planned outputs without writing
    #[arg(long)]
    pub dry_run: bool,
    /// JSON job file; when present, the option flags above are ignored
    #[arg(long, value_name = "FILE")]
    pub job: Option<PathBuf>,
}

#[derive(Args)]
pub struct EditArgs {
    /// JSON edit job: input, output, edits, and export recipe
    #[arg(long, value_name = "FILE")]
    pub job: PathBuf,
    /// Validate and report the result without writing
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(args: GlbArgs) -> i32 {
    match args.command {
        GlbCommand::Inspect(args) => inspect(&args),
        GlbCommand::Export(args) => export(&args),
        GlbCommand::Edit(args) => edit(&args),
    }
}

// ---- inspect ---------------------------------------------------------------

fn inspect(args: &InspectArgs) -> i32 {
    const COMMAND: &str = "glb.inspect";
    let mut files = Vec::new();
    let mut failures = Vec::new();
    let mut first_error: Option<CliError> = None;
    for input in &args.inputs {
        match inspect_one(input) {
            Ok(value) => files.push(value),
            Err(error) => {
                failures.push(error.message.clone());
                files.push(json!({
                    "path": input.display().to_string(),
                    "ok": false,
                    "error": {
                        "code": error.code.as_str(),
                        "message": error.message,
                    },
                }));
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        None => {
            output::emit_success(COMMAND, json!({ "files": files }), Vec::new())
        }
        Some(error) => output::emit_failure(
            COMMAND,
            json!({ "files": files }),
            &CliError {
                code: error.code,
                message: failures.join("; "),
            },
        ),
    }
}

fn inspect_one(path: &Path) -> Result<Value, CliError> {
    let document = GlbDocument::load(path).map_err(output::glb_error)?;
    let catalog = document.export_catalog().map_err(output::glb_error)?;
    let skins = catalog
        .skins
        .iter()
        .map(|skin| match document.skin_data_at(skin.index) {
            Ok(data) => json!({
                "index": data.index,
                "name": data.name,
                "skeleton": data.skeleton,
                "joints": data.joints,
                "mesh_nodes": data.mesh_nodes,
                "nodes": data
                    .nodes
                    .iter()
                    .map(|node| json!({
                        "index": node.index,
                        "name": node.name,
                        "parent": node.parent,
                        "translation": node.translation,
                        "rotation": node.rotation,
                        "scale": node.scale,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Err(error) => json!({
                "index": skin.index,
                "name": skin.name,
                "error": error.to_string(),
            }),
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "path": path.display().to_string(),
        "ok": true,
        "summary": summary_json(&document.summary()),
        "catalog": catalog_json(&catalog),
        "skins": skins,
    }))
}

// ---- export ----------------------------------------------------------------

fn export(args: &ExportArgs) -> i32 {
    const COMMAND: &str = "glb.export";
    let fail = |results: Value, error: CliError| {
        output::emit_failure(COMMAND, results, &error)
    };

    let mut inputs = args.inputs.clone();
    let mut input_root = args.input_root.clone();
    let mut output_root = args.output_root.clone();
    let mut overwrite = args.overwrite;
    let mut options = RecipeOptions {
        preset: args.preset.into(),
        skin: job::name_selector(&args.skin),
        animation_output: args.animation_output.into(),
        remove_root_motion: args.remove_root_motion,
        root_motion_mode: args.root_motion_mode.into(),
        root_motion_node: job::name_selector(&args.root_motion_node),
    };
    let mut dry_run = args.dry_run;
    let mut selection = None;

    if let Some(job_path) = &args.job {
        if !args.inputs.is_empty() || args.output_root.is_some() {
            return fail(
                json!({}),
                CliError::validation(
                    "--job cannot be combined with positional inputs or --output-root",
                ),
            );
        }
        let file: ExportJobFile = match job::load_json(job_path, "export job") {
            Ok(file) => file,
            Err(message) => return fail(json!({}), message),
        };
        if let Some(command) = &file.command {
            if command != "glb.export" {
                return fail(
                    json!({}),
                    CliError::validation(format!(
                        "export job declares command {command:?}, expected \"glb.export\""
                    )),
                );
            }
        }
        inputs = file.inputs;
        input_root = file.input_root;
        output_root = Some(file.output_root);
        overwrite = file.overwrite;
        if file.recipe.is_some() && file.selection.is_some() {
            return fail(
                json!({}),
                CliError::validation(
                    "Recipe and selection are mutually exclusive",
                ),
            );
        }
        let recipe = file.recipe.unwrap_or_default().to_options();
        options = RecipeOptions {
            preset: recipe.preset,
            skin: recipe.skin,
            animation_output: recipe.animation_output,
            remove_root_motion: recipe.remove_root_motion,
            root_motion_mode: recipe.root_motion_mode,
            root_motion_node: recipe.root_motion_node,
        };
        selection = file.selection;
        // A job file is explicit; --dry-run still applies from the flags.
        dry_run = args.dry_run;
    }

    if inputs.is_empty() {
        if args.recursive {
            let Some(root) = args.input_root.as_ref() else {
                return fail(
                    json!({}),
                    CliError::validation("--recursive requires --input-root"),
                );
            };
            match discover_glb_files(root, output_root.as_deref()) {
                Ok(found) => inputs = found,
                Err(error) => return fail(json!({}), error),
            }
        }
    }
    if inputs.is_empty() {
        return fail(
            json!({}),
            CliError::validation("no input GLB files were provided"),
        );
    }
    let Some(output_root) = output_root else {
        return fail(
            json!({}),
            CliError::validation("--output-root is required without --job"),
        );
    };
    let input_root = input_root.or_else(|| common_parent(&inputs));
    let Some(input_root) = input_root else {
        return fail(
            json!({}),
            CliError::validation(
                "cannot derive --input-root from the given inputs",
            ),
        );
    };

    let request = BatchRequest {
        input_root,
        inputs,
        output_root,
        recipe: options.to_recipe(),
        selection,
        overwrite_existing: overwrite,
    };

    let task_id = next_task_id();
    let (entries, all_valid) =
        run_preflight(&request, task_id, &mut |_event: BatchProgress| {});

    if !all_valid {
        return fail(
            json!({
                "dry_run": dry_run,
                "overwrite": overwrite,
                "entries": entries_json(&entries, None),
            }),
            CliError { code: if entries.iter().any(|e| e.error_kind == Some(aio_asset_normalizer::modules::operations::ErrorKind::Io)) { output::ErrorCode::Io } else { output::ErrorCode::Validation }, message: "preflight found errors".into() },
        );
    }

    if dry_run {
        return output::emit_success(
            COMMAND,
            json!({
                "dry_run": true,
                "overwrite": overwrite,
                "entries": entries_json(&entries, None),
            }),
            Vec::new(),
        );
    }

    let mut finished: HashMap<usize, (Vec<PathBuf>, Option<String>)> =
        HashMap::new();
    let mut progress = |event: BatchProgress| match event {
        BatchProgress::ExportFinished {
            index,
            completed,
            error,
        } => {
            finished.insert(index, (completed, error));
        }
        BatchProgress::ExportStarted { .. }
        | BatchProgress::PreflightFile { .. }
        | BatchProgress::PreflightFinished { .. } => {}
    };
    let result = run_export(&request, &entries, task_id, &mut progress);
    let results = json!({
        "dry_run": false,
        "overwrite": overwrite,
        "entries": entries_json(&entries, Some(&finished)),
    });

    match result {
        Ok(()) => output::emit_success(COMMAND, results, Vec::new()),
        Err(error) => fail(results, output::operation_error(error)),
    }
}

// ---- edit ------------------------------------------------------------------

fn edit(args: &EditArgs) -> i32 {
    const COMMAND: &str = "glb.edit";
    let fail = |results, error| output::emit_failure(COMMAND, results, &error);
    let file: EditJobFile = match job::load_json(&args.job, "edit job") {
        Ok(file) => file,
        Err(e) => return fail(json!({}), e),
    };
    if file.command.as_deref().is_some_and(|v| v != COMMAND) {
        return fail(
            json!({}),
            CliError::validation("Edit job declares a different command"),
        );
    }
    let prepared = (|| {
        let source = GlbDocument::load(&file.input)?;
        let edited = operations::edit_snapshot(&source, &file.edits)?;
        let selection = operations::resolve_selection(
            &edited,
            file.export.as_ref(),
            file.selection.as_ref(),
        )?;
        let jobs = operations::prepare_export(
            &edited,
            &file.edits.export_edits(),
            &selection,
            &file.output,
            &[&file.input, &args.job],
            file.overwrite,
        )?;
        Ok::<_, aio_asset_normalizer::modules::operations::OperationError>((
            jobs,
            edited.validate_export_selection(&selection).warnings,
        ))
    })();
    let (jobs, warnings) = match prepared {
        Ok(v) => v,
        Err(e) => return fail(json!({}), output::operation_error(e)),
    };
    let mut outputs = Vec::new();
    for job in &jobs {
        let result = if args.dry_run {
            operations::preview_job(job)
        } else {
            operations::execute_job(job, file.overwrite)
        };
        match result {
            Ok(report) => outputs
                .push(json!({"path":job.path,"report":report_json(&report)})),
            Err(e) => {
                return fail(
                    json!({"outputs":outputs}),
                    output::operation_error(e),
                )
            }
        }
    }
    output::emit_success(
        COMMAND,
        json!({"dry_run":args.dry_run,"input":file.input,"outputs":outputs,"warnings":warnings}),
        warnings,
    )
}

// ---- recursive input discovery ---------------------------------------------

fn discover_glb_files(
    root: &Path,
    output_root: Option<&Path>,
) -> Result<Vec<PathBuf>, CliError> {
    if !root.is_dir() {
        return Err(CliError::validation(format!(
            "--input-root is not a directory: {}",
            root.display()
        )));
    }
    if output_root.is_some_and(|out| {
        aio_asset_normalizer::modules::operations::same_path(out, root)
    }) {
        return Err(CliError::validation(
            "Recursive input and output roots must differ",
        ));
    }
    let excluded = output_root
        .map(aio_asset_normalizer::modules::operations::path_identity);
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| CliError::io(error.to_string()))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| CliError::io(error.to_string()))?;
            let file_type = entry
                .file_type()
                .map_err(|error| CliError::io(error.to_string()))?;
            if file_type.is_dir() {
                if excluded.as_ref().is_some_and(|out| {
                    aio_asset_normalizer::modules::operations::path_identity(
                        &entry.path(),
                    )
                    .starts_with(out)
                }) {
                    continue;
                }
                pending.push(entry.path());
            } else if file_type.is_file() && is_glb(&entry.path()) {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    Ok(found)
}

fn is_glb(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"))
}

// ---- JSON projection -------------------------------------------------------

fn summary_json(summary: &GlbSummary) -> Value {
    json!({
        "scenes": summary.scenes,
        "nodes": summary.nodes,
        "meshes": summary.meshes,
        "materials": summary.materials,
        "skins": summary.skins,
        "animations": summary.animations,
        "images": summary.images,
        "extensions": summary.extensions,
    })
}

fn report_json(report: &GlbExportReport) -> Value {
    json!({
        "source": summary_json(&report.source),
        "output": summary_json(&report.output),
        "removed_animation_channels": report.removed_animation_channels,
        "root_motion_channels_modified": report.root_motion_channels_modified,
        "source_bin_bytes": report.source_bin_bytes,
        "output_bin_bytes": report.output_bin_bytes,
        "source_glb_bytes": report.source_glb_bytes,
        "output_glb_bytes": report.output_glb_bytes,
    })
}

fn catalog_json(catalog: &GlbExportCatalog) -> Value {
    json!({
        "scenes": catalog.scenes.iter().map(|scene| json!({
            "index": scene.index,
            "name": scene.name,
            "roots": scene.roots,
        })).collect::<Vec<_>>(),
        "nodes": catalog.nodes.iter().map(|node| json!({
            "index": node.index,
            "name": node.name,
            "parent": node.parent,
            "children": node.children,
            "mesh": node.mesh,
            "skin": node.skin,
        })).collect::<Vec<_>>(),
        "meshes": catalog.meshes.iter().map(|mesh| json!({
            "index": mesh.index,
            "name": mesh.name,
            "primitives": mesh.primitives.iter().map(|primitive| json!({
                "index": primitive.index,
                "material": primitive.material,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "skins": catalog.skins.iter().map(|skin| json!({
            "index": skin.index,
            "name": skin.name,
            "joint_count": skin.joint_count,
        })).collect::<Vec<_>>(),
        "animations": catalog.animations.iter().map(|animation| json!({
            "index": animation.index,
            "name": animation.name,
        })).collect::<Vec<_>>(),
    })
}

fn status_name(status: BatchFileStatus) -> &'static str {
    match status {
        BatchFileStatus::Ready => "ready",
        BatchFileStatus::ReadyWithWarnings => "ready-with-warnings",
        BatchFileStatus::Error => "error",
        BatchFileStatus::Exporting => "exporting",
        BatchFileStatus::Succeeded => "succeeded",
        BatchFileStatus::Failed => "failed",
        BatchFileStatus::Skipped => "skipped",
    }
}

fn entries_json(
    entries: &[BatchEntry],
    finished: Option<&HashMap<usize, (Vec<PathBuf>, Option<String>)>>,
) -> Value {
    Value::Array(
        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| entry_json(index, entry, finished))
            .collect(),
    )
}

fn entry_json(
    index: usize,
    entry: &BatchEntry,
    finished: Option<&HashMap<usize, (Vec<PathBuf>, Option<String>)>>,
) -> Value {
    let (mut status, mut error, mut completed) = (
        entry.status,
        entry.error.clone(),
        entry.completed_outputs.clone(),
    );
    if let Some(finished) = finished {
        match finished.get(&index) {
            Some((paths, failed)) => {
                completed = paths.clone();
                match failed {
                    None => {
                        status = BatchFileStatus::Succeeded;
                        error = None;
                    }
                    Some(message) => {
                        status = BatchFileStatus::Failed;
                        error = Some(message.clone());
                    }
                }
            }
            None => {
                if entry.status == BatchFileStatus::Ready
                    || entry.status == BatchFileStatus::ReadyWithWarnings
                {
                    status = BatchFileStatus::Skipped;
                }
            }
        }
    }
    json!({
        "input": entry.input.display().to_string(),
        "status": status_name(status),
        "summary": entry.summary.as_ref().map(summary_json),
        "warnings": entry.warnings,
        "error": error,
        "outputs": entry.outputs.iter().map(|output| json!({
            "path": output.path.display().to_string(),
            "report": output.report.as_ref().map(report_json),
        })).collect::<Vec<_>>(),
        "completed_outputs": completed
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
    })
}
