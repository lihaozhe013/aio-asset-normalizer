use crate::app::App;
use crate::modules::glb::RootTransformPreview;
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
}

fn mat4_from_rows(matrix: [[f32; 4]; 4]) -> Mat4 {
    Mat4::from_cols(
        vec4(matrix[0][0], matrix[1][0], matrix[2][0], matrix[3][0]),
        vec4(matrix[0][1], matrix[1][1], matrix[2][1], matrix[3][1]),
        vec4(matrix[0][2], matrix[1][2], matrix[2][2], matrix[3][2]),
        vec4(matrix[0][3], matrix[1][3], matrix[2][3], matrix[3][3]),
    )
}
