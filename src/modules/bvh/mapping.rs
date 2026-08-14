use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use super::{BvhDocument, BvhError, MappingFile};
use crate::modules::glb::SkinData;

pub(super) fn unit_scale(unit: &str) -> Result<f32, BvhError> {
    match unit.to_ascii_lowercase().as_str() {
        "m" | "meter" | "meters" => Ok(1.0),
        "cm" | "centimeter" | "centimeters" => Ok(0.01),
        "mm" | "millimeter" | "millimeters" => Ok(0.001),
        other => Err(BvhError::Mapping(format!(
            "unsupported source unit '{other}'"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingSuggestion {
    pub source_joint: String,
    pub target_node: String,
    pub confidence: SuggestionConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionConfidence {
    Exact,
    Normalized,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingValidation {
    pub contract_error: Option<String>,
    pub target_skin_matches: bool,
    pub source_root_found: bool,
    pub target_root_found: bool,
    pub mapped_count: usize,
    pub unmapped_source_joints: Vec<String>,
    pub unknown_source_joints: Vec<String>,
    pub unknown_target_nodes: Vec<String>,
    pub duplicate_source_joints: Vec<String>,
    pub duplicate_target_nodes: Vec<String>,
}

impl MappingValidation {
    pub fn is_valid(&self) -> bool {
        self.contract_error.is_none()
            && self.target_skin_matches
            && self.source_root_found
            && self.target_root_found
            && self.mapped_count > 0
            && self.unknown_source_joints.is_empty()
            && self.unknown_target_nodes.is_empty()
            && self.duplicate_source_joints.is_empty()
            && self.duplicate_target_nodes.is_empty()
    }

    pub fn coverage_percent(&self, source_joint_count: usize) -> f32 {
        if source_joint_count == 0 {
            0.0
        } else {
            self.mapped_count as f32 * 100.0 / source_joint_count as f32
        }
    }
}

impl MappingFile {
    pub fn validate_contract(&self) -> Result<(), BvhError> {
        if self.schema_version != 1 {
            return Err(BvhError::Mapping(format!(
                "unsupported mapping schema version {}",
                self.schema_version
            )));
        }
        if self.source.up_axis.trim().is_empty()
            || self.source.forward_axis.trim().is_empty()
            || self.source.unit.trim().is_empty()
            || self.source.root.trim().is_empty()
        {
            return Err(BvhError::Mapping(
                "Mapping source axes, unit, and root are required".to_owned(),
            ));
        }
        if self.target.skin.trim().is_empty()
            || self.target.root.trim().is_empty()
        {
            return Err(BvhError::Mapping(
                "Mapping target Skin and root are required".to_owned(),
            ));
        }
        if self.bones.is_empty() {
            return Err(BvhError::Mapping(
                "Mapping contains no bones".to_owned(),
            ));
        }
        super::CoordinateBasis::from_mapping(
            &self.source.up_axis,
            &self.source.forward_axis,
        )?;
        unit_scale(&self.source.unit)?;
        for bone in &self.bones {
            if bone.source_joint.trim().is_empty()
                || bone.target_node.trim().is_empty()
            {
                return Err(BvhError::Mapping(
                    "Mapping bone source and target names are required"
                        .to_owned(),
                ));
            }
            if bone
                .rotation_offset_xyzw
                .iter()
                .any(|value| !value.is_finite())
            {
                return Err(BvhError::Mapping(format!(
                    "Mapping rotation offset for '{}' is not finite",
                    bone.source_joint
                )));
            }
        }
        Ok(())
    }
}

impl BvhDocument {
    pub fn mapping_report(
        &self,
        mapping: &MappingFile,
        target: &SkinData,
    ) -> MappingValidation {
        let contract_error = mapping
            .validate_contract()
            .err()
            .map(|error| error.to_string());
        let source_lookup: HashMap<&str, usize> = self
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| (joint.name.as_str(), index))
            .collect();
        let target_lookup: HashMap<&str, usize> = target
            .nodes
            .iter()
            .filter(|node| target.joints.contains(&node.index))
            .map(|node| (node.name.as_str(), node.index))
            .collect();
        let mut mapped_sources = HashSet::new();
        let mut source_assignments = HashSet::new();
        let mut target_assignments = HashSet::new();
        let mut unknown_source_joints = Vec::new();
        let mut unknown_target_nodes = Vec::new();
        let mut duplicate_source_joints = Vec::new();
        let mut duplicate_target_nodes = Vec::new();
        for bone in &mapping.bones {
            let source_exists =
                source_lookup.contains_key(bone.source_joint.as_str());
            let target_exists =
                target_lookup.contains_key(bone.target_node.as_str());
            if !source_exists {
                push_unique(&mut unknown_source_joints, &bone.source_joint);
            } else if !source_assignments.insert(bone.source_joint.clone()) {
                push_unique(&mut duplicate_source_joints, &bone.source_joint);
            }
            if !target_exists {
                push_unique(&mut unknown_target_nodes, &bone.target_node);
            } else if !target_assignments.insert(bone.target_node.clone()) {
                push_unique(&mut duplicate_target_nodes, &bone.target_node);
            }
            if source_exists
                && target_exists
                && !duplicate_source_joints.contains(&bone.source_joint)
                && !duplicate_target_nodes.contains(&bone.target_node)
            {
                mapped_sources.insert(bone.source_joint.clone());
            }
        }
        let unmapped_source_joints = self
            .joints
            .iter()
            .filter(|joint| !mapped_sources.contains(&joint.name))
            .map(|joint| joint.name.clone())
            .collect::<Vec<_>>();
        MappingValidation {
            contract_error,
            target_skin_matches: target.name == mapping.target.skin,
            source_root_found: source_lookup
                .contains_key(mapping.source.root.as_str()),
            target_root_found: target_lookup
                .contains_key(mapping.target.root.as_str()),
            mapped_count: mapped_sources.len(),
            unmapped_source_joints,
            unknown_source_joints,
            unknown_target_nodes,
            duplicate_source_joints,
            duplicate_target_nodes,
        }
    }

    pub fn suggest_mapping(&self, target: &SkinData) -> Vec<MappingSuggestion> {
        let targets = target
            .nodes
            .iter()
            .filter(|node| target.joints.contains(&node.index))
            .collect::<Vec<_>>();
        let mut suggestions = Vec::new();
        for source in &self.joints {
            let exact = targets
                .iter()
                .filter(|target| target.name.eq_ignore_ascii_case(&source.name))
                .collect::<Vec<_>>();
            if exact.len() == 1 {
                suggestions.push(MappingSuggestion {
                    source_joint: source.name.clone(),
                    target_node: exact[0].name.clone(),
                    confidence: SuggestionConfidence::Exact,
                });
                continue;
            }
            let normalized = normalize_name(&source.name);
            let matches = targets
                .iter()
                .filter(|target| normalize_name(&target.name) == normalized)
                .collect::<Vec<_>>();
            if normalized.is_empty() || matches.len() != 1 {
                continue;
            }
            suggestions.push(MappingSuggestion {
                source_joint: source.name.clone(),
                target_node: matches[0].name.clone(),
                confidence: SuggestionConfidence::Normalized,
            });
        }
        suggestions
    }
}

pub fn save_mapping(
    path: &Path,
    mapping: &MappingFile,
) -> Result<(), BvhError> {
    mapping.validate_contract()?;
    let bytes = serde_json::to_vec_pretty(mapping).map_err(|error| {
        BvhError::Mapping(format!("serialize mapping: {error}"))
    })?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

pub fn load_mapping(path: &Path) -> Result<MappingFile, BvhError> {
    let value = fs::read_to_string(path)?;
    let mapping: MappingFile = serde_json::from_str(&value)
        .map_err(|error| BvhError::Mapping(error.to_string()))?;
    mapping.validate_contract()?;
    Ok(mapping)
}

fn normalize_name(name: &str) -> String {
    let mut normalized = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    for prefix in ["mixamorig", "armature", "skeleton"] {
        if let Some(stripped) = normalized.strip_prefix(prefix) {
            normalized = stripped.to_owned();
        }
    }
    for suffix in ["joint", "jnt", "bone"] {
        if let Some(stripped) = normalized.strip_suffix(suffix) {
            normalized = stripped.to_owned();
        }
    }
    normalized
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_owned());
    }
}
