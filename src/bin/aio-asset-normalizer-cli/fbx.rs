//! `fbx` subcommands: convert FBX files to GLB through headless Blender.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use aio_asset_normalizer::modules::blender::task::normalized_output_path;
use aio_asset_normalizer::modules::logging::next_task_id;

use crate::output::{self, CliError};
use aio_asset_normalizer::modules::operations::converter::{
    validate_batch_outputs, ConversionRequest,
};

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
    /// Check inputs, dependencies and destinations without launching Blender
    #[arg(long)]
    pub dry_run: bool,
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

    let requests: Vec<_> = args
        .inputs
        .iter()
        .map(|input| ConversionRequest {
            input: input.clone(),
            output: args
                .out
                .clone()
                .unwrap_or_else(|| normalized_output_path(input)),
            blender: args.blender.clone(),
            overwrite: args.overwrite,
        })
        .collect();
    if let Err(error) = validate_batch_outputs(&requests) {
        return fail(json!({"files":[]}), output::operation_error(error));
    }
    let mut files = Vec::new();
    let mut first_error: Option<CliError> = None;
    for request in requests {
        let input = &request.input;
        let output = request.output.clone();
        let result = if args.dry_run {
            aio_asset_normalizer::modules::operations::converter::preflight(
                &request,
            )
            .map(|()| None)
        } else {
            aio_asset_normalizer::modules::operations::converter::execute(
                &request,
                next_task_id(),
            )
            .map(Some)
        };
        match result {
            Ok(summary) => files.push(json!({"input":input,"output":output,"ok":true,"summary":summary.map(|v|json!({"nodes":v.nodes,"meshes":v.meshes,"materials":v.materials,"skins":v.skins,"animations":v.animations})),"written":!args.dry_run})),
            Err(e) => { let error = output::operation_error(e); files.push(failure_json(input,&output,&error)); first_error.get_or_insert(error); }
        }
    }

    let results = json!({ "files": files, "dry_run":args.dry_run, "validation_scope": if args.dry_run { "preflight-only" } else { "converted-glb" } });
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
