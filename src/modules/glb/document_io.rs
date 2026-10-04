//! Validated document serialization and atomic submission.
use super::*;
impl GlbDocument {
    pub fn export_atomic(&self, path: &Path) -> Result<(), GlbError> {
        self.export_atomic_with_policy(path, true)
    }
    pub fn export_atomic_with_policy(
        &self,
        path: &Path,
        overwrite: bool,
    ) -> Result<(), GlbError> {
        let bytes = self.to_bytes()?;
        Self::from_bytes(&bytes, None)?;
        crate::modules::atomic_file::write(path, &bytes, overwrite)?;
        Ok(())
    }
    pub(crate) fn from_bytes(
        bytes: &[u8],
        source_path: Option<PathBuf>,
    ) -> Result<Self, GlbError> {
        let glb = gltf::binary::Glb::from_slice(bytes)?;
        let json =
            serde_json::from_slice::<Value>(&glb.json).map_err(|error| {
                GlbError::Invalid(format!("JSON chunk: {error}"))
            })?;
        let parsed = gltf::Gltf::from_slice(bytes)?;
        super::document_validation::validate(
            &parsed.document,
            glb.bin.as_deref(),
        )?;
        Ok(Self {
            source_path,
            json,
            bin: glb.bin.map(Cow::into_owned),
            dirty: false,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, GlbError> {
        let mut json_bytes =
            serde_json::to_vec(&self.json).map_err(|error| {
                GlbError::Invalid(format!("Serialize JSON: {error}"))
            })?;
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }
        let bin = self.bin.as_deref().map(|data| {
            let mut padded = data.to_vec();
            while padded.len() % 4 != 0 {
                padded.push(0);
            }
            padded
        });
        let glb = gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(json_bytes),
            bin: bin.map(Cow::Owned),
        };
        glb.to_vec().map_err(GlbError::from)
    }
}
