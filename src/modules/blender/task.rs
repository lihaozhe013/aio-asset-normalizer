use std::path::PathBuf;

#[allow(dead_code)]
pub enum TaskMessage {
    Log(String),
    Progress {
        current: usize,
        total: usize,
        file: PathBuf,
    },
    Finished {
        file: PathBuf,
        output: PathBuf,
        success: bool,
    },
}

pub struct ConversionTask {
    pub input: PathBuf,
    pub output: PathBuf,
    pub config_json: String,
    pub script_version: super::bridge::ScriptVersion,
}
