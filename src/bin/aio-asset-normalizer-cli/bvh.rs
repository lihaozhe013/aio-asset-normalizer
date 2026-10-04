//! `bvh` subcommands.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use aio_asset_normalizer::modules::bvh::{BvhChannel, BvhDocument};

use crate::output::{self, CliError};
use aio_asset_normalizer::modules::operations::{self, bvh as core};

#[derive(Args)]
pub struct BvhArgs {
    #[command(subcommand)]
    pub command: BvhCommand,
}

#[derive(Subcommand)]
pub enum BvhCommand {
    /// Print hierarchy, channel, and timing metadata
    Inspect(InspectArgs),
    /// Trim a BVH and write it out
    Process(ProcessArgs),
}

#[derive(Args)]
pub struct InspectArgs {
    /// BVH files to inspect
    #[arg(value_name = "BVH", required = true)]
    pub inputs: Vec<PathBuf>,
}

#[derive(Args)]
pub struct ProcessArgs {
    /// Shared BVH processing request
    #[arg(long)]
    pub job: Option<PathBuf>,
    /// Source BVH
    #[arg(value_name = "BVH", required_unless_present = "job")]
    pub input: Option<PathBuf>,
    /// Destination BVH
    #[arg(long, value_name = "FILE", required_unless_present = "job")]
    pub output: Option<PathBuf>,
    /// Keep only the seconds between START and END
    #[arg(long, num_args = 2, value_names = ["START", "END"])]
    pub trim: Option<Vec<f32>>,
    /// Replace an existing output
    #[arg(long)]
    pub overwrite: bool,
    /// Report the result without writing the output
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(args: BvhArgs) -> i32 {
    match args.command {
        BvhCommand::Inspect(args) => inspect(&args),
        BvhCommand::Process(args) => process(&args),
    }
}

fn inspect(args: &InspectArgs) -> i32 {
    const COMMAND: &str = "bvh.inspect";
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
    let document = BvhDocument::load(path).map_err(output::bvh_error)?;
    Ok(json!({
        "path": path.display().to_string(),
        "ok": true,
        "frame_time_seconds": document.frame_time,
        "frame_count": document.frames.len(),
        "duration_seconds": document.duration(),
        "joint_count": document.joints.len(),
        "channel_count": document
            .joints
            .iter()
            .map(|joint| joint.channels.len())
            .sum::<usize>(),
        "joints": document
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| json!({
                "index": index,
                "name": joint.name,
                "parent": joint.parent,
                "children": joint.children,
                "offset": joint.offset,
                "channels": joint
                    .channels
                    .iter()
                    .map(|channel| channel_name(*channel))
                    .collect::<Vec<_>>(),
                "end_site": joint.end_site,
            }))
            .collect::<Vec<_>>(),
    }))
}

fn process(args: &ProcessArgs) -> i32 {
    const COMMAND: &str = "bvh.process";
    let fail =
        |error: CliError| output::emit_failure(COMMAND, json!({}), &error);

    let request = match &args.job {
        Some(path) => match operations::load_json::<core::BvhProcessRequest>(
            path, "BVH job",
        ) {
            Ok(request) => request,
            Err(error) => return fail(output::operation_error(error)),
        },
        None => core::BvhProcessRequest {
            command: None,
            input: match &args.input {
                Some(path) => path.clone(),
                None => return fail(CliError::usage("Source BVH is required")),
            },
            output: match &args.output {
                Some(path) => path.clone(),
                None => return fail(CliError::usage("--output is required")),
            },
            trim: args.trim.as_ref().map(|v| [v[0], v[1]]),
            overwrite: args.overwrite,
        },
    };
    if request.command.as_deref().is_some_and(|v| v != COMMAND) {
        return fail(CliError::validation(
            "BVH job declares a different command",
        ));
    }
    let mut document = match BvhDocument::load(&request.input) {
        Ok(document) => document,
        Err(error) => return fail(output::bvh_error(error)),
    };
    let original_frames = document.frames.len();

    document = match aio_asset_normalizer::modules::operations::bvh::prepare(
        &document,
        request.trim,
    ) {
        Ok(document) => document,
        Err(e) => return fail(output::operation_error(e)),
    };

    if let Err(error) = core::validate_output(
        &document,
        &request.output,
        &request.input,
        request.overwrite,
    ) {
        return fail(output::operation_error(error));
    }
    let result = json!({
        "input": request.input.display().to_string(),
        "output": request.output.display().to_string(),
        "original_frame_count": original_frames,
        "output_frame_count": document.frames.len(),
        "written": !args.dry_run,
    });

    if args.dry_run {
        return output::emit_success(COMMAND, result, Vec::new());
    }

    if let Err(error) = aio_asset_normalizer::modules::operations::bvh::write(
        &document,
        &request.output,
        &request.input,
        request.overwrite,
    ) {
        return fail(output::operation_error(error));
    }
    output::emit_success(COMMAND, result, Vec::new())
}

fn channel_name(channel: BvhChannel) -> &'static str {
    match channel {
        BvhChannel::Xposition => "Xposition",
        BvhChannel::Yposition => "Yposition",
        BvhChannel::Zposition => "Zposition",
        BvhChannel::Xrotation => "Xrotation",
        BvhChannel::Yrotation => "Yrotation",
        BvhChannel::Zrotation => "Zrotation",
    }
}
