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
    RootTransformPreview {
        euler_degrees: orientation_euler_degrees,
        scale: root_scale,
        translation: root_translation,
    }
    .to_matrix()
    .map_err(|error| error.to_string())?;

    if let Some((animation, start, end)) = trim {
        document
            .apply(EditOperation::TrimAnimation {
                animation,
                start,
                end,
            })
            .map_err(|error| error.to_string())?;
    }
    if orientation_euler_degrees
        .iter()
        .any(|value| value.abs() > f32::EPSILON)
    {
        document
            .apply(EditOperation::RotateRoots {
                euler_degrees: orientation_euler_degrees,
            })
            .map_err(|error| error.to_string())?;
    }
    if (root_scale - 1.0).abs() > f32::EPSILON {
        document
            .apply(EditOperation::ScaleRoots { factor: root_scale })
            .map_err(|error| error.to_string())?;
    }
    if root_translation
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
    pub(crate) fn build_glb_export_snapshot(
        &self,
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
        apply_glb_export_preview(
            &mut snapshot,
            self.orientation_euler_degrees,
            self.root_scale,
            self.root_translation,
            trim,
            animation_rate,
            smart_loop,
        )?;
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
        std::env::temp_dir().join(format!(
            "aio-asset-normalizer-export-preview-{}.glb",
            std::process::id()
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
}
