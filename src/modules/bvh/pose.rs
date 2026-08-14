use super::{BvhDocument, BvhError};

impl BvhDocument {
    pub fn joint_positions(
        &self,
        frame_index: usize,
    ) -> Result<Vec<[f32; 3]>, BvhError> {
        let frame = self.frames.get(frame_index).ok_or_else(|| {
            BvhError::Parse(format!("BVH frame {frame_index} does not exist"))
        })?;
        Ok(self
            .frame_transforms(frame)?
            .into_iter()
            .map(|transform| transform.position)
            .collect())
    }
}
