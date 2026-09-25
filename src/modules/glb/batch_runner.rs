//! Headless GLB batch preflight and export runner.
//!
//! The desktop application and the CLI both drive this runner. Progress is
//! reported through a callback so callers can forward it to any channel or
//! render it directly.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::modules::logging::safe_path_label;

use super::{
    AnimationOutputMode, GlbBatchRecipe, GlbDocument, GlbExportPreset,
    GlbExportReport, GlbExportSelection, GlbSummary,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchFileStatus {
    Ready,
    ReadyWithWarnings,
    Error,
    Exporting,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct BatchOutput {
    pub path: PathBuf,
    pub selection: GlbExportSelection,
    pub report: Option<GlbExportReport>,
}

#[derive(Debug, Clone)]
pub struct BatchEntry {
    pub input: PathBuf,
    pub source_sha256: Option<String>,
    pub outputs: Vec<BatchOutput>,
    pub summary: Option<GlbSummary>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    pub status: BatchFileStatus,
    pub completed_outputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRequest {
    pub input_root: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub output_root: PathBuf,
    pub recipe: GlbBatchRecipe,
    pub overwrite_existing: bool,
}

#[derive(Debug, Clone)]
pub struct BatchPreflight {
    pub request: BatchRequest,
    pub entries: Vec<BatchEntry>,
    pub all_valid: bool,
}

#[derive(Debug, Clone)]
pub enum BatchProgress {
    PreflightFile {
        index: usize,
        entry: BatchEntry,
    },
    PreflightFinished {
        entries: Vec<BatchEntry>,
        all_valid: bool,
    },
    ExportStarted {
        index: usize,
    },
    ExportFinished {
        index: usize,
        completed: Vec<PathBuf>,
        error: Option<String>,
    },
}

pub fn empty_entry(input: PathBuf) -> BatchEntry {
    BatchEntry {
        input,
        source_sha256: None,
        outputs: Vec::new(),
        summary: None,
        warnings: Vec::new(),
        error: None,
        status: BatchFileStatus::Error,
        completed_outputs: Vec::new(),
    }
}

/// Load every input, resolve the recipe, and build an in-memory export
/// estimate. Nothing is written to disk.
pub fn run_preflight(
    request: &BatchRequest,
    task_id: u64,
    progress: &mut dyn FnMut(BatchProgress),
) -> (Vec<BatchEntry>, bool) {
    let mut entries = Vec::with_capacity(request.inputs.len());
    for (index, input) in request.inputs.iter().enumerate() {
        let entry = preflight_entry(request, input);
        tracing::info!(
            target: "glb_export",
            task_id,
            input = %safe_path_label(input),
            status = ?entry.status,
            "GLB batch preflight file"
        );
        progress(BatchProgress::PreflightFile {
            index,
            entry: entry.clone(),
        });
        entries.push(entry);
    }
    apply_path_conflicts(request, &mut entries);
    let all_valid = entries.iter().all(|entry| {
        matches!(
            entry.status,
            BatchFileStatus::Ready | BatchFileStatus::ReadyWithWarnings
        )
    });
    (entries, all_valid)
}

/// Export every preflight entry. Sources are re-fingerprinted before the first
/// write, and an unexpected failure stops the remaining files while keeping
/// the paths that were already completed.
pub fn run_export(
    request: &BatchRequest,
    entries: &[BatchEntry],
    task_id: u64,
    progress: &mut dyn FnMut(BatchProgress),
) -> Result<(), String> {
    if entries.iter().any(|entry| {
        !matches!(
            entry.status,
            BatchFileStatus::Ready | BatchFileStatus::ReadyWithWarnings
        )
    }) {
        return Err("Batch preflight contains errors; run it again".to_owned());
    }
    for entry in entries {
        let bytes = fs::read(&entry.input).map_err(|error| {
            format!("{}: cannot re-read source: {error}", entry.input.display())
        })?;
        let actual = sha256_hex(&bytes);
        if entry.source_sha256.as_deref() != Some(actual.as_str()) {
            return Err(format!(
                "{} changed after preflight; run preflight again",
                entry.input.display()
            ));
        }
    }
    let source_paths = request
        .inputs
        .iter()
        .map(|path| path_identity(path))
        .collect::<BTreeSet<_>>();
    for entry in entries {
        for output in &entry.outputs {
            if source_paths.contains(&path_identity(&output.path)) {
                return Err(format!(
                    "Output {} would overwrite a selected source GLB",
                    output.path.display()
                ));
            }
            if let Some(existing) = existing_equivalent_path(&output.path) {
                if existing != output.path || !request.overwrite_existing {
                    return Err(format!(
                        "Output {} appeared after preflight; run preflight again",
                        output.path.display()
                    ));
                }
            }
        }
    }

    for (index, entry) in entries.iter().enumerate() {
        progress(BatchProgress::ExportStarted { index });
        tracing::info!(
            target: "glb_export",
            task_id,
            input = %safe_path_label(&entry.input),
            "GLB batch export file started"
        );
        let mut completed = Vec::new();
        let result = (|| -> Result<(), String> {
            let document = GlbDocument::load(&entry.input)
                .map_err(|error| error.to_string())?;
            for output in &entry.outputs {
                let mut candidate = document.clone();
                candidate.prune_for_export(&output.selection).map_err(
                    |error| format!("{}: {error}", output.path.display()),
                )?;
                candidate.export_atomic(&output.path).map_err(|error| {
                    format!("{}: {error}", output.path.display())
                })?;
                GlbDocument::load(&output.path).map_err(|error| {
                    format!(
                        "{}: generated GLB failed re-parse: {error}",
                        output.path.display()
                    )
                })?;
                completed.push(output.path.clone());
            }
            Ok(())
        })();
        let error = result.as_ref().err().cloned();
        progress(BatchProgress::ExportFinished {
            index,
            completed: completed.clone(),
            error,
        });
        match result {
            Ok(()) => {
                tracing::info!(
                    target: "glb_export",
                    task_id,
                    input = %safe_path_label(&entry.input),
                    output_count = completed.len(),
                    "GLB batch export file completed"
                );
            }
            Err(error) => {
                return Err(error);
            }
        }
    }
    Ok(())
}

fn preflight_entry(request: &BatchRequest, input: &Path) -> BatchEntry {
    let mut entry = empty_entry(input.to_path_buf());
    let bytes = match fs::read(input) {
        Ok(bytes) => bytes,
        Err(error) => {
            entry.error = Some(format!("Cannot read source: {error}"));
            entry.status = BatchFileStatus::Error;
            return entry;
        }
    };
    entry.source_sha256 = Some(sha256_hex(&bytes));
    let document = match GlbDocument::load(input) {
        Ok(document) => document,
        Err(error) => {
            entry.error = Some(error.to_string());
            entry.status = BatchFileStatus::Error;
            return entry;
        }
    };
    entry.summary = Some(document.summary());
    let selection = match request.recipe.resolve(&document) {
        Ok(selection) => selection,
        Err(error) => {
            entry.error = Some(error.to_string());
            entry.status = BatchFileStatus::Error;
            return entry;
        }
    };
    let outputs = match build_outputs(request, &document, &selection, input) {
        Ok(outputs) => outputs,
        Err(error) => {
            entry.error = Some(error);
            entry.status = BatchFileStatus::Error;
            return entry;
        }
    };
    for output in &outputs {
        let validation = document.validate_export_selection(&output.selection);
        if !validation.is_valid() {
            entry.error = Some(validation.errors.join("; "));
            entry.status = BatchFileStatus::Error;
            return entry;
        }
        entry.warnings.extend(validation.warnings);
        match document.preview_export_selection(&output.selection) {
            Ok(report) => {
                entry.outputs.push(BatchOutput {
                    path: output.path.clone(),
                    selection: output.selection.clone(),
                    report: Some(report),
                });
            }
            Err(error) => {
                entry.error = Some(error.to_string());
                entry.status = BatchFileStatus::Error;
                return entry;
            }
        }
    }
    entry.status = if entry.warnings.is_empty() {
        BatchFileStatus::Ready
    } else {
        BatchFileStatus::ReadyWithWarnings
    };
    entry
}

#[derive(Clone)]
struct PlannedOutput {
    path: PathBuf,
    selection: GlbExportSelection,
}

fn build_outputs(
    request: &BatchRequest,
    document: &GlbDocument,
    selection: &GlbExportSelection,
    input: &Path,
) -> Result<Vec<PlannedOutput>, String> {
    let relative_parent = input
        .strip_prefix(&request.input_root)
        .map_err(|_| "Selected input is outside the input root".to_owned())?
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Selected input has no usable file name".to_owned())?;
    let preset_suffix = match selection.preset {
        GlbExportPreset::PreserveAll => "full",
        GlbExportPreset::CharacterPackage => "character",
        GlbExportPreset::SkeletonAnimation => "skeleton",
    };
    let base = format!("{stem}_{preset_suffix}");
    let parent = request.output_root.join(relative_parent);
    if selection.animation_output == AnimationOutputMode::Split {
        if selection.selected_animations.is_empty() {
            return Err(
                "Split animation output requires at least one animation"
                    .to_owned(),
            );
        }
        let names = document.animation_names();
        let mut used = BTreeSet::new();
        let mut outputs = Vec::new();
        for animation_index in &selection.selected_animations {
            let name = names
                .get(*animation_index)
                .cloned()
                .unwrap_or_else(|| format!("animation-{animation_index}"));
            let cleaned = clean_filename_component(&name);
            let mut suffix = cleaned.clone();
            let mut count = 1;
            while !used.insert(suffix.to_ascii_lowercase()) {
                count += 1;
                suffix = format!("{cleaned}-{count}");
            }
            let mut split_selection = selection.clone();
            split_selection.selected_animations =
                BTreeSet::from([*animation_index]);
            split_selection.animation_output = AnimationOutputMode::Combined;
            outputs.push(PlannedOutput {
                path: parent.join(format!("{base}--{suffix}.glb")),
                selection: split_selection,
            });
        }
        Ok(outputs)
    } else {
        Ok(vec![PlannedOutput {
            path: parent.join(format!("{base}.glb")),
            selection: selection.clone(),
        }])
    }
}

fn apply_path_conflicts(request: &BatchRequest, entries: &mut [BatchEntry]) {
    let mut output_to_entries: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        for output in &entry.outputs {
            output_to_entries
                .entry(path_identity(&output.path))
                .or_default()
                .push(index);
        }
    }
    let source_paths = request
        .inputs
        .iter()
        .map(|path| path_identity(path))
        .collect::<BTreeSet<_>>();
    for entry in entries.iter_mut() {
        let mut errors = Vec::new();
        for output in &entry.outputs {
            let identity = path_identity(&output.path);
            if source_paths.contains(&identity) {
                errors.push(format!(
                    "Output {} would overwrite a selected source GLB",
                    output.path.display()
                ));
            }
            if output_to_entries
                .get(&identity)
                .is_some_and(|indices| indices.len() > 1)
            {
                errors.push(format!(
                    "Output path collision at {}",
                    output.path.display()
                ));
            }
            if let Some(existing) = existing_equivalent_path(&output.path) {
                if existing != output.path {
                    errors.push(format!(
                        "Output path differs only by case from existing file: {}",
                        existing.display()
                    ));
                } else if request.overwrite_existing {
                    entry.warnings.push(format!(
                        "Existing output will be replaced: {}",
                        output.path.display()
                    ));
                } else {
                    errors.push(format!(
                        "Output already exists: {}",
                        output.path.display()
                    ));
                }
            }
        }
        if errors.is_empty() {
            if !entry.warnings.is_empty()
                && entry.status == BatchFileStatus::Ready
            {
                entry.status = BatchFileStatus::ReadyWithWarnings;
            }
            continue;
        }
        entry.error = Some(errors.join("; "));
        entry.status = BatchFileStatus::Error;
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn path_identity(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let normalized = fs::canonicalize(&absolute).unwrap_or(absolute);
    normalized.to_string_lossy().to_ascii_lowercase()
}

fn existing_equivalent_path(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.to_path_buf());
    }
    let parent = path.parent()?;
    let identity = path_identity(path);
    fs::read_dir(parent)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|candidate| path_identity(candidate) == identity)
}

/// Sanitize one authored name into a portable file-name component.
pub fn clean_filename_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed =
        cleaned.trim_matches(|character| character == ' ' || character == '.');
    if trimmed.is_empty() {
        "animation".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_names_include_preset_and_animation() {
        assert_eq!(clean_filename_component("Walk/Run"), "Walk_Run");
        assert_eq!(clean_filename_component("..."), "animation");
    }

    #[test]
    fn path_identity_is_conservative_about_case() {
        assert_eq!(
            path_identity(Path::new("BatchOutput.glb")),
            path_identity(Path::new("batchoutput.glb")),
        );
    }

    #[test]
    fn path_conflicts_block_existing_outputs_without_overwrite() {
        let root = std::env::temp_dir().join(format!(
            "aio-asset-normalizer-batch-conflict-{}",
            std::process::id()
        ));
        let input = root.join("source.glb");
        let output = root.join("nested/source_skeleton.glb");
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        fs::write(&output, b"existing").unwrap();
        let request = BatchRequest {
            input_root: root.clone(),
            inputs: vec![input.clone()],
            output_root: root.clone(),
            recipe: GlbBatchRecipe {
                preset: GlbExportPreset::SkeletonAnimation,
                ..Default::default()
            },
            overwrite_existing: false,
        };
        let mut entries = vec![BatchEntry {
            input,
            source_sha256: None,
            outputs: vec![BatchOutput {
                path: output.clone(),
                selection: GlbExportSelection::default(),
                report: None,
            }],
            summary: None,
            warnings: Vec::new(),
            error: None,
            status: BatchFileStatus::Ready,
            completed_outputs: Vec::new(),
        }];
        apply_path_conflicts(&request, &mut entries);
        assert_eq!(entries[0].status, BatchFileStatus::Error);
        assert!(entries[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("already exists")));
        let _ = fs::remove_dir_all(root);
    }
}
