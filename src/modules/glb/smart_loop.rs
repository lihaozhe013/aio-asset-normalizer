use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::{GlbDocument, GlbError};
use crate::modules::glb::transform::{
    identity, multiply, node_matrix, normalize_quaternion,
};

const MAX_DRIFT_RATIO: f32 = 0.02;
const MAX_ROOT_ROTATION_RADIANS: f32 = 15.0_f32.to_radians();

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmartLoopOptions {
    pub transition_seconds: f32,
}

impl Default for SmartLoopOptions {
    fn default() -> Self {
        Self {
            transition_seconds: 0.15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmartLoopReport {
    pub original_duration: f32,
    pub new_duration: f32,
    pub root_node: Option<usize>,
    pub drift_ratio: f32,
    pub added_keyframes: usize,
    pub already_looped: bool,
}

#[derive(Debug, Clone)]
struct SamplerSource {
    path: String,
    node: usize,
    times: Vec<f32>,
    values: Vec<Vec<f32>>,
    accessor_type: String,
}

impl GlbDocument {
    pub fn smart_loop_animation(
        &mut self,
        animation_index: usize,
        options: SmartLoopOptions,
    ) -> Result<SmartLoopReport, GlbError> {
        let mut working = self.clone();
        let report = working.apply_smart_loop(animation_index, options)?;
        if !report.already_looped {
            *self = working;
            self.dirty = true;
        }
        Ok(report)
    }

    fn apply_smart_loop(
        &mut self,
        animation_index: usize,
        options: SmartLoopOptions,
    ) -> Result<SmartLoopReport, GlbError> {
        if !options.transition_seconds.is_finite()
            || !(0.01..=2.0).contains(&options.transition_seconds)
        {
            return Err(GlbError::Invalid(
                "Smart LOOP transition must be between 0.01 and 2.00 seconds"
                    .to_owned(),
            ));
        }
        let animation = self
            .json
            .get("animations")
            .and_then(Value::as_array)
            .and_then(|items| items.get(animation_index))
            .cloned()
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Animation {animation_index} does not exist"
                ))
            })?;
        let skins = self
            .json
            .get("skins")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Unsupported(
                    "Smart LOOP requires one valid Skinned GLB".to_owned(),
                )
            })?;
        if skins.len() != 1 {
            return Err(GlbError::Unsupported(
                "Smart LOOP requires exactly one Skin".to_owned(),
            ));
        }
        let skin = self.skin_data()?;
        if skin.joints.len() < 2 {
            return Err(GlbError::Invalid(
                "Smart LOOP requires a Skin with at least two joints"
                    .to_owned(),
            ));
        }
        let channels = animation
            .get("channels")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("Animation has no channels".to_owned())
            })?;
        let samplers = animation
            .get("samplers")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("Animation has no samplers".to_owned())
            })?;
        let mut targets = BTreeMap::<usize, (usize, String)>::new();
        for channel in channels {
            let sampler = channel
                .get("sampler")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    GlbError::Invalid(
                        "Animation channel has no sampler".to_owned(),
                    )
                })? as usize;
            let target = channel.get("target").ok_or_else(|| {
                GlbError::Invalid("Animation channel has no target".to_owned())
            })?;
            let node =
                target.get("node").and_then(Value::as_u64).ok_or_else(|| {
                    GlbError::Invalid("Animation target has no node".to_owned())
                })? as usize;
            let path = target
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    GlbError::Invalid("Animation target has no path".to_owned())
                })?
                .to_owned();
            if path == "weights" {
                return Err(GlbError::Unsupported(
                    "Smart LOOP does not support Morph Target animation"
                        .to_owned(),
                ));
            }
            if targets.insert(sampler, (node, path)).is_some() {
                return Err(GlbError::Unsupported(format!("Animation sampler {sampler} is shared by different targets")));
            }
        }
        if targets.len() != samplers.len() {
            return Err(GlbError::Invalid(
                "Every animation sampler must be referenced by one channel"
                    .to_owned(),
            ));
        }

        let mut sources = Vec::with_capacity(samplers.len());
        let mut duration: f32 = 0.0;
        let mut steps = Vec::new();
        for (index, sampler) in samplers.iter().enumerate() {
            let (node, path) =
                targets.get(&index).cloned().ok_or_else(|| {
                    GlbError::Invalid(
                        "Animation sampler has no channel".to_owned(),
                    )
                })?;
            let interpolation = sampler
                .get("interpolation")
                .and_then(Value::as_str)
                .unwrap_or("LINEAR");
            if interpolation != "LINEAR" {
                return Err(GlbError::Unsupported(format!("Smart LOOP supports LINEAR samplers only (sampler {index} uses {interpolation})")));
            }
            let input = sampler
                .get("input")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    GlbError::Invalid(
                        "Animation sampler has no input accessor".to_owned(),
                    )
                })? as usize;
            let output = sampler
                .get("output")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    GlbError::Invalid(
                        "Animation sampler has no output accessor".to_owned(),
                    )
                })? as usize;
            let times = self
                .read_accessor_f32(input)?
                .into_iter()
                .map(|v| v[0])
                .collect::<Vec<_>>();
            validate_times(&times)?;
            duration =
                duration.max(*times.last().ok_or_else(|| invalid_output())?);
            steps.extend(times.windows(2).map(|pair| pair[1] - pair[0]));
            let accessor = self.accessor(output)?.clone();
            let accessor_type = accessor
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid_output())?
                .to_owned();
            let expected = match path.as_str() {
                "translation" | "scale" => ("VEC3", 3),
                "rotation" => ("VEC4", 4),
                _ => {
                    return Err(GlbError::Unsupported(format!(
                        "Smart LOOP does not support animation path {path}"
                    )))
                }
            };
            if accessor_type != expected.0 {
                return Err(GlbError::Unsupported(format!("Animation sampler {index} has an invalid {path} output type")));
            }
            let values = read_output(self, &accessor)?;
            if values.len() != times.len()
                || values.iter().any(|v| v.len() != expected.1)
            {
                return Err(invalid_output());
            }
            sources.push(SamplerSource {
                path,
                node,
                times,
                values,
                accessor_type,
            });
        }
        if duration <= f32::EPSILON {
            return Err(GlbError::Invalid(
                "Animation duration must be positive".to_owned(),
            ));
        }

        let parents = parent_indices(self)?;
        let root = common_ancestor(&skin.joints, &parents)?;
        let motion_root =
            find_motion_root(&sources, &skin.joints, root, &parents)?;
        let nodes = self
            .json
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid("GLB has no nodes array".to_owned())
            })?;
        let locals = nodes
            .iter()
            .map(node_matrix)
            .collect::<Result<Vec<_>, _>>()?;
        let mut cache = vec![None; locals.len()];
        let mut visiting = vec![false; locals.len()];
        let positions = skin
            .joints
            .iter()
            .map(|joint| {
                let m = world_matrix(
                    *joint,
                    &locals,
                    &parents,
                    &mut cache,
                    &mut visiting,
                )?;
                Ok([m[0][3], m[1][3], m[2][3]])
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        let min_y =
            positions.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let max_y = positions
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let height = max_y - min_y;
        if !height.is_finite() || height <= f32::EPSILON {
            return Err(GlbError::Invalid(
                "Smart LOOP could not determine a valid character height"
                    .to_owned(),
            ));
        }
        let translation_index = sources
            .iter()
            .enumerate()
            .filter(|(_, source)| {
                source.path == "translation"
                    && skin.joints.iter().all(|joint| {
                        ancestors(*joint, &parents)
                            .map(|chain| chain.contains(&source.node))
                            .unwrap_or(false)
                    })
            })
            .min_by_key(|(_, source)| {
                ancestors(source.node, &parents)
                    .map(|chain| chain.len())
                    .unwrap_or(usize::MAX)
            })
            .map(|(index, _)| index);
        let translation_root = translation_index
            .map(|index| sources[index].node)
            .unwrap_or(motion_root);
        let parent_world = parents[translation_root]
            .map(|p| {
                world_matrix(p, &locals, &parents, &mut cache, &mut visiting)
            })
            .transpose()?
            .unwrap_or_else(identity);
        let (local_drift, drift_ratio) = if let Some(index) = translation_index
        {
            let first = sample(&sources[index], 0.0)?;
            let last = sample(&sources[index], duration)?;
            let delta =
                [last[0] - first[0], last[1] - first[1], last[2] - first[2]];
            let world_delta = transform_vector(parent_world, delta);
            let horizontal = [world_delta[0], 0.0, world_delta[2]];
            let ratio = vector_length(horizontal) / height;
            if !ratio.is_finite() {
                return Err(GlbError::Invalid(
                    "Root drift is not finite".to_owned(),
                ));
            }
            if ratio > MAX_DRIFT_RATIO {
                return Err(GlbError::Unsupported(format!("Root Motion detected: horizontal drift is {:.2}% of character height (limit is 2.00%)", ratio * 100.0)));
            }
            (inverse_transform_vector(parent_world, horizontal).ok_or_else(|| GlbError::Unsupported("Smart LOOP cannot transform root drift through a degenerate parent".to_owned()))?, ratio)
        } else {
            ([0.0; 3], 0.0)
        };
        if let Some(index) = sources
            .iter()
            .position(|s| s.node == motion_root && s.path == "rotation")
        {
            let first: [f32; 4] = sample(&sources[index], 0.0)?
                .try_into()
                .map_err(|_| invalid_output())?;
            let last: [f32; 4] = sample(&sources[index], duration)?
                .try_into()
                .map_err(|_| invalid_output())?;
            if quaternion_angle(first, last) > MAX_ROOT_ROTATION_RADIANS {
                return Err(GlbError::Unsupported("Root Motion detected: root rotation changes by more than 15 degrees".to_owned()));
            }
        }
        let already_looped = local_drift.iter().all(|v| v.abs() <= 1e-5)
            && sources.iter().all(|s| {
                let a = sample(s, 0.0).ok();
                let b = sample(s, duration).ok();
                match (a, b, s.path.as_str()) {
                    (Some(a), Some(b), "rotation") => a
                        .as_slice()
                        .try_into()
                        .ok()
                        .zip(b.as_slice().try_into().ok())
                        .is_some_and(|(a, b)| quaternion_angle(a, b) <= 1e-3),
                    (Some(a), Some(b), _) => vector_distance(&a, &b) <= 1e-5,
                    _ => false,
                }
            });
        if already_looped {
            return Ok(SmartLoopReport {
                original_duration: duration,
                new_duration: duration,
                root_node: Some(motion_root),
                drift_ratio,
                added_keyframes: 0,
                already_looped: true,
            });
        }

        let step = median(&mut steps)
            .unwrap_or(1.0 / 30.0)
            .clamp(1.0 / 120.0, 1.0 / 15.0);
        let frames =
            (options.transition_seconds / step).ceil().max(2.0) as usize;
        let mut updates = Vec::with_capacity(sources.len());
        for source in &sources {
            let mut times = vec![0.0];
            times.extend(
                source
                    .times
                    .iter()
                    .copied()
                    .filter(|t| *t > 0.0 && *t < duration),
            );
            times.push(duration);
            times.sort_by(f32::total_cmp);
            times.dedup_by(|a, b| (*a - *b).abs() <= f32::EPSILON);
            let mut values = times
                .iter()
                .map(|t| sample(source, *t))
                .collect::<Result<Vec<_>, _>>()?;
            if source.node == translation_root && source.path == "translation" {
                for (time, value) in times.iter().zip(&mut values) {
                    let f = *time / duration;
                    for i in 0..3 {
                        value[i] -= local_drift[i] * f;
                    }
                }
            }
            let first = values[0].clone();
            let last = values.last().cloned().ok_or_else(invalid_output)?;
            for frame in 1..=frames {
                let amount = frame as f32 / frames as f32;
                let eased = amount * amount * (3.0 - 2.0 * amount);
                times.push(duration + options.transition_seconds * amount);
                values.push(blend(&last, &first, eased, &source.path)?);
            }
            updates.push((times, values, source.accessor_type.clone()));
        }
        let mut accessors = Vec::with_capacity(updates.len());
        for (times, values, kind) in updates {
            let input = self.append_float_accessor(
                &times.iter().map(|t| vec![*t]).collect::<Vec<_>>(),
                "SCALAR",
            )?;
            let output = self.append_float_accessor(&values, &kind)?;
            accessors.push((input, output));
        }
        let animation = self
            .json
            .get_mut("animations")
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(animation_index))
            .ok_or_else(|| {
                GlbError::Invalid(
                    "Animation disappeared during Smart LOOP".to_owned(),
                )
            })?;
        let samplers = animation
            .get_mut("samplers")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                GlbError::Invalid("Animation has no samplers".to_owned())
            })?;
        for (sampler, (input, output)) in samplers.iter_mut().zip(accessors) {
            sampler["input"] = json!(input);
            sampler["output"] = json!(output);
        }
        Ok(SmartLoopReport {
            original_duration: duration,
            new_duration: duration + options.transition_seconds,
            root_node: Some(motion_root),
            drift_ratio,
            added_keyframes: frames * sources.len(),
            already_looped: false,
        })
    }
}

fn invalid_output() -> GlbError {
    GlbError::Invalid("Animation output is invalid".to_owned())
}
fn validate_times(times: &[f32]) -> Result<(), GlbError> {
    if times.len() < 2 || times.iter().any(|t| !t.is_finite() || *t < 0.0) {
        return Err(GlbError::Invalid("Animation sampler needs at least two finite non-negative keyframe times".to_owned()));
    }
    if times.windows(2).any(|p| p[1] <= p[0]) {
        return Err(GlbError::Invalid(
            "Animation keyframe times must be strictly increasing".to_owned(),
        ));
    }
    Ok(())
}
fn read_output(
    document: &GlbDocument,
    accessor: &Value,
) -> Result<Vec<Vec<f32>>, GlbError> {
    if accessor.get("componentType").and_then(Value::as_u64) != Some(5126) {
        return Err(GlbError::Unsupported(
            "Smart LOOP requires float animation outputs".to_owned(),
        ));
    }
    let components = match accessor.get("type").and_then(Value::as_str) {
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        _ => {
            return Err(GlbError::Unsupported(
                "Smart LOOP supports VEC3 and VEC4 outputs only".to_owned(),
            ))
        }
    };
    let count = accessor
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(invalid_output)? as usize;
    let bytes = document.accessor_bytes(accessor, components * 4)?;
    (0..count)
        .map(|index| {
            let value = (0..components)
                .map(|component| {
                    let offset = (index * components + component) * 4;
                    let raw = bytes
                        .get(offset..offset + 4)
                        .ok_or_else(invalid_output)?;
                    Ok(f32::from_le_bytes(
                        raw.try_into().map_err(|_| invalid_output())?,
                    ))
                })
                .collect::<Result<Vec<_>, GlbError>>()?;
            if value.iter().any(|component| !component.is_finite()) {
                return Err(invalid_output());
            }
            Ok(value)
        })
        .collect()
}
fn sample(source: &SamplerSource, time: f32) -> Result<Vec<f32>, GlbError> {
    let right = source.times.partition_point(|t| *t < time);
    if right == 0 {
        return source.values.first().cloned().ok_or_else(invalid_output);
    }
    if right == source.times.len() {
        return source.values.last().cloned().ok_or_else(invalid_output);
    }
    let left = right - 1;
    let amount = (time - source.times[left])
        / (source.times[right] - source.times[left]);
    blend(
        &source.values[left],
        &source.values[right],
        amount,
        &source.path,
    )
}
fn blend(
    left: &[f32],
    right: &[f32],
    amount: f32,
    path: &str,
) -> Result<Vec<f32>, GlbError> {
    if left.len() != right.len()
        || left.iter().chain(right).any(|v| !v.is_finite())
    {
        return Err(invalid_output());
    }
    if path == "rotation" {
        return Ok(slerp(
            left.try_into().map_err(|_| invalid_output())?,
            right.try_into().map_err(|_| invalid_output())?,
            amount,
        )
        .to_vec());
    }
    Ok(left
        .iter()
        .zip(right)
        .map(|(a, b)| a + (b - a) * amount)
        .collect())
}
fn slerp(left: [f32; 4], right: [f32; 4], amount: f32) -> [f32; 4] {
    let left = normalize_quaternion(left);
    let mut right = normalize_quaternion(right);
    let mut dot = left.iter().zip(right).map(|(a, b)| a * b).sum::<f32>();
    if dot < 0.0 {
        dot = -dot;
        right = right.map(|v| -v);
    }
    if dot > 0.9995 {
        return normalize_quaternion([
            left[0] + (right[0] - left[0]) * amount,
            left[1] + (right[1] - left[1]) * amount,
            left[2] + (right[2] - left[2]) * amount,
            left[3] + (right[3] - left[3]) * amount,
        ]);
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let d = theta.sin();
    normalize_quaternion([
        left[0] * ((1.0 - amount) * theta).sin() / d
            + right[0] * (amount * theta).sin() / d,
        left[1] * ((1.0 - amount) * theta).sin() / d
            + right[1] * (amount * theta).sin() / d,
        left[2] * ((1.0 - amount) * theta).sin() / d
            + right[2] * (amount * theta).sin() / d,
        left[3] * ((1.0 - amount) * theta).sin() / d
            + right[3] * (amount * theta).sin() / d,
    ])
}
fn parent_indices(
    document: &GlbDocument,
) -> Result<Vec<Option<usize>>, GlbError> {
    let nodes = document
        .json
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            GlbError::Invalid("GLB has no nodes array".to_owned())
        })?;
    let mut parents = vec![None; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            for child in children
                .iter()
                .filter_map(Value::as_u64)
                .map(|v| v as usize)
            {
                if child >= parents.len() {
                    return Err(GlbError::Invalid(
                        "Node references a missing child".to_owned(),
                    ));
                }
                if parents[child].replace(index).is_some() {
                    return Err(GlbError::Invalid(
                        "Node has multiple parents".to_owned(),
                    ));
                }
            }
        }
    }
    Ok(parents)
}
fn ancestors(
    mut node: usize,
    parents: &[Option<usize>],
) -> Result<Vec<usize>, GlbError> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(node) {
            return Err(GlbError::Invalid(
                "Node hierarchy contains a cycle".to_owned(),
            ));
        }
        result.push(node);
        match parents.get(node).copied().flatten() {
            Some(parent) => node = parent,
            None => break,
        }
    }
    Ok(result)
}
fn common_ancestor(
    joints: &[usize],
    parents: &[Option<usize>],
) -> Result<usize, GlbError> {
    let mut common = ancestors(
        *joints.first().ok_or_else(|| {
            GlbError::Invalid("Skin has no joints".to_owned())
        })?,
        parents,
    )?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for joint in joints.iter().skip(1) {
        let set = ancestors(*joint, parents)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        common = common.intersection(&set).copied().collect();
    }
    common
        .into_iter()
        .min_by_key(|node| {
            ancestors(*node, parents)
                .map(|chain| chain.len())
                .unwrap_or(usize::MAX)
        })
        .ok_or_else(|| {
            GlbError::Invalid("Skin joints have no common root".to_owned())
        })
}
fn find_motion_root(
    sources: &[SamplerSource],
    joints: &[usize],
    fallback: usize,
    parents: &[Option<usize>],
) -> Result<usize, GlbError> {
    let mut common = ancestors(joints[0], parents)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    for joint in joints.iter().skip(1) {
        let set = ancestors(*joint, parents)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        common = common.intersection(&set).copied().collect();
    }
    let mut candidates = sources
        .iter()
        .filter(|s| {
            (s.path == "translation" || s.path == "rotation")
                && common.contains(&s.node)
        })
        .map(|s| s.node)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|node| {
        ancestors(*node, parents)
            .map(|chain| chain.len())
            .unwrap_or(usize::MAX)
    });
    match candidates.as_slice() {
        [] => Ok(fallback),
        [node] => Ok(*node),
        [a, b, ..]
            if ancestors(*a, parents)?.len()
                == ancestors(*b, parents)?.len() =>
        {
            Err(GlbError::Unsupported(
                "Smart LOOP could not identify a unique animated skeleton root"
                    .to_owned(),
            ))
        }
        [node, ..] => Ok(*node),
    }
}
fn world_matrix(
    index: usize,
    local: &[[[f32; 4]; 4]],
    parents: &[Option<usize>],
    cache: &mut [Option<[[f32; 4]; 4]>],
    visiting: &mut [bool],
) -> Result<[[f32; 4]; 4], GlbError> {
    if let Some(matrix) = cache[index] {
        return Ok(matrix);
    }
    if visiting[index] {
        return Err(GlbError::Invalid(
            "Node hierarchy contains a cycle".to_owned(),
        ));
    }
    visiting[index] = true;
    let result = parents[index]
        .map(|parent| {
            world_matrix(parent, local, parents, cache, visiting)
                .map(|m| multiply(m, local[index]))
        })
        .transpose()?
        .unwrap_or(local[index]);
    visiting[index] = false;
    cache[index] = Some(result);
    Ok(result)
}
fn transform_vector(m: [[f32; 4]; 4], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}
fn inverse_transform_vector(m: [[f32; 4]; 4], v: [f32; 3]) -> Option<[f32; 3]> {
    let (a, b, c) = (m[0][0], m[0][1], m[0][2]);
    let (d, e, f) = (m[1][0], m[1][1], m[1][2]);
    let (g, h, i) = (m[2][0], m[2][1], m[2][2]);
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det.abs() <= f32::EPSILON {
        return None;
    }
    Some([
        ((e * i - f * h) * v[0]
            + (c * h - b * i) * v[1]
            + (b * f - c * e) * v[2])
            / det,
        ((f * g - d * i) * v[0]
            + (a * i - c * g) * v[1]
            + (c * d - a * f) * v[2])
            / det,
        ((d * h - e * g) * v[0]
            + (b * g - a * h) * v[1]
            + (a * e - b * d) * v[2])
            / det,
    ])
}
fn vector_length(v: [f32; 3]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}
fn vector_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}
fn quaternion_angle(a: [f32; 4], b: [f32; 4]) -> f32 {
    let a = normalize_quaternion(a);
    let b = normalize_quaternion(b);
    2.0 * a
        .iter()
        .zip(b)
        .map(|(x, y)| x * y)
        .sum::<f32>()
        .abs()
        .clamp(0.0, 1.0)
        .acos()
}
fn median(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        None
    } else {
        values.sort_by(f32::total_cmp);
        Some(values[values.len() / 2])
    }
}

#[cfg(test)]
#[path = "smart_loop_tests.rs"]
mod tests;
