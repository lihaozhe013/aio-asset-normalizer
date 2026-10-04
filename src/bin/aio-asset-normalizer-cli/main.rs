//! Headless CLI for the AIO Asset Normalizer.
//!
//! Every command writes one JSON envelope to stdout; diagnostics go to stderr
//! through the shared logging pipeline. Run `docs` for the full reference.

mod bvh;
mod discovery;
mod docs;
mod fbx;
mod glb;
mod job;
mod output;
mod retarget;
mod util;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

use aio_asset_normalizer::modules::logging::LogRuntime;

#[derive(Parser)]
#[command(
    name = "aio-asset-normalizer-cli",
    version = concat!(
        env!("CARGO_PKG_VERSION"),
        " (commit ",
        env!("AIO_ASSET_NORMALIZER_COMMIT"),
        ")"
    ),
    about = "Headless GLB, BVH, retargeting, and FBX conversion",
    long_about = "Headless asset normalization for agent-driven pipelines.\n\n\
Every command writes exactly one JSON envelope to stdout, described by the \
`schema_version` field, and writes progress and diagnostics to stderr. Exit \
codes: 0 success, 2 usage, 3 validation, 4 I/O, 5 external tool. Run \
`aio-asset-normalizer-cli docs` for the complete command and JSON reference."
)]
struct Cli {
    /// Tracing filter for stderr and file logs (for example info, debug, glb_export=debug)
    #[arg(long, global = true, value_name = "FILTER", default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect, export, and edit GLB assets
    Glb(glb::GlbArgs),
    /// Inspect and process BVH motion
    Bvh(bvh::BvhArgs),
    /// Build prompts, validate mappings, and retarget animations
    Retarget(retarget::RetargetArgs),
    /// Convert FBX files to GLB through a headless Blender
    Fbx(fbx::FbxArgs),
    /// Print the embedded CLI reference
    Docs(docs::DocsArgs),
    /// List supported automation capabilities
    Capabilities,
    /// Print a JSON Schema for a shared request type
    Schema(discovery::SchemaArgs),
}

fn main() {
    let matches = match Cli::command().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return;
            }
            std::process::exit(output::emit_failure(
                "cli.parse",
                serde_json::json!({}),
                &output::CliError::usage(error.to_string()),
            ));
        }
    };
    if let Err(error) = check_job_conflicts(&matches, &Cli::command()) {
        std::process::exit(output::emit_failure(
            "cli.parse",
            serde_json::json!({}),
            &error,
        ));
    }
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => {
            std::process::exit(output::emit_failure(
                "cli.parse",
                serde_json::json!({}),
                &output::CliError::usage(error.to_string()),
            ));
        }
    };
    let logging = LogRuntime::init_cli(Some(&cli.log_level));

    let code = match cli.command {
        Command::Glb(args) => glb::run(args),
        Command::Bvh(args) => bvh::run(args),
        Command::Retarget(args) => retarget::run(args),
        Command::Fbx(args) => fbx::run(args),
        Command::Docs(args) => docs::run(&args),
        Command::Capabilities => discovery::capabilities(),
        Command::Schema(args) => discovery::schema(&args),
    };

    // Flush the file log before bypassing destructors with process::exit.
    drop(logging);
    std::process::exit(code);
}

fn check_job_conflicts(
    matches: &clap::ArgMatches,
    command: &clap::Command,
) -> Result<(), output::CliError> {
    if let Some((name, child)) = matches.subcommand() {
        if let Some(command) = command.find_subcommand(name) {
            return check_job_conflicts(child, command);
        }
    }
    if matches
        .try_get_one::<std::path::PathBuf>("job")
        .ok()
        .flatten()
        .is_some()
    {
        for arg in command.get_arguments() {
            let id = arg.get_id();
            if !["job", "dry_run", "log_level"].contains(&id.as_str())
                && matches.value_source(id.as_str())
                    == Some(clap::parser::ValueSource::CommandLine)
            {
                return Err(output::CliError::usage(format!(
                    "--job cannot be combined with task argument {}",
                    id.as_str()
                )));
            }
        }
    }
    Ok(())
}
