//! One-click corrections for assets authored with a non-glTF up-axis.
//!
//! The exported standard is right-handed Y-Up with -Z forward (see
//! `StandardizationProfile`).  These presets map a declared authoring up
//! axis to the single clean root rotation that brings that axis to +Y, so
//! the user only declares the asset convention instead of typing Euler
//! angles.  Every correction is a proper rotation (determinant +1); the
//! presets never mirror geometry.

/// Up axis an asset was authored with, in the asset's own world space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpAxisPreset {
    /// Already conforms to the glTF standard.
    YUp,
    ZUp,
    NegativeZUp,
    XUp,
    NegativeXUp,
    /// Y axis exists but points down (e.g. some engine exports).
    YDown,
}

impl UpAxisPreset {
    pub const ALL: [Self; 6] = [
        Self::YUp,
        Self::ZUp,
        Self::NegativeZUp,
        Self::XUp,
        Self::NegativeXUp,
        Self::YDown,
    ];

    /// Localization key for the label of this preset.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::YUp => "glb.up_axis.y",
            Self::ZUp => "glb.up_axis.z",
            Self::NegativeZUp => "glb.up_axis.negative_z",
            Self::XUp => "glb.up_axis.x",
            Self::NegativeXUp => "glb.up_axis.negative_x",
            Self::YDown => "glb.up_axis.y_down",
        }
    }

    /// Root Euler correction (degrees) that rotates the authored up axis
    /// onto +Y.  Each entry is a single-axis rotation, so the combined
    /// Euler order used by `RootTransformPreview` is irrelevant here.
    pub const fn correction_euler_degrees(self) -> [f32; 3] {
        match self {
            Self::YUp => [0.0, 0.0, 0.0],
            Self::ZUp => [-90.0, 0.0, 0.0],
            Self::NegativeZUp => [90.0, 0.0, 0.0],
            Self::XUp => [0.0, 0.0, 90.0],
            Self::NegativeXUp => [0.0, 0.0, -90.0],
            // Around Z so that -Z forward is preserved.
            Self::YDown => [0.0, 0.0, 180.0],
        }
    }

    /// Authored up direction this preset stands for.
    fn authored_up(self) -> [f32; 3] {
        match self {
            Self::YUp => [0.0, 1.0, 0.0],
            Self::ZUp => [0.0, 0.0, 1.0],
            Self::NegativeZUp => [0.0, 0.0, -1.0],
            Self::XUp => [1.0, 0.0, 0.0],
            Self::NegativeXUp => [-1.0, 0.0, 0.0],
            Self::YDown => [0.0, -1.0, 0.0],
        }
    }

    /// Reverse-match a correction Euler triple to its preset.  Angles are
    /// compared modulo a full turn, so `-180.0` finds `YDown`.  `None`
    /// means the current rotation no longer corresponds to a preset.
    pub fn from_correction_euler_degrees(euler: [f32; 3]) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| {
            euler_matches(
                preset.correction_euler_degrees(),
                normalize_euler(euler),
            )
        })
    }
}

fn normalize_euler(euler: [f32; 3]) -> [f32; 3] {
    euler.map(|angle| {
        let angle = angle.rem_euclid(360.0);
        if angle > 180.0 {
            angle - 360.0
        } else {
            angle
        }
    })
}

fn euler_matches(left: [f32; 3], right: [f32; 3]) -> bool {
    const EPSILON: f32 = 1.0e-3;
    left.iter()
        .zip(right.iter())
        .all(|(a, b)| (a - b).abs() <= EPSILON)
}

#[cfg(test)]
mod tests {
    use super::super::RootTransformPreview;
    use super::*;

    fn rotation_of_euler(euler: [f32; 3]) -> [[f32; 4]; 4] {
        RootTransformPreview {
            euler_degrees: euler,
            scale: 1.0,
            translation: [0.0, 0.0, 0.0],
        }
        .to_matrix()
        .expect("preset corrections use finite Euler angles")
    }

    fn apply_rotation(matrix: [[f32; 4]; 4], vector: [f32; 3]) -> [f32; 3] {
        (0..3)
            .map(|row| {
                (0..3)
                    .map(|column| matrix[row][column] * vector[column])
                    .sum()
            })
            .collect::<Vec<_>>()
            .try_into()
            .expect("three rows")
    }

    fn determinant_3x3(matrix: &[[f32; 4]; 4]) -> f32 {
        matrix[0][0]
            * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
            - matrix[0][1]
                * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
            + matrix[0][2]
                * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0])
    }

    fn vectors_close(left: [f32; 3], right: [f32; 3]) -> bool {
        left.iter()
            .zip(right.iter())
            .all(|(a, b)| (a - b).abs() < 1.0e-4)
    }

    #[test]
    fn every_preset_maps_its_authored_up_axis_to_positive_y() {
        for preset in UpAxisPreset::ALL {
            let matrix = rotation_of_euler(preset.correction_euler_degrees());
            let corrected = apply_rotation(matrix, preset.authored_up());
            assert!(
                vectors_close(corrected, [0.0, 1.0, 0.0]),
                "preset {preset:?} maps {:?} to {corrected:?}",
                preset.authored_up()
            );
        }
    }

    #[test]
    fn every_preset_is_a_proper_rotation_without_mirroring() {
        for preset in UpAxisPreset::ALL {
            let matrix = rotation_of_euler(preset.correction_euler_degrees());
            let det = determinant_3x3(&matrix);
            assert!(
                (det - 1.0).abs() < 1.0e-4,
                "preset {preset:?} has determinant {det}"
            );
        }
    }

    #[test]
    fn y_down_preset_preserves_negative_z_forward() {
        let matrix =
            rotation_of_euler(UpAxisPreset::YDown.correction_euler_degrees());
        let forward = apply_rotation(matrix, [0.0, 0.0, -1.0]);
        assert!(vectors_close(forward, [0.0, 0.0, -1.0]));
    }

    #[test]
    fn correction_euler_round_trips_through_reverse_lookup() {
        for preset in UpAxisPreset::ALL {
            let euler = preset.correction_euler_degrees();
            assert_eq!(
                UpAxisPreset::from_correction_euler_degrees(euler),
                Some(preset)
            );
        }
    }

    #[test]
    fn reverse_lookup_normalizes_equivalent_angles() {
        assert_eq!(
            UpAxisPreset::from_correction_euler_degrees([0.0, 0.0, -180.0]),
            Some(UpAxisPreset::YDown)
        );
        assert_eq!(
            UpAxisPreset::from_correction_euler_degrees([360.0, 0.0, 0.0]),
            Some(UpAxisPreset::YUp)
        );
    }

    #[test]
    fn reverse_lookup_recludes_unmatched_rotations() {
        assert_eq!(
            UpAxisPreset::from_correction_euler_degrees([45.0, 0.0, 0.0]),
            None
        );
        assert_eq!(
            UpAxisPreset::from_correction_euler_degrees([-90.0, 90.0, 0.0]),
            None
        );
    }
}
