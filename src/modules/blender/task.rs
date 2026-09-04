use std::path::{Path, PathBuf};

/// Immutable inputs for one headless Blender conversion run.
pub struct ConversionTask {
    pub task_id: u64,
    pub input: PathBuf,
    pub output: PathBuf,
    pub config_json: String,
    pub blender_path: Option<String>,
}

/// Message the converter worker thread sends back to the UI layer.
pub enum ConverterMessage {
    /// A single file is about to be processed by the worker.
    FileStarted { task_id: u64, input: PathBuf },
    /// A single file finished; the result carries `Ok(output)` or an error.
    FileFinished {
        task_id: u64,
        input: PathBuf,
        result: Result<PathBuf, String>,
    },
    /// The whole batch finished and the worker thread is done.
    Finished,
}

/// Default conversion output for an input file:
/// `<input_dir>/<stem>_normalized.glb`, written next to the source.
pub fn normalized_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}_normalized.glb"))
}

/// Fixed normalization profile for the FBX Converter workflow. It matches
/// the historical V2 defaults and is intentionally not user-editable.
pub fn default_config_json() -> String {
    serde_json::json!({
        "target_scale": 1.0,
        "up_axis": "Y",
        "remove_unused_materials": true,
        "remove_cameras": true,
        "remove_lights": true,
        "remove_loose_vertices": false,
        "correct_bone_axes": true,
        "preserve_leaf_bones": true,
        "bake_animations": true
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_path_is_sibling_normalized_glb() {
        assert_eq!(
            normalized_output_path(Path::new("/assets/rig.fbx")),
            PathBuf::from("/assets/rig_normalized.glb")
        );
        assert_eq!(
            normalized_output_path(Path::new("model.OBJ")),
            PathBuf::from("model_normalized.glb")
        );
    }

    #[test]
    fn default_config_matches_v2_profile() {
        let value: serde_json::Value =
            serde_json::from_str(&default_config_json()).unwrap();
        let expected = serde_json::json!({
            "target_scale": 1.0,
            "up_axis": "Y",
            "remove_unused_materials": true,
            "remove_cameras": true,
            "remove_lights": true,
            "remove_loose_vertices": false,
            "correct_bone_axes": true,
            "preserve_leaf_bones": true,
            "bake_animations": true
        });
        assert_eq!(value, expected);
    }
}
