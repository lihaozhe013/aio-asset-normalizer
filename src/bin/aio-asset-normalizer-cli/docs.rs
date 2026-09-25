//! `docs` subcommand: print the embedded CLI reference.

use clap::Args;
use serde_json::json;

use crate::output;

const CLI_DOCUMENTATION: &str = include_str!("../../../docs/CLI.md");

#[derive(Args)]
pub struct DocsArgs {
    /// Print the raw Markdown instead of a JSON envelope
    #[arg(long)]
    pub raw: bool,
}

pub fn run(args: &DocsArgs) -> i32 {
    if args.raw {
        output::emit_stdout(CLI_DOCUMENTATION);
        return 0;
    }
    output::emit_success(
        "docs",
        json!({
            "format": "markdown",
            "markdown": CLI_DOCUMENTATION,
        }),
        Vec::new(),
    )
}
