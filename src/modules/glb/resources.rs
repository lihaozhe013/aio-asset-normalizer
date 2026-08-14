use std::fs;
use std::path::Path;

use image::ImageFormat;
use serde_json::{json, Value};

use super::{GlbDocument, GlbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimitiveTarget {
    pub mesh: usize,
    pub primitive: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureSlot {
    BaseColor,
    Normal,
    MetallicRoughness,
    Occlusion,
    Emissive,
}

impl TextureSlot {
    pub const ALL: [Self; 5] = [
        Self::BaseColor,
        Self::Normal,
        Self::MetallicRoughness,
        Self::Occlusion,
        Self::Emissive,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::BaseColor => "Base Color",
            Self::Normal => "Normal",
            Self::MetallicRoughness => "Metallic-Roughness",
            Self::Occlusion => "Occlusion",
            Self::Emissive => "Emissive",
        }
    }
}

impl GlbDocument {
    pub fn replace_texture(
        &mut self,
        target: PrimitiveTarget,
        slot: TextureSlot,
        image_path: &Path,
        duplicate_shared_material: bool,
    ) -> Result<(), GlbError> {
        let image_bytes = fs::read(image_path)?;
        let mime_type = image_mime_type(&image_bytes)?;
        let material_index = self.primitive_material(target)?;
        let material_index = if duplicate_shared_material
            && self.material_usage_count(material_index)? > 1
        {
            self.duplicate_material(target, material_index)?
        } else {
            material_index
        };
        let image_index = self.append_image(image_bytes, mime_type)?;
        let texture_index = self.append_texture(image_index)?;
        self.set_material_texture(material_index, slot, texture_index)?;
        self.dirty = true;
        Ok(())
    }

    fn primitive_material(
        &self,
        target: PrimitiveTarget,
    ) -> Result<usize, GlbError> {
        let meshes = self
            .json
            .get("meshes")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no meshes array".to_owned())
            })?;
        let mesh = meshes.get(target.mesh).ok_or_else(|| {
            GlbError::Invalid(format!("Mesh {} does not exist", target.mesh))
        })?;
        let primitives = mesh
            .get("primitives")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("Mesh has no primitives".to_owned())
            })?;
        let primitive = primitives.get(target.primitive).ok_or_else(|| {
            GlbError::Invalid(format!(
                "Mesh {} primitive {} does not exist",
                target.mesh, target.primitive
            ))
        })?;
        primitive
            .get("material")
            .and_then(Value::as_u64)
            .map(|index| index as usize)
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Mesh {} primitive {} has no material",
                    target.mesh, target.primitive
                ))
            })
    }

    fn material_usage_count(
        &self,
        material_index: usize,
    ) -> Result<usize, GlbError> {
        let meshes = self
            .json
            .get("meshes")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no meshes array".to_owned())
            })?;
        let mut count = 0;
        for mesh in meshes {
            let primitives = mesh
                .get("primitives")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                GlbError::Invalid("Mesh has no primitives".to_owned())
            })?;
            count += primitives
                .iter()
                .filter(|primitive| {
                    primitive.get("material").and_then(Value::as_u64)
                        == Some(material_index as u64)
                })
                .count();
        }
        Ok(count)
    }

    fn duplicate_material(
        &mut self,
        target: PrimitiveTarget,
        material_index: usize,
    ) -> Result<usize, GlbError> {
        let material = self
            .json
            .get("materials")
            .and_then(Value::as_array)
            .and_then(|materials| materials.get(material_index))
            .cloned()
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Material {material_index} does not exist"
                ))
            })?;
        let materials = self
            .json
            .get_mut("materials")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no materials array".to_owned())
            })?;
        let duplicate_index = materials.len();
        materials.push(material);

        let meshes = self
            .json
            .get_mut("meshes")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no meshes array".to_owned())
            })?;
        let mesh = meshes.get_mut(target.mesh).ok_or_else(|| {
            GlbError::Invalid(format!("Mesh {} does not exist", target.mesh))
        })?;
        let primitives = mesh
            .get_mut("primitives")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("Mesh has no primitives".to_owned())
            })?;
        let primitive =
            primitives.get_mut(target.primitive).ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Mesh {} primitive {} does not exist",
                    target.mesh, target.primitive
                ))
            })?;
        primitive["material"] = json!(duplicate_index);
        Ok(duplicate_index)
    }

    fn append_image(
        &mut self,
        bytes: Vec<u8>,
        mime_type: &'static str,
    ) -> Result<usize, GlbError> {
        let buffer_view = self.append_binary_resource(&bytes)?;
        let images = ensure_array(&mut self.json, "images")?;
        let image_index = images.len();
        images.push(json!({
            "bufferView": buffer_view,
            "mimeType": mime_type,
        }));
        Ok(image_index)
    }

    fn append_texture(
        &mut self,
        image_index: usize,
    ) -> Result<usize, GlbError> {
        let textures = ensure_array(&mut self.json, "textures")?;
        let texture_index = textures.len();
        textures.push(json!({"source": image_index}));
        Ok(texture_index)
    }

    fn set_material_texture(
        &mut self,
        material_index: usize,
        slot: TextureSlot,
        texture_index: usize,
    ) -> Result<(), GlbError> {
        let materials = self
            .json
            .get_mut("materials")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no materials array".to_owned())
            })?;
        let material = materials.get_mut(material_index).ok_or_else(|| {
            GlbError::Invalid(format!(
                "Material {material_index} does not exist"
            ))
        })?;
        let object = material.as_object_mut().ok_or_else(|| {
            GlbError::Invalid("Material is not an object".to_owned())
        })?;
        let (parent_key, texture_key) = match slot {
            TextureSlot::BaseColor => {
                ("pbrMetallicRoughness", "baseColorTexture")
            }
            TextureSlot::Normal => ("", "normalTexture"),
            TextureSlot::MetallicRoughness => {
                ("pbrMetallicRoughness", "metallicRoughnessTexture")
            }
            TextureSlot::Occlusion => ("", "occlusionTexture"),
            TextureSlot::Emissive => ("", "emissiveTexture"),
        };
        if parent_key.is_empty() {
            object.insert(
                texture_key.to_owned(),
                json!({"index": texture_index}),
            );
        } else {
            let parent = object
                .entry(parent_key.to_owned())
                .or_insert_with(|| json!({}));
            let parent = parent.as_object_mut().ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Material field {parent_key} is not an object"
                ))
            })?;
            parent.insert(
                texture_key.to_owned(),
                json!({"index": texture_index}),
            );
        }
        Ok(())
    }

    fn append_binary_resource(
        &mut self,
        bytes: &[u8],
    ) -> Result<usize, GlbError> {
        let bin = self.bin.get_or_insert_with(Vec::new);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let offset = bin.len();
        bin.extend_from_slice(bytes);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let views = ensure_array(&mut self.json, "bufferViews")?;
        let view_index = views.len();
        views.push(json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": bytes.len(),
        }));
        let buffers = ensure_array(&mut self.json, "buffers")?;
        if buffers.is_empty() {
            buffers.push(json!({"byteLength": bin.len()}));
        } else {
            buffers[0]["byteLength"] = json!(bin.len());
        }
        Ok(view_index)
    }
}

fn ensure_array<'a>(
    json: &'a mut Value,
    key: &str,
) -> Result<&'a mut Vec<Value>, GlbError> {
    let object = json.as_object_mut().ok_or_else(|| {
        GlbError::Invalid("GLB JSON root is not an object".to_owned())
    })?;
    let value = object.entry(key.to_owned()).or_insert_with(|| json!([]));
    value.as_array_mut().ok_or_else(|| {
        GlbError::Invalid(format!("GLB field {key} is not an array"))
    })
}

fn image_mime_type(bytes: &[u8]) -> Result<&'static str, GlbError> {
    let format = image::guess_format(bytes).map_err(|error| {
        GlbError::Invalid(format!(
            "Texture image format could not be detected: {error}"
        ))
    })?;
    match format {
        ImageFormat::Png => Ok("image/png"),
        ImageFormat::Jpeg => Ok("image/jpeg"),
        other => Err(GlbError::Unsupported(format!(
            "Texture format {other:?} is not supported; use PNG or JPEG"
        ))),
    }
}
