use crate::app::App;
use crate::modules::glb::RootTransformPreview;
use crate::reload::GlbReloadKind;
use three_d::*;

impl App {
    pub(crate) fn mark_root_preview_dirty(&mut self) {
        self.root_preview_dirty = true;
        self.root_preview_error = None;
    }

    pub(crate) fn root_preview_error(&self) -> Option<&str> {
        self.root_preview_error.as_deref()
    }

    pub(crate) fn reset_root_preview(&mut self) {
        self.orientation_euler_degrees = [0.0, 0.0, 0.0];
        self.root_scale = 1.0;
        self.root_translation = [0.0, 0.0, 0.0];
        self.root_preview_error = None;
        self.root_preview_dirty = true;
    }

    pub(crate) fn reset_root_orientation(&mut self) {
        self.orientation_euler_degrees = [0.0, 0.0, 0.0];
        self.mark_root_preview_dirty();
    }

    pub(crate) fn reset_root_scale(&mut self) {
        self.root_scale = 1.0;
        self.mark_root_preview_dirty();
    }

    pub(crate) fn reset_root_translation(&mut self) {
        self.root_translation = [0.0, 0.0, 0.0];
        self.mark_root_preview_dirty();
    }

    fn root_transform_preview(&self) -> RootTransformPreview {
        RootTransformPreview {
            euler_degrees: self.orientation_euler_degrees,
            scale: self.root_scale,
            translation: self.root_translation,
        }
    }

    pub(crate) fn update_root_preview_if_needed(&mut self) {
        if !self.root_preview_dirty {
            return;
        }
        self.root_preview_dirty = false;
        let matrix = match self.root_transform_preview().to_matrix() {
            Ok(matrix) => matrix,
            Err(error) => {
                self.log.append(&format!(
                    "[glb_editor] Root transform preview failed: {error}"
                ));
                self.root_preview_error = Some(error.to_string());
                return;
            }
        };
        let transform = mat4_from_rows(matrix);
        if let Err(error) = self.canvas.set_root_preview_transform(transform) {
            self.log.append(&format!(
                "[glb_editor] Root transform preview failed: {error}"
            ));
            self.root_preview_error = Some(error);
        } else {
            self.root_preview_error = None;
        }
    }

    /// Report loaded preview geometry bounds and frame the viewport on them
    /// when a model was freshly opened.  Authored assets may use centimetre
    /// or millimetre scales (for example a root node with scale 0.01), so a
    /// fixed default camera can leave the model and skeleton sub-pixel small.
    /// Guide overlays are scaled to the same order as the model.
    pub(crate) fn frame_glb_preview(
        &mut self,
        context: &Context,
        reload_kind: GlbReloadKind,
    ) {
        let part_count =
            self.canvas.model.as_ref().map_or(0, |model| model.len());
        let Some((minimum, maximum)) = self.canvas.preview_bounds() else {
            self.log.append(&format!(
                "[glb_editor] Preview bounds unavailable, parts={part_count}"
            ));
            return;
        };
        self.log.append(&format!(
            "[glb_editor] Preview bounds parts={part_count} min=[{:.5}, {:.5}, {:.5}] max=[{:.5}, {:.5}, {:.5}]",
            minimum[0], minimum[1], minimum[2], maximum[0], maximum[1],
            maximum[2]
        ));
        if reload_kind != GlbReloadKind::OpenModel {
            return;
        }
        let extent = (maximum[0] - minimum[0])
            .max(maximum[1] - minimum[1])
            .max(maximum[2] - minimum[2]);
        self.canvas.set_guide_scale(context, extent.max(1.0e-5));
        self.camera.focus_on_bounds(minimum, maximum);
    }
}

fn mat4_from_rows(matrix: [[f32; 4]; 4]) -> Mat4 {
    Mat4::from_cols(
        vec4(matrix[0][0], matrix[1][0], matrix[2][0], matrix[3][0]),
        vec4(matrix[0][1], matrix[1][1], matrix[2][1], matrix[3][1]),
        vec4(matrix[0][2], matrix[1][2], matrix[2][2], matrix[3][2]),
        vec4(matrix[0][3], matrix[1][3], matrix[2][3], matrix[3][3]),
    )
}
