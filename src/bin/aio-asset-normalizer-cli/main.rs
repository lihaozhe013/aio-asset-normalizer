//! Headless CLI for the AIO Asset Normalizer.
//!
//! Every command writes one JSON envelope to stdout; diagnostics go to stderr
//! through the shared logging pipeline. Run `docs` for the full reference.

mod bvh;
mod docs;
mod fbx;
mod glb;
mod job;
mod output;
mod retarget;
mod util;

use clap::{Parser, Subcommand};

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
}

fn main() {
    let cli = Cli::parse();
    let logging = LogRuntime::init_cli(Some(&cli.log_level));

    let code = match cli.command {
        Command::Glb(args) => glb::run(args),
        Command::Bvh(args) => bvh::run(args),
        Command::Retarget(args) => retarget::run(args),
        Command::Fbx(args) => fbx::run(args),
        Command::Docs(args) => docs::run(&args),
    };

    // Flush the file log before bypassing destructors with process::exit.
    drop(logging);
    std::process::exit(code);
}
