use crate::app::App;
use crate::modules::glb::{
    EditOperation, GlbDocument, RootTransformPreview, SmartLoopOptions,
};

fn apply_glb_export_preview(
    document: &mut GlbDocument,
    orientation_euler_degrees: [f32; 3],
    root_scale: f32,
    root_translation: [f32; 3],
    trim: Option<(usize, f32, f32)>,
    animation_rate: Option<(usize, f32)>,
    smart_loop: Option<(usize, f32)>,
) -> Result<(), String> {
    apply_glb_export_preview_with_root_transform(
        document,
        orientation_euler_degrees,
        root_scale,
        root_translation,
        trim,
        animation_rate,
        smart_loop,
        true,
    )
}

fn apply_glb_export_preview_with_root_transform(
    document: &mut GlbDocument,
    orientation_euler_degrees: [f32; 3],
    root_scale: f32,
    root_translation: [f32; 3],
    trim: Option<(usize, f32, f32)>,
    animation_rate: Option<(usize, f32)>,
    smart_loop: Option<(usize, f32)>,
    include_root_transform: bool,
) -> Result<(), String> {
    if include_root_transform {
        RootTransformPreview {
            euler_degrees: orientation_euler_degrees,
            scale: root_scale,
            translation: root_translation,
        }
        .to_matrix()
        .map_err(|error| error.to_string())?;
    }

    if let Some((animation, start, end)) = trim {
        document
            .apply(EditOperation::TrimAnimation {
                animation,
                start,
                end,
            })
            .map_err(|error| error.to_string())?;
    }

    if include_root_transform
        && orientation_euler_degrees
            .iter()
            .any(|value| value.abs() > f32::EPSILON)
    {
        document
            .apply(EditOperation::RotateRoots {
                euler_degrees: orientation_euler_degrees,
            })
            .map_err(|error| error.to_string())?;
    }
    if include_root_transform && (root_scale - 1.0).abs() > f32::EPSILON {
        document
            .apply(EditOperation::ScaleRoots { factor: root_scale })
            .map_err(|error| error.to_string())?;
    }
    if include_root_transform
        && root_translation
            .iter()
            .any(|value| value.abs() > f32::EPSILON)
    {
        document
            .apply(EditOperation::TranslateRoots {
                offset: root_translation,
            })
            .map_err(|error| error.to_string())?;
    }

    if let Some((animation, rate)) = animation_rate {
        if !rate.is_finite() || rate <= 0.0 {
            return Err("Animation rate must be finite and greater than zero"
                .to_owned());
        }
        if (rate - 1.0).abs() > f32::EPSILON {
            document
                .apply(EditOperation::ScaleAnimationRate { animation, rate })
                .map_err(|error| error.to_string())?;
        }
    }

    if let Some((animation, transition_seconds)) = smart_loop {
        document
            .smart_loop_animation(
                animation,
                SmartLoopOptions { transition_seconds },
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

impl App {
    pub(crate) fn root_transform_active(&self) -> bool {
        self.orientation_euler_degrees
            .iter()
            .any(|value| value.abs() > f32::EPSILON)
            || (self.root_scale - 1.0).abs() > f32::EPSILON
            || self
                .root_translation
                .iter()
                .any(|value| value.abs() > f32::EPSILON)
    }

    pub(crate) fn build_glb_export_snapshot(
        &self,
    ) -> Result<GlbDocument, String> {
        self.build_glb_export_snapshot_with_root_transform(
            self.bake_root_transform,
        )
    }

    pub(crate) fn build_glb_retarget_source_snapshot(
        &self,
    ) -> Result<GlbDocument, String> {
        self.build_glb_export_snapshot_with_root_transform(false)
    }

    fn build_glb_export_snapshot_with_root_transform(
        &self,
        include_root_transform: bool,
    ) -> Result<GlbDocument, String> {
        let Some(document) = self.glb.as_ref() else {
            return Err("Nothing to export".to_owned());
        };

        let rate = self.glb_animation_rate;
        if !rate.is_finite() || rate <= 0.0 {
            return Err("Animation rate must be finite and greater than zero"
                .to_owned());
        }
        let animation_rate = if (rate - 1.0).abs() > f32::EPSILON {
            let animation = self.glb_animation_index;
            let clip = self
                .canvas
                .animation_clips()
                .get(animation)
                .ok_or_else(|| {
                    format!(
                        "Cannot export animation rate: animation {animation} is unavailable"
                    )
                })?;
            if !clip.is_playable() {
                return Err(format!(
                    "Cannot export animation rate: animation {animation} is unavailable"
                ));
            }
            Some((animation, rate))
        } else {
            None
        };
        let trim = if self.trim_enabled {
            Some((self.trim_animation, self.trim_start, self.trim_end))
        } else {
            None
        };
        let smart_loop = if self.smart_loop_enabled {
            Some((self.glb_animation_index, self.smart_loop_transition))
        } else {
            None
        };

        let mut snapshot = document.clone();
        if include_root_transform {
            apply_glb_export_preview(
                &mut snapshot,
                self.orientation_euler_degrees,
                self.root_scale,
                self.root_translation,
                trim,
                animation_rate,
                smart_loop,
            )?;
        } else {
            apply_glb_export_preview_with_root_transform(
                &mut snapshot,
                self.orientation_euler_degrees,
                self.root_scale,
                self.root_translation,
                trim,
                animation_rate,
                smart_loop,
                false,
            )?;
        }
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::path::PathBuf;

    use serde_json::{json, Value};

    use super::*;

    fn export_fixture() -> GlbDocument {
        let mut bin = Vec::new();
        for value in [0.0_f32, 1.0, 2.0] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
        for value in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]] {
            for component in value {
                bin.extend_from_slice(&component.to_le_bytes());
            }
        }

        let json = json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Root", "translation": [1.0, 0.0, 0.0]}],
            "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 12},
                {"buffer": 0, "byteOffset": 12, "byteLength": 36}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "SCALAR", "min": [0.0], "max": [2.0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"}
            ],
            "animations": [{
                "name": "Clip",
                "samplers": [{"input": 0, "output": 1, "interpolation": "LINEAR"}],
                "channels": [{"sampler": 0, "target": {"node": 0, "path": "translation"}}]
            }]
        });
        let mut json_bytes = serde_json::to_vec(&json).unwrap();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }
        let bytes = gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(json_bytes),
            bin: Some(Cow::Owned(bin)),
        }
        .to_vec()
        .unwrap();
        let path = fixture_path();
        std::fs::write(&path, bytes).unwrap();
        let document = GlbDocument::load(&path).unwrap();
        let _ = std::fs::remove_file(path);
        document
    }

    fn fixture_path() -> PathBuf {
        // Tests run concurrently on separate threads; include the thread id
        // so parallel fixtures never share a file.
        std::env::temp_dir().join(format!(
            "aio-asset-normalizer-export-preview-{}-{:?}.glb",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[test]
    fn export_preview_applies_pending_settings_to_a_clone() {
        let document = export_fixture();
        let original = document.to_bytes().unwrap();
        let mut snapshot = document.clone();

        apply_glb_export_preview(
            &mut snapshot,
            [0.0, 0.0, 90.0],
            2.0,
            [3.0, 4.0, 5.0],
            None,
            Some((0, 2.0)),
            None,
        )
        .unwrap();

        assert_eq!(document.to_bytes().unwrap(), original);
        let bytes = snapshot.to_bytes().unwrap();
        gltf::Gltf::from_slice(&bytes).unwrap();
        let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
        let json: Value = serde_json::from_slice(&glb.json).unwrap();
        let matrix = json["nodes"][0]["matrix"].as_array().unwrap();
        assert!((matrix[12].as_f64().unwrap() - 3.0).abs() < 1e-5);
        assert!((matrix[13].as_f64().unwrap() - 6.0).abs() < 1e-5);
        assert!((matrix[14].as_f64().unwrap() - 5.0).abs() < 1e-5);

        let input = json["animations"][0]["samplers"][0]["input"]
            .as_u64()
            .unwrap() as usize;
        let view =
            json["accessors"][input]["bufferView"].as_u64().unwrap() as usize;
        let offset = json["bufferViews"][view]
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let bin = glb.bin.as_ref().unwrap();
        let times = (0..3)
            .map(|index| {
                let start = offset + index * 4;
                f32::from_le_bytes(bin[start..start + 4].try_into().unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(times, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn invalid_export_preview_keeps_the_document_unchanged() {
        let document = export_fixture();
        let mut snapshot = document.clone();
        let original = snapshot.to_bytes().unwrap();

        assert!(apply_glb_export_preview(
            &mut snapshot,
            [0.0, 0.0, 0.0],
            f32::NAN,
            [0.0, 0.0, 0.0],
            None,
            None,
            None,
        )
        .is_err());
        assert_eq!(snapshot.to_bytes().unwrap(), original);
    }

    #[test]
    fn skipping_root_transform_keeps_other_export_edits() {
        let document = export_fixture();
        let mut snapshot = document.clone();

        apply_glb_export_preview_with_root_transform(
            &mut snapshot,
            [0.0, 0.0, 90.0],
            2.0,
            [3.0, 4.0, 5.0],
            Some((0, 0.25, 0.75)),
            None,
            None,
            false,
        )
        .unwrap();

        let bytes = snapshot.to_bytes().unwrap();
        gltf::Gltf::from_slice(&bytes).unwrap();
        let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
        let json: Value = serde_json::from_slice(&glb.json).unwrap();
        assert!(json["nodes"][0].get("matrix").is_none());
        assert_eq!(json["nodes"][0]["translation"][0].as_f64().unwrap(), 1.0);

        let input = json["animations"][0]["samplers"][0]["input"]
            .as_u64()
            .unwrap() as usize;
        let accessor = &json["accessors"][input];
        assert_eq!(accessor["count"].as_u64().unwrap(), 2);
        let view = accessor["bufferView"].as_u64().unwrap() as usize;
        let offset = json["bufferViews"][view]
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let bin = glb.bin.as_ref().unwrap();
        let times = (0..2)
            .map(|index| {
                let start = offset + index * 4;
                f32::from_le_bytes(bin[start..start + 4].try_into().unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(times, vec![0.0, 0.5]);
    }
}

use std::fs;
use std::path::Path;

use crate::modules::bvh;

impl App {
    pub(crate) fn export_glb(&mut self) {
        if self.glb.is_none() {
            self.log.append("[glb_editor] Nothing to export");
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB", &["glb"])
            .set_file_name(
                self.glb_path
                    .as_ref()
                    .and_then(|path| path.file_stem())
                    .map(|stem| {
                        format!("{}_standardized.glb", stem.to_string_lossy())
                    })
                    .as_deref()
                    .unwrap_or("asset_standardized.glb"),
            )
            .save_file()
        else {
            return;
        };
        if is_source_path(&path, self.glb_path.as_deref()) {
            self.log
                .append("[glb_editor] Refusing to overwrite the source GLB");
            return;
        }
        let document = match self.build_glb_export_snapshot() {
            Ok(document) => document,
            Err(error) => {
                self.log
                    .append(&format!("[glb_editor] Export failed: {error}"));
                return;
            }
        };
        match document.export_atomic(&path) {
            Ok(()) => {
                self.file_tree.refresh();
                self.bvh_file_tree.refresh();
                let baked_note = if !self.bake_root_transform
                    && self.root_transform_active()
                {
                    " (root transform not baked)"
                } else {
                    ""
                };
                self.log.append(&format!(
                    "[glb_editor] Exported {}{baked_note}",
                    path.display()
                ));
            }
            Err(error) => self
                .log
                .append(&format!("[glb_editor] Export failed: {error}")),
        }
    }

    pub(crate) fn export_mapping(&mut self) {
        if self.retarget_mapping.is_some() {
            self.export_retarget_mapping();
            return;
        }
        let Some(mapping) = self.mapping.as_ref() else {
            self.log.append("[bvh_studio] Nothing to export");
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Mapping JSON", &["json"])
            .set_file_name("mapping.json")
            .save_file()
        else {
            return;
        };
        if is_source_path(&path, self.mapping_path.as_deref()) {
            self.log.append(
                "[bvh_studio] Refusing to overwrite the source mapping file",
            );
            return;
        }
        match bvh::save_mapping(&path, mapping) {
            Ok(()) => {
                self.file_tree.refresh();
                self.bvh_file_tree.refresh();
                self.log.append(&format!(
                    "[bvh_studio] Exported mapping {}",
                    path.display()
                ));
            }
            Err(error) => self.log.append(&format!(
                "[bvh_studio] Mapping export failed: {error}"
            )),
        }
    }

    pub(crate) fn export_bvh(&mut self) {
        let Some(document) = self.bvh.as_ref() else {
            self.log.append("[bvh_studio] Nothing to export");
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("BVH", &["bvh"])
            .set_file_name("animation_trimmed.bvh")
            .save_file()
        else {
            return;
        };
        if is_source_path(&path, self.bvh_path.as_deref()) {
            self.log
                .append("[bvh_studio] Refusing to overwrite the source BVH");
            return;
        }
        match document.write(&path) {
            Ok(()) => {
                self.file_tree.refresh();
                self.bvh_file_tree.refresh();
                self.log.append(&format!(
                    "[bvh_studio] Exported {}",
                    path.display()
                ));
            }
            Err(error) => self
                .log
                .append(&format!("[bvh_studio] Export failed: {error}")),
        }
    }
}

fn is_source_path(path: &Path, source: Option<&Path>) -> bool {
    source.is_some_and(|source| same_path(path, source))
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());

    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}
