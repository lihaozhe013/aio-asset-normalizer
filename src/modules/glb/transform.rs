use serde_json::{json, Value};

use super::GlbError;

pub(super) fn node_matrix(node: &Value) -> Result<[[f32; 4]; 4], GlbError> {
    if let Some(matrix) = node.get("matrix").and_then(Value::as_array) {
        if matrix.len() != 16 {
            return Err(GlbError::Invalid(
                "Node matrix must have 16 values".to_owned(),
            ));
        }
        let mut result = [[0.0; 4]; 4];
        for (index, value) in matrix.iter().enumerate() {
            let value = value.as_f64().ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Node matrix component {index} is not numeric"
                ))
            })? as f32;
            if !value.is_finite() {
                return Err(GlbError::Invalid(
                    "Node matrix contains non-finite values".to_owned(),
                ));
            }
            result[index % 4][index / 4] = value;
        }
        return Ok(result);
    }
    let translation = array3(node.get("translation"), [0.0, 0.0, 0.0])?;
    let scale = array3(node.get("scale"), [1.0, 1.0, 1.0])?;
    let rotation = array4(node.get("rotation"), [0.0, 0.0, 0.0, 1.0])?;
    let matrix = multiply(
        translation_matrix(translation),
        multiply(quaternion_matrix(rotation), scale_matrix(scale)),
    );
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        return Err(GlbError::Invalid(
            "Node TRS contains non-finite values".to_owned(),
        ));
    }
    Ok(matrix)
}

pub(super) type NodeTrs = ([f32; 3], [f32; 4], [f32; 3]);

pub(super) fn decompose_matrix(
    matrix: [[f32; 4]; 4],
) -> Result<NodeTrs, GlbError> {
    let translation = [matrix[0][3], matrix[1][3], matrix[2][3]];
    if translation.iter().any(|value| !value.is_finite()) {
        return Err(GlbError::Invalid(
            "Node translation contains non-finite values".to_owned(),
        ));
    }
    let columns = [
        [matrix[0][0], matrix[1][0], matrix[2][0]],
        [matrix[0][1], matrix[1][1], matrix[2][1]],
        [matrix[0][2], matrix[1][2], matrix[2][2]],
    ];
    let mut scale = columns.map(|column| {
        (column[0] * column[0] + column[1] * column[1] + column[2] * column[2])
            .sqrt()
    });
    if scale
        .iter()
        .any(|value| !value.is_finite() || *value <= f32::EPSILON)
    {
        return Err(GlbError::Invalid(
            "Node matrix has a degenerate scale".to_owned(),
        ));
    }
    let mut rotation = [[0.0; 3]; 3];
    for column in 0..3 {
        for row in 0..3 {
            rotation[row][column] = columns[column][row] / scale[column];
        }
    }
    let determinant = rotation[0][0]
        * (rotation[1][1] * rotation[2][2] - rotation[1][2] * rotation[2][1])
        - rotation[0][1]
            * (rotation[1][0] * rotation[2][2]
                - rotation[1][2] * rotation[2][0])
        + rotation[0][2]
            * (rotation[1][0] * rotation[2][1]
                - rotation[1][1] * rotation[2][0]);
    if determinant < 0.0 {
        scale[0] = -scale[0];
        for row in &mut rotation {
            row[0] = -row[0];
        }
    }
    let trace = rotation[0][0] + rotation[1][1] + rotation[2][2];
    let quaternion = if trace > 0.0 {
        let root = (trace + 1.0).sqrt() * 2.0;
        [
            (rotation[2][1] - rotation[1][2]) / root,
            (rotation[0][2] - rotation[2][0]) / root,
            (rotation[1][0] - rotation[0][1]) / root,
            0.25 * root,
        ]
    } else if rotation[0][0] > rotation[1][1] && rotation[0][0] > rotation[2][2]
    {
        let root = (1.0 + rotation[0][0] - rotation[1][1] - rotation[2][2])
            .sqrt()
            * 2.0;
        [
            0.25 * root,
            (rotation[0][1] + rotation[1][0]) / root,
            (rotation[0][2] + rotation[2][0]) / root,
            (rotation[2][1] - rotation[1][2]) / root,
        ]
    } else if rotation[1][1] > rotation[2][2] {
        let root = (1.0 + rotation[1][1] - rotation[0][0] - rotation[2][2])
            .sqrt()
            * 2.0;
        [
            (rotation[0][1] + rotation[1][0]) / root,
            0.25 * root,
            (rotation[1][2] + rotation[2][1]) / root,
            (rotation[0][2] - rotation[2][0]) / root,
        ]
    } else {
        let root = (1.0 + rotation[2][2] - rotation[0][0] - rotation[1][1])
            .sqrt()
            * 2.0;
        [
            (rotation[0][2] + rotation[2][0]) / root,
            (rotation[1][2] + rotation[2][1]) / root,
            0.25 * root,
            (rotation[1][0] - rotation[0][1]) / root,
        ]
    };
    Ok((translation, normalize_quaternion(quaternion), scale))
}

pub(super) fn normalize_quaternion(quaternion: [f32; 4]) -> [f32; 4] {
    let length = quaternion
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if length <= f32::EPSILON || !length.is_finite() {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        quaternion.map(|value| value / length)
    }
}

pub(super) fn set_matrix(node: &mut Value, matrix: [[f32; 4]; 4]) {
    let values = (0..4)
        .flat_map(|column| (0..4).map(move |row| matrix[row][column]))
        .map(|value| json!(value))
        .collect::<Vec<_>>();
    node["matrix"] = Value::Array(values);
    if let Some(object) = node.as_object_mut() {
        object.remove("translation");
        object.remove("rotation");
        object.remove("scale");
    }
}

fn array3(
    value: Option<&Value>,
    default: [f32; 3],
) -> Result<[f32; 3], GlbError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let values = value.as_array().ok_or_else(|| {
        GlbError::Invalid("Node TRS component must be an array".to_owned())
    })?;
    if values.len() != 3 {
        return Err(GlbError::Invalid(
            "Node translation/scale must contain three values".to_owned(),
        ));
    }
    let values = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let value = value.as_f64().ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Node TRS component {index} is not numeric"
                ))
            })? as f32;
            if !value.is_finite() {
                return Err(GlbError::Invalid(
                    "Node TRS contains non-finite values".to_owned(),
                ));
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, GlbError>>()?;
    Ok([values[0], values[1], values[2]])
}

fn array4(
    value: Option<&Value>,
    default: [f32; 4],
) -> Result<[f32; 4], GlbError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let values = value.as_array().ok_or_else(|| {
        GlbError::Invalid("Node TRS component must be an array".to_owned())
    })?;
    if values.len() != 4 {
        return Err(GlbError::Invalid(
            "Node rotation must contain four values".to_owned(),
        ));
    }
    let values = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let value = value.as_f64().ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Node rotation component {index} is not numeric"
                ))
            })? as f32;
            if !value.is_finite() {
                return Err(GlbError::Invalid(
                    "Node rotation contains non-finite values".to_owned(),
                ));
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, GlbError>>()?;
    let quaternion = [values[0], values[1], values[2], values[3]];
    let length = quaternion
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return Err(GlbError::Invalid(
            "Node rotation quaternion must be non-zero".to_owned(),
        ));
    }
    Ok(quaternion)
}

pub(super) fn identity() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub(super) fn multiply(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            result[row][column] =
                (0..4).map(|index| a[row][index] * b[index][column]).sum();
        }
    }
    result
}

pub(super) fn translation_matrix(offset: [f32; 3]) -> [[f32; 4]; 4] {
    let mut matrix = identity();
    matrix[0][3] = offset[0];
    matrix[1][3] = offset[1];
    matrix[2][3] = offset[2];
    matrix
}

pub(super) fn scale_matrix(scale: [f32; 3]) -> [[f32; 4]; 4] {
    let mut matrix = identity();
    matrix[0][0] = scale[0];
    matrix[1][1] = scale[1];
    matrix[2][2] = scale[2];
    matrix
}

pub(super) fn quaternion_matrix(q: [f32; 4]) -> [[f32; 4]; 4] {
    let [x, y, z, w] = q;
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
            0.0,
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
            0.0,
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
