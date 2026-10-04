//! Machine-readable capabilities and schemas derived from shared request types.
use crate::output::{self, CliError};
use aio_asset_normalizer::modules::operations;
use clap::Args;
use serde_json::json;

#[derive(Args)]
pub struct SchemaArgs {
    #[arg(long)]
    pub command: String,
}
pub fn capabilities() -> i32 {
    output::emit_success(
        "capabilities",
        json!({
            "job_schema_version":1,
            "commands":["glb.inspect","glb.export","glb.edit","bvh.inspect","bvh.process","retarget.prompt","retarget.suggest","retarget.validate","retarget.run","fbx.convert","docs","capabilities","schema"],
            "job_commands":["glb.export","glb.edit","bvh.process","retarget.prompt","retarget.suggest","retarget.validate","retarget.run"],
            "dry_run_commands":["glb.export","glb.edit","bvh.process","retarget.prompt","retarget.run","fbx.convert"],
            "features":["texture-replacement","explicit-export-selection","edited-retarget-source","atomic-no-clobber"],
            "converter":{"requires":"Blender","inputs":["fbx","obj","blend"],"dry_run_scope":"preflight-only"}
        }),
        vec![],
    )
}
pub fn schema(args: &SchemaArgs) -> i32 {
    let schema = match args.command.as_str() {
        "glb.export" => schemars::schema_for!(operations::spec::ExportJobFile),
        "glb.edit" => schemars::schema_for!(operations::spec::EditJobFile),
        "bvh.process" => {
            schemars::schema_for!(operations::bvh::BvhProcessRequest)
        }
        "retarget.prompt" | "retarget.suggest" | "retarget.validate"
        | "retarget.run" => {
            schemars::schema_for!(operations::retarget::RetargetJob)
        }
        "fbx.convert" => {
            schemars::schema_for!(operations::converter::ConversionRequest)
        }
        _ => {
            return output::emit_failure(
                "schema",
                json!({}),
                &CliError::usage("No request schema for this command"),
            )
        }
    };
    output::emit_success(
        "schema",
        json!({"command":args.command,"schema":schema}),
        vec![],
    )
}
