//! Root translation motion removal for compact GLB animation exports.

use std::collections::{BTreeSet, HashSet};

use serde_json::{json, Value};

use super::export_selection::GlbExportSelection;
use super::{GlbDocument, GlbError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootMotionInfo {
    pub(crate) resolved_node: Option<usize>,
    pub(crate) candidates: Vec<usize>,
    pub(crate) animations_without_track: Vec<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct RootMotionPlan {
    pub(super) node: usize,
    pub(super) candidate_nodes: Vec<usize>,
    pub(super) channels: Vec<RootMotionChannel>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RootMotionChannel {
    pub(super) animation: usize,
    pub(super) channel: usize,
    pub(super) node: usize,
}

#[derive(Debug, Clone)]
struct RootMotionChannelData {
    values: Vec<[f32; 3]>,
}

#[derive(Debug, Clone)]
struct RootHierarchyInfo {
    common_ancestors: BTreeSet<usize>,
    preferred_node: Option<usize>,
    depths: Vec<usize>,
}

impl GlbDocument {
    /// Return root-motion candidates for the currently selected animations.
    pub(crate) fn root_motion_info(
        &self,
        selection: &GlbExportSelection,
    ) -> Result<RootMotionInfo, GlbError> {
        if selection.preset == super::GlbExportPreset::PreserveAll
            || !selection.remove_root_motion
            || selection.selected_animations.is_empty()
        {
            return Ok(RootMotionInfo {
                resolved_node: None,
                candidates: Vec::new(),
                animations_without_track: Vec::new(),
            });
        }
        let animation_indices = selection
            .selected_animations
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let plan = self
            .plan_root_motion(
                selection,
                &animation_indices,
                selection.skin_index,
            )?
            .ok_or_else(|| {
                GlbError::Invalid(
                    "Root motion plan is unavailable for this selection"
                        .to_owned(),
                )
            })?;
        Ok(RootMotionInfo {
            resolved_node: Some(plan.node),
            candidates: plan.candidate_nodes,
            animations_without_track: missing_animation_indices(
                &animation_indices,
                &plan.channels,
                plan.node,
            ),
        })
    }

    pub(super) fn plan_root_motion(
        &self,
        selection: &GlbExportSelection,
        animation_indices: &[usize],
        skin_index: Option<usize>,
    ) -> Result<Option<RootMotionPlan>, GlbError> {
        if selection.preset == super::GlbExportPreset::PreserveAll
            || !selection.remove_root_motion
            || animation_indices.is_empty()
        {
            return Ok(None);
        }

        let nodes = json_array(&self.json, "nodes")?;
        let parents = build_parents(nodes)?;
        let channels = collect_translation_channels(
            &self.json,
            animation_indices,
            nodes.len(),
        )?;
        let channel_nodes = unique_channel_nodes(&channels);
        let hierarchy =
            root_hierarchy(&self.json, &parents, selection, skin_index)?;
        let node = resolve_root_node(
            selection.root_motion_node_override,
            &channel_nodes,
            &hierarchy,
        )?;
        let selected_channels = channels
            .iter()
            .copied()
            .filter(|channel| channel.node == node)
            .collect::<Vec<_>>();
        for channel in &selected_channels {
            inspect_root_motion_channel(self, *channel)?;
        }

        let missing = missing_animation_indices(
            animation_indices,
            &selected_channels,
            node,
        );
        let warnings = if missing.is_empty() {
            Vec::new()
        } else {
            tracing::warn!(
                target: "glb_export",
                root_node = node,
                animations = ?missing,
                "[root_motion] Selected animation has no translation channel on the resolved root"
            );
            vec![format!(
                "No translation channel targets root motion node {node} for selected animation(s): {}",
                format_indices(&missing)
            )]
        };
        Ok(Some(RootMotionPlan {
            node,
            candidate_nodes: channel_nodes,
            channels: selected_channels,
            warnings,
        }))
    }

    pub(super) fn apply_root_motion(
        &mut self,
        plan: &RootMotionPlan,
    ) -> Result<usize, GlbError> {
        let mut modified = 0;
        for channel in &plan.channels {
            let data = inspect_root_motion_channel(self, *channel)?;
            let first = *data.values.first().ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Animation {} channel {} has no translation keyframes",
                    channel.animation, channel.channel
                ))
            })?;
            if data.values.iter().all(|value| *value == first) {
                continue;
            }
            let values = vec![first.to_vec(); data.values.len()];
            let output_accessor =
                self.append_float_accessor(&values, "VEC3")
                    .map_err(|error| context_error(*channel, error))?;
            replace_sampler_output(
                self,
                channel.animation,
                channel.channel,
                output_accessor,
            )?;
            modified += 1;
        }
        if modified > 0 {
            self.dirty = true;
        }
        tracing::info!(
            target: "glb_export",
            channels_modified = modified,
            root_node = plan.node,
            "[root_motion] Removed root translation motion from export copy"
        );
        Ok(modified)
    }
}

fn json_array<'a>(json: &'a Value, key: &str) -> Result<&'a [Value], GlbError> {
    match json.get(key) {
        None => Ok(&[]),
        Some(value) => value.as_array().map(Vec::as_slice).ok_or_else(|| {
            GlbError::Invalid(format!("GLB {key} field is not an array"))
        }),
    }
}

fn read_index(value: &Value, context: &str) -> Result<usize, GlbError> {
    let value = value.as_u64().ok_or_else(|| {
        GlbError::Invalid(format!("{context} index is not an unsigned integer"))
    })?;
    usize::try_from(value).map_err(|_| {
        GlbError::Invalid(format!("{context} index is out of range"))
    })
}

fn build_parents(nodes: &[Value]) -> Result<Vec<Option<usize>>, GlbError> {
    let mut parents = vec![None; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        let Some(children) = node.get("children") else {
            continue;
        };
        let children = children.as_array().ok_or_else(|| {
            GlbError::Invalid(format!("Node {index} children is not an array"))
        })?;
        for child in children {
            let child = read_index(child, &format!("Node {index} child"))?;
            if child >= nodes.len() {
                return Err(GlbError::Invalid(format!(
                    "Node {index} references missing child {child}"
                )));
            }
            if parents[child].replace(index).is_some() {
                return Err(GlbError::Invalid(format!(
                    "Node {child} has more than one parent"
                )));
            }
        }
    }
    for start in 0..nodes.len() {
        let mut seen = HashSet::new();
        let mut current = Some(start);
        while let Some(index) = current {
            if !seen.insert(index) {
                return Err(GlbError::Invalid(format!(
                    "Node hierarchy contains a cycle at node {index}"
                )));
            }
            current = parents[index];
        }
    }
    Ok(parents)
}

fn ancestors(
    node: usize,
    parents: &[Option<usize>],
) -> Result<Vec<usize>, GlbError> {
    if node >= parents.len() {
        return Err(GlbError::Invalid(format!(
            "Root motion node {node} does not exist"
        )));
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(node);
    while let Some(index) = current {
        if !seen.insert(index) {
            return Err(GlbError::Invalid(format!(
                "Node hierarchy contains a cycle at node {index}"
            )));
        }
        result.push(index);
        current = parents[index];
    }
    Ok(result)
}

fn common_ancestors(
    seeds: &[usize],
    parents: &[Option<usize>],
) -> Result<BTreeSet<usize>, GlbError> {
    let Some(first) = seeds.first().copied() else {
        return Ok(BTreeSet::new());
    };
    let mut common = ancestors(first, parents)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    for seed in seeds.iter().copied().skip(1) {
        let chain = ancestors(seed, parents)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        common.retain(|node| chain.contains(node));
    }
    Ok(common)
}

fn root_hierarchy(
    json: &Value,
    parents: &[Option<usize>],
    selection: &GlbExportSelection,
    skin_index: Option<usize>,
) -> Result<RootHierarchyInfo, GlbError> {
    let nodes = json_array(json, "nodes")?;
    let (seeds, preferred_node) = if let Some(skin_index) = skin_index {
        let skins = json_array(json, "skins")?;
        let skin = skins.get(skin_index).ok_or_else(|| {
            GlbError::Invalid(format!(
                "Selected Skin {skin_index} does not exist"
            ))
        })?;
        let joints =
            skin.get("joints")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    GlbError::Invalid(format!(
                        "Skin {skin_index} has no joints array"
                    ))
                })?;
        if joints.is_empty() {
            return Err(GlbError::Invalid(format!(
                "Skin {skin_index} has no joints"
            )));
        }
        let joints = joints
            .iter()
            .enumerate()
            .map(|(joint_index, joint)| {
                let node = read_index(
                    joint,
                    &format!("Skin {skin_index} joint {joint_index}"),
                )?;
                if node >= nodes.len() {
                    return Err(GlbError::Invalid(format!(
                        "Skin {skin_index} joint {joint_index} targets missing node {node}"
                    )));
                }
                Ok(node)
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        let preferred = skin
            .get("skeleton")
            .map(|value| {
                read_index(value, &format!("Skin {skin_index} skeleton"))
            })
            .transpose()?;
        if let Some(preferred) = preferred {
            if preferred >= nodes.len() {
                return Err(GlbError::Invalid(format!(
                    "Skin {skin_index} skeleton targets missing node {preferred}"
                )));
            }
        }
        (joints, preferred)
    } else {
        let seeds =
            selection.selected_nodes.iter().copied().collect::<Vec<_>>();
        if seeds.is_empty() {
            return Err(GlbError::Invalid(
                "Automatic root motion detection requires selected nodes or a Skin"
                    .to_owned(),
            ));
        }
        for seed in &seeds {
            if *seed >= nodes.len() {
                return Err(GlbError::Invalid(format!(
                    "Selected node {seed} does not exist"
                )));
            }
        }
        (seeds, None)
    };
    Ok(RootHierarchyInfo {
        common_ancestors: common_ancestors(&seeds, parents)?,
        preferred_node,
        depths: (0..nodes.len())
            .map(|node| ancestors(node, parents).map(|chain| chain.len()))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn resolve_root_node(
    override_node: Option<usize>,
    channel_nodes: &[usize],
    hierarchy: &RootHierarchyInfo,
) -> Result<usize, GlbError> {
    if let Some(node) = override_node {
        return if node < hierarchy.depths.len() {
            Ok(node)
        } else {
            Err(GlbError::Invalid(format!(
                "Root Motion Node {node} does not exist"
            )))
        };
    }

    if let Some(preferred) = hierarchy.preferred_node {
        if channel_nodes.contains(&preferred) {
            return Ok(preferred);
        }
        if hierarchy.common_ancestors.is_empty() {
            return Ok(preferred);
        }
    }

    if hierarchy.common_ancestors.is_empty() {
        return Err(GlbError::Invalid(
            "Automatic root motion detection is ambiguous; select a Root Motion Node manually"
                .to_owned(),
        ));
    }

    let mut candidates = channel_nodes
        .iter()
        .copied()
        .filter(|node| hierarchy.common_ancestors.contains(node))
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    if candidates.is_empty() {
        return Ok(hierarchy
            .preferred_node
            .filter(|node| *node < hierarchy.depths.len())
            .unwrap_or_else(|| {
                hierarchy
                    .common_ancestors
                    .iter()
                    .copied()
                    .min_by_key(|node| hierarchy.depths[*node])
                    .unwrap_or_default()
            }));
    }

    let minimum_depth = candidates
        .iter()
        .map(|node| hierarchy.depths[*node])
        .min()
        .unwrap_or_default();
    let best = candidates
        .into_iter()
        .filter(|node| hierarchy.depths[*node] == minimum_depth)
        .collect::<Vec<_>>();
    if best.len() != 1 {
        return Err(GlbError::Invalid(
            "Automatic root motion detection is ambiguous; select a Root Motion Node manually"
                .to_owned(),
        ));
    }
    Ok(best[0])
}

fn collect_translation_channels(
    json: &Value,
    animation_indices: &[usize],
    node_count: usize,
) -> Result<Vec<RootMotionChannel>, GlbError> {
    let animations = json_array(json, "animations")?;
    let mut channels = Vec::new();
    for animation_index in animation_indices {
        let animation = animations.get(*animation_index).ok_or_else(|| {
            GlbError::Invalid(format!(
                "Selected animation {animation_index} does not exist"
            ))
        })?;
        let animation_channels = animation
            .get("channels")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Animation {animation_index} has no channels array"
                ))
            })?;
        let samplers = animation
            .get("samplers")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                GlbError::Invalid(format!(
                    "Animation {animation_index} has no samplers array"
                ))
            })?;
        for (channel_index, channel) in animation_channels.iter().enumerate() {
            let target = channel.get("target").ok_or_else(|| {
                channel_error(*animation_index, channel_index, "has no target")
            })?;
            if target.get("path").and_then(Value::as_str) != Some("translation")
            {
                continue;
            }
            let node = target
                .get("node")
                .ok_or_else(|| {
                    channel_error(
                        *animation_index,
                        channel_index,
                        "has no target node",
                    )
                })
                .and_then(|value| {
                    read_index(
                        value,
                        &format!(
                            "Animation {animation_index} channel {channel_index} target node"
                        ),
                    )
                })?;
            if node >= node_count {
                return Err(channel_error(
                    *animation_index,
                    channel_index,
                    &format!("targets missing node {node}"),
                ));
            }
            let sampler = channel
                .get("sampler")
                .ok_or_else(|| {
                    channel_error(
                        *animation_index,
                        channel_index,
                        "has no sampler",
                    )
                })
                .and_then(|value| {
                    read_index(
                        value,
                        &format!(
                            "Animation {animation_index} channel {channel_index} sampler"
                        ),
                    )
                })?;
            if sampler >= samplers.len() {
                return Err(channel_error(
                    *animation_index,
                    channel_index,
                    &format!("references missing sampler {sampler}"),
                ));
            }
            channels.push(RootMotionChannel {
                animation: *animation_index,
                channel: channel_index,
                node,
            });
        }
    }
    Ok(channels)
}

fn unique_channel_nodes(channels: &[RootMotionChannel]) -> Vec<usize> {
    channels
        .iter()
        .map(|channel| channel.node)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn missing_animation_indices(
    animation_indices: &[usize],
    channels: &[RootMotionChannel],
    node: usize,
) -> Vec<usize> {
    animation_indices
        .iter()
        .copied()
        .filter(|animation| {
            !channels.iter().any(|channel| {
                channel.animation == *animation && channel.node == node
            })
        })
        .collect()
}

fn format_indices(indices: &[usize]) -> String {
    indices
        .iter()
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn channel_error(animation: usize, channel: usize, message: &str) -> GlbError {
    GlbError::Invalid(format!(
        "Animation {animation} channel {channel} {message}"
    ))
}

fn inspect_root_motion_channel(
    document: &GlbDocument,
    channel: RootMotionChannel,
) -> Result<RootMotionChannelData, GlbError> {
    let animation = document
        .json
        .get("animations")
        .and_then(Value::as_array)
        .and_then(|animations| animations.get(channel.animation))
        .ok_or_else(|| {
            channel_error(channel.animation, channel.channel, "has disappeared")
        })?;
    let animation_channels = animation
        .get("channels")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            channel_error(
                channel.animation,
                channel.channel,
                "has no channels array",
            )
        })?;
    let json_channel =
        animation_channels.get(channel.channel).ok_or_else(|| {
            channel_error(channel.animation, channel.channel, "does not exist")
        })?;
    let sampler_index = json_channel
        .get("sampler")
        .ok_or_else(|| {
            channel_error(channel.animation, channel.channel, "has no sampler")
        })
        .and_then(|value| {
            read_index(
                value,
                &format!(
                    "Animation {} channel {} sampler",
                    channel.animation, channel.channel
                ),
            )
        })?;
    let samplers = animation
        .get("samplers")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            channel_error(
                channel.animation,
                channel.channel,
                "animation has no samplers array",
            )
        })?;
    let sampler = samplers.get(sampler_index).ok_or_else(|| {
        channel_error(
            channel.animation,
            channel.channel,
            &format!("references missing sampler {sampler_index}"),
        )
    })?;
    let interpolation = sampler
        .get("interpolation")
        .and_then(Value::as_str)
        .unwrap_or("LINEAR");
    match interpolation {
        "LINEAR" | "STEP" => {}
        "CUBICSPLINE" => {
            return Err(GlbError::Unsupported(format!(
                "Root motion removal does not support CUBICSPLINE animation {} channel {}",
                channel.animation, channel.channel
            )))
        }
        other => {
            return Err(GlbError::Unsupported(format!(
                "Root motion removal does not support {other} interpolation in animation {} channel {}",
                channel.animation, channel.channel
            )))
        }
    }

    let input_index = sampler
        .get("input")
        .ok_or_else(|| {
            channel_error(
                channel.animation,
                channel.channel,
                "sampler has no input accessor",
            )
        })
        .and_then(|value| {
            read_index(
                value,
                &format!(
                    "Animation {} channel {} input accessor",
                    channel.animation, channel.channel
                ),
            )
        })?;
    let times = document
        .read_accessor_f32(input_index)
        .map_err(|error| context_error(channel, error))?
        .into_iter()
        .map(|value| value.into_iter().next().unwrap_or_default())
        .collect::<Vec<_>>();
    if times.is_empty() || times.iter().any(|time| !time.is_finite()) {
        return Err(context_error(
            channel,
            GlbError::Invalid(
                "input accessor has no finite keyframe times".to_owned(),
            ),
        ));
    }
    if times.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(context_error(
            channel,
            GlbError::Invalid(
                "input accessor times are not strictly increasing".to_owned(),
            ),
        ));
    }

    let output_index = sampler
        .get("output")
        .ok_or_else(|| {
            channel_error(
                channel.animation,
                channel.channel,
                "sampler has no output accessor",
            )
        })
        .and_then(|value| {
            read_index(
                value,
                &format!(
                    "Animation {} channel {} output accessor",
                    channel.animation, channel.channel
                ),
            )
        })?;
    let accessor = document
        .accessor(output_index)
        .map_err(|error| context_error(channel, error))?
        .clone();
    if accessor.get("componentType").and_then(Value::as_u64) != Some(5126) {
        return Err(context_error(
            channel,
            GlbError::Unsupported(
                "root translation output must use FLOAT components".to_owned(),
            ),
        ));
    }
    if accessor.get("type").and_then(Value::as_str) != Some("VEC3") {
        return Err(context_error(
            channel,
            GlbError::Unsupported(
                "root translation output must use a VEC3 accessor".to_owned(),
            ),
        ));
    }
    let count = accessor
        .get("count")
        .ok_or_else(|| {
            context_error(
                channel,
                GlbError::Invalid("output accessor has no count".to_owned()),
            )
        })
        .and_then(|value| {
            read_index(
                value,
                &format!(
                    "Animation {} channel {} output accessor count",
                    channel.animation, channel.channel
                ),
            )
        })?;
    if count != times.len() {
        return Err(context_error(
            channel,
            GlbError::Invalid(
                "input and root translation output counts differ".to_owned(),
            ),
        ));
    }
    let bytes = document
        .accessor_bytes(&accessor, 12)
        .map_err(|error| context_error(channel, error))?;
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let offset = index * 12;
        let components = (0..3)
            .map(|component| {
                let start = offset + component * 4;
                let raw = bytes.get(start..start + 4).ok_or_else(|| {
                    context_error(
                        channel,
                        GlbError::Invalid(
                            "root translation output is truncated".to_owned(),
                        ),
                    )
                })?;
                let raw: [u8; 4] = raw.try_into().map_err(|_| {
                    context_error(
                        channel,
                        GlbError::Invalid(
                            "root translation output component is truncated"
                                .to_owned(),
                        ),
                    )
                })?;
                Ok(f32::from_le_bytes(raw))
            })
            .collect::<Result<Vec<_>, GlbError>>()?;
        let value = [components[0], components[1], components[2]];
        if value.iter().any(|component| !component.is_finite()) {
            return Err(context_error(
                channel,
                GlbError::Invalid(
                    "root translation output contains non-finite values"
                        .to_owned(),
                ),
            ));
        }
        values.push(value);
    }
    Ok(RootMotionChannelData { values })
}

fn context_error(channel: RootMotionChannel, error: GlbError) -> GlbError {
    let context = format!(
        "Animation {} channel {}",
        channel.animation, channel.channel
    );
    match error {
        GlbError::Io(error) => GlbError::Io(error),
        GlbError::Invalid(message) => {
            GlbError::Invalid(format!("{context}: {message}"))
        }
        GlbError::Unsupported(message) => {
            GlbError::Unsupported(format!("{context}: {message}"))
        }
    }
}

fn replace_sampler_output(
    document: &mut GlbDocument,
    animation_index: usize,
    channel_index: usize,
    output_accessor: usize,
) -> Result<(), GlbError> {
    let animation = document
        .json
        .get("animations")
        .and_then(Value::as_array)
        .and_then(|animations| animations.get(animation_index))
        .ok_or_else(|| {
            channel_error(animation_index, channel_index, "does not exist")
        })?;
    let channels = animation
        .get("channels")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            channel_error(
                animation_index,
                channel_index,
                "has no channels array",
            )
        })?;
    let channel = channels.get(channel_index).ok_or_else(|| {
        channel_error(animation_index, channel_index, "does not exist")
    })?;
    let sampler_index = channel
        .get("sampler")
        .ok_or_else(|| channel_error(animation_index, channel_index, "has no sampler"))
        .and_then(|value| {
            read_index(
                value,
                &format!(
                    "Animation {animation_index} channel {channel_index} sampler"
                ),
            )
        })?;
    let samplers = animation
        .get("samplers")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            channel_error(
                animation_index,
                channel_index,
                "has no samplers array",
            )
        })?;
    if sampler_index >= samplers.len() {
        return Err(channel_error(
            animation_index,
            channel_index,
            &format!("references missing sampler {sampler_index}"),
        ));
    }
    let references = channels
        .iter()
        .filter(|channel| {
            channel.get("sampler").and_then(Value::as_u64)
                == Some(sampler_index as u64)
        })
        .count();
    let animations = document
        .json
        .get_mut("animations")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            GlbError::Invalid("GLB has no animations array".to_owned())
        })?;
    let animation = animations.get_mut(animation_index).ok_or_else(|| {
        channel_error(animation_index, channel_index, "does not exist")
    })?;
    if references > 1 {
        let samplers = animation
            .get_mut("samplers")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                channel_error(
                    animation_index,
                    channel_index,
                    "has no samplers array",
                )
            })?;
        let mut sampler = samplers[sampler_index].clone();
        sampler["output"] = json!(output_accessor);
        let new_sampler_index = samplers.len();
        samplers.push(sampler);
        let channels = animation
            .get_mut("channels")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                channel_error(
                    animation_index,
                    channel_index,
                    "has no channels array",
                )
            })?;
        channels[channel_index]["sampler"] = json!(new_sampler_index);
    } else {
        let samplers = animation
            .get_mut("samplers")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                channel_error(
                    animation_index,
                    channel_index,
                    "has no samplers array",
                )
            })?;
        samplers[sampler_index]["output"] = json!(output_accessor);
    }
    Ok(())
}

#[cfg(test)]
#[path = "root_motion_tests.rs"]
mod tests;
