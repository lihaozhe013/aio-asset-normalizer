use serde_json::{json, Value};

use super::{GlbDocument, GlbError};

impl GlbDocument {
    pub(super) fn trim_animation_interpolated(
        &mut self,
        animation_index: usize,
        start: f32,
        end: f32,
    ) -> Result<(), GlbError> {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start
        {
            return Err(GlbError::Invalid(
                "Animation range must satisfy 0 <= start < end".to_owned(),
            ));
        }
        let animation = self
            .json
            .get("animations")
            .and_then(Value::as_array)
            .and_then(|animations| animations.get(animation_index))
            .cloned()
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Animation {animation_index} does not exist"
                ))
            })?;
        let samplers = animation
            .get("samplers")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("Animation has no samplers".to_owned())
            })?;
        let channels = animation
            .get("channels")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("Animation has no channels".to_owned())
            })?;
        let mut updates = Vec::with_capacity(samplers.len());
        for (sampler_index, sampler) in samplers.iter().enumerate() {
            let input_index = sampler
                .get("input")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    GlbError::Invalid(
                        "Animation sampler has no input accessor".to_owned(),
                    )
                })? as usize;
            let output_index = sampler
                .get("output")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    GlbError::Invalid(
                        "Animation sampler has no output accessor".to_owned(),
                    )
                })? as usize;
            let input = self
                .read_accessor_f32(input_index)?
                .into_iter()
                .map(|value| value[0])
                .collect::<Vec<_>>();
            validate_timeline(&input)?;
            let input_end = *input.last().ok_or_else(|| {
                GlbError::Invalid(
                    "Animation sampler has no keyframes".to_owned(),
                )
            })?;
            if start < input[0] || end > input_end {
                return Err(GlbError::Invalid(format!(
                    "Animation range [{start}, {end}] is outside sampler {sampler_index}"
                )));
            }
            let output_accessor = self.accessor(output_index)?.clone();
            let output = read_float_output(self, &output_accessor)?;
            if output.len() != input.len() {
                return Err(GlbError::Invalid(format!(
                    "Animation sampler {sampler_index} input/output counts differ"
                )));
            }
            let path = channels
                .iter()
                .filter(|channel| {
                    channel.get("sampler").and_then(Value::as_u64)
                        == Some(sampler_index as u64)
                })
                .filter_map(|channel| {
                    channel
                        .get("target")
                        .and_then(|target| target.get("path"))
                        .and_then(Value::as_str)
                })
                .next()
                .unwrap_or("translation");
            let interpolation = sampler
                .get("interpolation")
                .and_then(Value::as_str)
                .unwrap_or("LINEAR");
            if interpolation == "CUBICSPLINE" {
                return Err(GlbError::Unsupported(
                    "CUBICSPLINE animation trimming is not supported yet"
                        .to_owned(),
                ));
            }
            let times = trim_times(&input, start, end);
            let values = times
                .iter()
                .map(|time| {
                    sample_value(&input, &output, *time, path, interpolation)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let new_input = self.append_float_accessor(
                &times
                    .iter()
                    .map(|time| vec![*time - start])
                    .collect::<Vec<_>>(),
                "SCALAR",
            )?;
            let output_type = output_accessor
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    GlbError::Invalid("Animation output has no type".to_owned())
                })?;
            let new_output =
                self.append_float_accessor(&values, output_type)?;
            updates.push((new_input, new_output));
        }
        let animation = self
            .json
            .get_mut("animations")
            .and_then(Value::as_array_mut)
            .and_then(|animations| animations.get_mut(animation_index))
            .ok_or_else(|| {
                GlbError::Invalid(
                    "Animation disappeared during trimming".to_owned(),
                )
            })?;
        let samplers = animation
            .get_mut("samplers")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("Animation has no samplers".to_owned())
            })?;
        for (sampler, (input, output)) in samplers.iter_mut().zip(updates) {
            sampler["input"] = json!(input);
            sampler["output"] = json!(output);
        }
        Ok(())
    }
}

fn validate_timeline(times: &[f32]) -> Result<(), GlbError> {
    if times.len() < 2 || times.iter().any(|time| !time.is_finite()) {
        return Err(GlbError::Invalid(
            "Animation sampler needs at least two finite keyframe times"
                .to_owned(),
        ));
    }
    if times.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Err(GlbError::Invalid(
            "Animation keyframe times must be strictly increasing".to_owned(),
        ));
    }
    Ok(())
}

fn trim_times(input: &[f32], start: f32, end: f32) -> Vec<f32> {
    let mut times = vec![start];
    times.extend(
        input
            .iter()
            .copied()
            .filter(|time| *time > start && *time < end),
    );
    if end > start {
        times.push(end);
    }
    times
}

fn read_float_output(
    document: &GlbDocument,
    accessor: &Value,
) -> Result<Vec<Vec<f32>>, GlbError> {
    if accessor.get("componentType").and_then(Value::as_u64) != Some(5126) {
        return Err(GlbError::Unsupported(
            "Animation outputs must use float components for interpolation"
                .to_owned(),
        ));
    }
    let components = match accessor.get("type").and_then(Value::as_str) {
        Some("SCALAR") => 1,
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        _ => {
            return Err(GlbError::Unsupported(
                "Animation output accessor type is unsupported".to_owned(),
            ))
        }
    };
    let count =
        accessor
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                GlbError::Invalid("Animation output has no count".to_owned())
            })? as usize;
    let bytes = document.accessor_bytes(accessor, components * 4)?;
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let mut value = Vec::with_capacity(components);
        for component in 0..components {
            let offset = (index * components + component) * 4;
            let raw = bytes.get(offset..offset + 4).ok_or_else(|| {
                GlbError::Invalid(
                    "Animation output exceeds BIN chunk".to_owned(),
                )
            })?;
            value.push(f32::from_le_bytes(raw.try_into().map_err(|_| {
                GlbError::Invalid(
                    "Animation output component is truncated".to_owned(),
                )
            })?));
        }
        values.push(value);
    }
    Ok(values)
}

fn sample_value(
    times: &[f32],
    values: &[Vec<f32>],
    time: f32,
    path: &str,
    interpolation: &str,
) -> Result<Vec<f32>, GlbError> {
    let right = times.partition_point(|key| *key < time);
    if right == 0 {
        return values.first().cloned().ok_or_else(|| {
            GlbError::Invalid(
                "Animation sampler has no output values".to_owned(),
            )
        });
    }
    if right == times.len() {
        return values.last().cloned().ok_or_else(|| {
            GlbError::Invalid(
                "Animation sampler has no output values".to_owned(),
            )
        });
    }
    let left = right - 1;
    if interpolation == "STEP" || (time - times[left]).abs() <= f32::EPSILON {
        return Ok(values[left].clone());
    }
    let amount = (time - times[left]) / (times[right] - times[left]);
    if path == "rotation" && values[left].len() == 4 {
        Ok(slerp(
            values[left].as_slice(),
            values[right].as_slice(),
            amount,
        ))
    } else {
        Ok(values[left]
            .iter()
            .zip(&values[right])
            .map(|(left, right)| left + (right - left) * amount)
            .collect())
    }
}

fn slerp(left: &[f32], right: &[f32], amount: f32) -> Vec<f32> {
    let mut right = right.to_vec();
    let mut dot = left.iter().zip(&right).map(|(a, b)| a * b).sum::<f32>();
    if dot < 0.0 {
        dot = -dot;
        for value in &mut right {
            *value = -*value;
        }
    }
    if dot > 0.9995 {
        let blended = left
            .iter()
            .zip(&right)
            .map(|(left, right)| left + (right - left) * amount)
            .collect::<Vec<_>>();
        if blended.len() == 4 {
            return normalize_quaternion([
                blended[0], blended[1], blended[2], blended[3],
            ])
            .to_vec();
        }
        return blended;
    }
    let theta = dot.acos();
    let scale_left = ((1.0 - amount) * theta).sin() / theta.sin();
    let scale_right = (amount * theta).sin() / theta.sin();
    left.iter()
        .zip(&right)
        .map(|(left, right)| left * scale_left + right * scale_right)
        .collect()
}

fn normalize_quaternion(quaternion: [f32; 4]) -> [f32; 4] {
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
