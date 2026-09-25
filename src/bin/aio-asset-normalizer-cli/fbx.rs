//! `fbx` subcommands: convert FBX files to GLB through headless Blender.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use aio_asset_normalizer::modules::blender::bridge;
use aio_asset_normalizer::modules::blender::bridge::run_task;
use aio_asset_normalizer::modules::blender::task::{
    default_config_json, normalized_output_path, ConversionTask,
};
use aio_asset_normalizer::modules::glb::GlbDocument;
use aio_asset_normalizer::modules::logging::next_task_id;

use crate::output::{self, CliError};
use crate::util::same_path;

#[derive(Args)]
pub struct FbxArgs {
    #[command(subcommand)]
    pub command: FbxCommand,
}

#[derive(Subcommand)]
pub enum FbxCommand {
    /// Convert files to GLB with a headless Blender
    Convert(ConvertArgs),
}

#[derive(Args)]
pub struct ConvertArgs {
    /// FBX or other Blender-importable files
    #[arg(value_name = "INPUT", required = true)]
    pub inputs: Vec<PathBuf>,
    /// Blender executable or macOS .app bundle; defaults to a discovered install
    #[arg(long, value_name = "FILE")]
    pub blender: Option<PathBuf>,
    /// Replace existing outputs
    #[arg(long)]
    pub overwrite: bool,
    /// Output file for a single input; defaults to a sibling *_normalized.glb
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,
}

pub fn run(args: FbxArgs) -> i32 {
    match args.command {
        FbxCommand::Convert(args) => convert(&args),
    }
}

fn convert(args: &ConvertArgs) -> i32 {
    const COMMAND: &str = "fbx.convert";
    let fail = |results: Value, error: CliError| {
        output::emit_failure(COMMAND, results, &error)
    };

    if args.out.is_some() && args.inputs.len() != 1 {
        return fail(
            json!({}),
            CliError::validation("--out requires exactly one input file"),
        );
    }

    let preferred = args.blender.as_ref().map(|path| path.to_string_lossy());
    if bridge::find_blender(preferred.as_deref()).is_none() {
        return fail(
            json!({}),
            CliError::external(
                "Blender executable not found; install Blender or pass --blender",
            ),
        );
    }

    let mut files = Vec::new();
    let mut first_error: Option<CliError> = None;
    for input in &args.inputs {
        let output = args
            .out
            .clone()
            .unwrap_or_else(|| normalized_output_path(input));

        if same_path(&output, input) {
            let error = CliError::validation(format!(
                "output must not replace the input: {}",
                input.display()
            ));
            files.push(failure_json(input, &output, &error));
            first_error.get_or_insert(error);
            continue;
        }
        if output.exists() && !args.overwrite {
            let error = CliError::validation(format!(
                "output already exists: {} (pass --overwrite)",
                output.display()
            ));
            files.push(failure_json(input, &output, &error));
            first_error.get_or_insert(error);
            continue;
        }

        let task = ConversionTask {
            task_id: next_task_id(),
            input: input.clone(),
            output: output.clone(),
            config_json: default_config_json(),
            blender_path: args
                .blender
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
        };
        match run_task(&task) {
            Ok(()) => match GlbDocument::load(&output) {
                Ok(document) => files.push(json!({
                    "input": input.display().to_string(),
                    "output": output.display().to_string(),
                    "ok": true,
                    "summary": {
                        "nodes": document.summary().nodes,
                        "meshes": document.summary().meshes,
                        "materials": document.summary().materials,
                        "skins": document.summary().skins,
                        "animations": document.summary().animations,
                    },
                })),
                Err(error) => {
                    let error = output::glb_error(error);
                    files.push(failure_json(input, &output, &error));
                    first_error.get_or_insert(error);
                }
            },
            Err(error) => {
                let error = output::blender_error(error);
                files.push(failure_json(input, &output, &error));
                first_error.get_or_insert(error);
            }
        }
    }

    let results = json!({ "files": files });
    match first_error {
        None => output::emit_success(COMMAND, results, Vec::new()),
        Some(error) => fail(results, error),
    }
}

fn failure_json(input: &Path, output: &Path, error: &CliError) -> Value {
    json!({
        "input": input.display().to_string(),
        "output": output.display().to_string(),
        "ok": false,
        "error": {
            "code": error.code.as_str(),
            "message": error.message,
        },
    })
}
