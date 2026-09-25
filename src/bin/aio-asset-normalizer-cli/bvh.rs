//! `bvh` subcommands.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use aio_asset_normalizer::modules::bvh::{BvhChannel, BvhDocument};

use crate::output::{self, CliError};
use crate::util::same_path;

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
    /// Source BVH
    #[arg(value_name = "BVH")]
    pub input: PathBuf,
    /// Destination BVH
    #[arg(long, value_name = "FILE")]
    pub output: PathBuf,
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
    let fail = |error: CliError| {
        output::emit_failure(COMMAND, json!({}), &error)
    };

    let mut document = match BvhDocument::load(&args.input) {
        Ok(document) => document,
        Err(error) => return fail(output::bvh_error(error)),
    };
    let original_frames = document.frames.len();

    if let Some(range) = &args.trim {
        let (start, end) = (range[0], range[1]);
        if let Err(error) = document.trim(start, end) {
            return fail(output::bvh_error(error));
        }
    }

    if same_path(&args.output, &args.input) {
        return fail(CliError::validation(
            "output must not replace the source BVH",
        ));
    }
    if args.output.exists() && !args.overwrite {
        return fail(CliError::validation(format!(
            "output already exists: {} (pass --overwrite)",
            args.output.display()
        )));
    }

    let result = json!({
        "input": args.input.display().to_string(),
        "output": args.output.display().to_string(),
        "original_frame_count": original_frames,
        "output_frame_count": document.frames.len(),
        "written": !args.dry_run,
    });

    if args.dry_run {
        return output::emit_success(COMMAND, result, Vec::new());
    }

    if let Err(error) = document.write(&args.output) {
        return fail(output::bvh_error(error));
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