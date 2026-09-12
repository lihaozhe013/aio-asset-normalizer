use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use sha2::{Digest, Sha256};

use crate::app::App;
use crate::modules::glb::{
    AnimationOutputMode, GlbBatchRecipe, GlbDocument, GlbExportPreset,
    GlbExportReport, GlbExportSelection,
};
use crate::modules::logging::{next_task_id, safe_path_label};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlbInspectorScope {
    Current,
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlbSingleTab {
    Inspect,
    Retarget,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlbBatchFileStatus {
    Ready,
    ReadyWithWarnings,
    Error,
    Exporting,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub(crate) struct GlbBatchOutput {
    pub(crate) path: PathBuf,
    pub(crate) selection: GlbExportSelection,
    pub(crate) report: Option<GlbExportReport>,
}

#[derive(Debug, Clone)]
pub(crate) struct GlbBatchEntry {
    pub(crate) input: PathBuf,
    pub(crate) source_sha256: Option<String>,
    pub(crate) outputs: Vec<GlbBatchOutput>,
    pub(crate) summary: Option<crate::modules::glb::GlbSummary>,
    pub(crate) warnings: Vec<String>,
    pub(crate) error: Option<String>,
    pub(crate) status: GlbBatchFileStatus,
    pub(crate) completed_outputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlbBatchRequest {
    pub(crate) input_root: PathBuf,
    pub(crate) inputs: Vec<PathBuf>,
    pub(crate) output_root: PathBuf,
    pub(crate) recipe: GlbBatchRecipe,
    pub(crate) overwrite_existing: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct GlbBatchPreflight {
    pub(crate) request: GlbBatchRequest,
    pub(crate) entries: Vec<GlbBatchEntry>,
    pub(crate) all_valid: bool,
}

pub(crate) struct GlbBatchState {
    pub(crate) scope: GlbInspectorScope,
    pub(crate) single_tab: GlbSingleTab,
    pub(crate) recipe: GlbBatchRecipe,
    pub(crate) output_root: Option<PathBuf>,
    pub(crate) overwrite_existing: bool,
    pub(crate) preflight: Option<GlbBatchPreflight>,
    pub(crate) task_request: Option<GlbBatchRequest>,
    pub(crate) task_generation: u64,
    pub(crate) observed_selection_count: usize,
    observed_selection_paths: Vec<PathBuf>,
    pub(crate) last_result: Option<String>,
    rx: Option<mpsc::Receiver<GlbBatchMessage>>,
}

impl Default for GlbBatchState {
    fn default() -> Self {
        Self {
            scope: GlbInspectorScope::Current,
            single_tab: GlbSingleTab::Inspect,
            recipe: GlbBatchRecipe::default(),
            output_root: None,
            overwrite_existing: false,
            preflight: None,
            task_request: None,
            task_generation: 0,
            observed_selection_count: 0,
            observed_selection_paths: Vec::new(),
            last_result: None,
            rx: None,
        }
    }
}

impl GlbBatchState {
    pub(crate) fn is_busy(&self) -> bool {
        self.rx.is_some()
    }
}

enum GlbBatchMessage {
    PreflightProgress {
        generation: u64,
        index: usize,
        entry: GlbBatchEntry,
    },
    PreflightFinished {
        generation: u64,
        entries: Vec<GlbBatchEntry>,
        all_valid: bool,
    },
    ExportStarted {
        generation: u64,
        index: usize,
    },
    ExportFinished {
        generation: u64,
        index: usize,
        completed: Vec<PathBuf>,
        error: Option<String>,
    },
    Finished {
        generation: u64,
        result: Result<(), String>,
    },
}

impl App {
    pub(crate) fn export_glb_for_scope(&mut self) {
        if self.glb_batch.scope == GlbInspectorScope::Batch {
            self.start_glb_batch_export();
        } else {
            self.export_glb();
        }
    }

    pub(crate) fn observe_glb_selection(&mut self, selected: &[PathBuf]) {
        let count = selected.len();
        if count >= 2
            && self.glb_batch.observed_selection_count < 2
            && self.glb_batch.scope == GlbInspectorScope::Current
        {
            self.glb_batch.scope = GlbInspectorScope::Batch;
        }
        if count == 0 && self.glb_batch.scope == GlbInspectorScope::Batch {
            self.glb_batch.scope = GlbInspectorScope::Current;
        }
        let selection_changed =
            self.glb_batch.observed_selection_paths.as_slice() != selected;
        if selection_changed
            || self.glb_batch.preflight.as_ref().is_some_and(|preflight| {
                preflight.request.inputs.as_slice() != selected
            })
        {
            self.invalidate_glb_batch_preflight();
        }
        self.glb_batch.observed_selection_paths = selected.to_vec();
        self.glb_batch.observed_selection_count = count;
    }

    pub(crate) fn current_glb_batch_request(
        &self,
    ) -> Result<GlbBatchRequest, String> {
        let input_root = self.file_tree.root().cloned().ok_or_else(|| {
            "Open an input folder before batching GLBs".to_owned()
        })?;
        let inputs = self.file_tree.selected_files();
        if inputs.is_empty() {
            return Err("Select at least one GLB for the batch".to_owned());
        }
        let output_root = self
            .glb_batch
            .output_root
            .clone()
            .ok_or_else(|| "Choose an output directory first".to_owned())?;
        if !output_root.is_dir() {
            return Err("The batch output directory is unavailable".to_owned());
        }
        Ok(GlbBatchRequest {
            input_root,
            inputs,
            output_root,
            recipe: self.glb_batch.recipe.clone(),
            overwrite_existing: self.glb_batch.overwrite_existing,
        })
    }

    pub(crate) fn batch_preflight_is_current(&self) -> bool {
        let Ok(request) = self.current_glb_batch_request() else {
            return false;
        };
        self.glb_batch.preflight.as_ref().is_some_and(|preflight| {
            preflight.request == request && preflight.all_valid
        })
    }

    pub(crate) fn invalidate_glb_batch_preflight(&mut self) {
        self.glb_batch.task_generation =
            self.glb_batch.task_generation.wrapping_add(1);
        self.glb_batch.preflight = None;
        self.glb_batch.last_result = None;
    }

    pub(crate) fn choose_glb_batch_output_root(&mut self) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        if self.glb_batch.output_root.as_ref() != Some(&path) {
            self.glb_batch.output_root = Some(path);
            self.invalidate_glb_batch_preflight();
        }
    }

    pub(crate) fn start_glb_batch_preflight(&mut self) {
        if self.task_busy || self.glb_batch.is_busy() {
            return;
        }
        let request = match self.current_glb_batch_request() {
            Ok(request) => request,
            Err(error) => {
                self.glb_batch.last_result = Some(error.clone());
                tracing::warn!(target: "glb_export", error = %error, "Batch preflight unavailable");
                return;
            }
        };
        self.glb_batch.task_generation =
            self.glb_batch.task_generation.wrapping_add(1);
        let generation = self.glb_batch.task_generation;
        let task_id = next_task_id();
        let (sender, receiver) = mpsc::channel();
        self.glb_batch.rx = Some(receiver);
        self.glb_batch.task_request = Some(request.clone());
        self.glb_batch.preflight = None;
        self.glb_batch.last_result = None;
        self.task_busy = true;
        self.active_task_id = Some(task_id);
        tracing::info!(
            target: "glb_export",
            task_id,
            file_count = request.inputs.len(),
            "Starting GLB batch preflight"
        );
        std::thread::spawn(move || {
            let (entries, all_valid) =
                run_preflight(&request, task_id, generation, &sender);
            let _ = sender.send(GlbBatchMessage::PreflightFinished {
                generation,
                entries,
                all_valid,
            });
        });
    }

    pub(crate) fn start_glb_batch_export(&mut self) {
        if self.task_busy || self.glb_batch.is_busy() {
            return;
        }
        let Ok(request) = self.current_glb_batch_request() else {
            self.glb_batch.last_result =
                Some("Run a fresh batch preflight first".to_owned());
            return;
        };
        let Some(preflight) = self.glb_batch.preflight.clone() else {
            self.glb_batch.last_result =
                Some("Run a fresh batch preflight first".to_owned());
            return;
        };
        if preflight.request != request || !preflight.all_valid {
            self.glb_batch.last_result =
                Some("Run a fresh successful batch preflight first".to_owned());
            return;
        }
        self.glb_batch.task_generation =
            self.glb_batch.task_generation.wrapping_add(1);
        let generation = self.glb_batch.task_generation;
        let task_id = next_task_id();
        let (sender, receiver) = mpsc::channel();
        self.glb_batch.rx = Some(receiver);
        self.glb_batch.task_request = Some(request.clone());
        self.glb_batch.last_result = None;
        self.task_busy = true;
        self.active_task_id = Some(task_id);
        tracing::info!(
            target: "glb_export",
            task_id,
            file_count = request.inputs.len(),
            "Starting GLB batch export"
        );
        std::thread::spawn(move || {
            let result = run_export(
                &request,
                &preflight.entries,
                task_id,
                generation,
                &sender,
            );
            let _ =
                sender.send(GlbBatchMessage::Finished { generation, result });
        });
    }

    pub(crate) fn poll_glb_batch(&mut self) {
        let Some(receiver) = self.glb_batch.rx.take() else {
            return;
        };
        let mut finished = false;
        loop {
            match receiver.try_recv() {
                Ok(message) => match message {
                    GlbBatchMessage::PreflightProgress {
                        generation,
                        index,
                        entry,
                    } if generation == self.glb_batch.task_generation => {
                        if let Some(preflight) =
                            self.glb_batch.preflight.as_mut()
                        {
                            if index >= preflight.entries.len() {
                                preflight
                                    .entries
                                    .resize_with(index + 1, || {
                                        empty_entry(PathBuf::new())
                                    });
                            }
                            preflight.entries[index] = entry;
                        } else {
                            let request = self
                                .glb_batch
                                .task_request
                                .clone()
                                .unwrap_or_else(|| GlbBatchRequest {
                                    input_root: PathBuf::new(),
                                    inputs: Vec::new(),
                                    output_root: PathBuf::new(),
                                    recipe: GlbBatchRecipe::default(),
                                    overwrite_existing: false,
                                });
                            let mut entries = Vec::new();
                            entries.resize_with(index + 1, || {
                                empty_entry(PathBuf::new())
                            });
                            entries[index] = entry;
                            self.glb_batch.preflight =
                                Some(GlbBatchPreflight {
                                    request,
                                    entries,
                                    all_valid: false,
                                });
                        }
                    }
                    GlbBatchMessage::PreflightFinished {
                        generation,
                        entries,
                        all_valid,
                    } if generation == self.glb_batch.task_generation => {
                        if let Some(request) =
                            self.glb_batch.task_request.take()
                        {
                            self.glb_batch.preflight =
                                Some(GlbBatchPreflight {
                                    request,
                                    entries,
                                    all_valid,
                                });
                        }
                        self.glb_batch.last_result = Some(if all_valid {
                            "Batch preflight passed".to_owned()
                        } else {
                            "Batch preflight found errors".to_owned()
                        });
                        finished = true;
                    }
                    GlbBatchMessage::ExportStarted { generation, index }
                        if generation == self.glb_batch.task_generation =>
                    {
                        if let Some(preflight) =
                            self.glb_batch.preflight.as_mut()
                        {
                            if let Some(entry) =
                                preflight.entries.get_mut(index)
                            {
                                entry.status = GlbBatchFileStatus::Exporting;
                            }
                        }
                    }
                    GlbBatchMessage::ExportFinished {
                        generation,
                        index,
                        completed,
                        error,
                    } if generation == self.glb_batch.task_generation => {
                        if let Some(preflight) =
                            self.glb_batch.preflight.as_mut()
                        {
                            if let Some(entry) =
                                preflight.entries.get_mut(index)
                            {
                                entry.completed_outputs = completed;
                                match error {
                                    None => {
                                        entry.status =
                                            GlbBatchFileStatus::Succeeded;
                                        entry.error = None;
                                    }
                                    Some(error) => {
                                        entry.status =
                                            GlbBatchFileStatus::Failed;
                                        entry.error = Some(error);
                                    }
                                }
                            }
                        }
                    }
                    GlbBatchMessage::Finished { generation, result }
                        if generation == self.glb_batch.task_generation =>
                    {
                        self.glb_batch.last_result = Some(match &result {
                            Ok(()) => "Batch export completed".to_owned(),
                            Err(error) => {
                                format!("Batch export stopped: {error}")
                            }
                        });
                        if let Err(error) = &result {
                            if let Some(preflight) =
                                self.glb_batch.preflight.as_mut()
                            {
                                for entry in &mut preflight.entries {
                                    if entry.status == GlbBatchFileStatus::Ready
                                        || entry.status
                                            == GlbBatchFileStatus::ReadyWithWarnings
                                    {
                                        entry.status = GlbBatchFileStatus::Skipped;
                                    }
                                }
                            }
                            tracing::error!(target: "glb_export", error = %error, "GLB batch export failed");
                        }
                        if self.glb_batch.preflight.is_some() {
                            // Output existence may have changed even when the
                            // worker completed successfully; require a fresh
                            // preflight before another run.
                            if let Some(preflight) =
                                self.glb_batch.preflight.as_mut()
                            {
                                preflight.all_valid = false;
                            }
                        }
                        finished = true;
                    }
                    _ => {}
                },
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.glb_batch.last_result = Some(
                        "GLB batch worker stopped unexpectedly".to_owned(),
                    );
                    self.glb_batch.task_request = None;
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.task_busy = false;
            self.active_task_id = None;
            self.glb_batch.task_request = None;
        } else {
            self.glb_batch.rx = Some(receiver);
        }
    }
}

fn run_preflight(
    request: &GlbBatchRequest,
    task_id: u64,
    generation: u64,
    sender: &mpsc::Sender<GlbBatchMessage>,
) -> (Vec<GlbBatchEntry>, bool) {
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
        let _ = sender.send(GlbBatchMessage::PreflightProgress {
            generation,
            index,
            entry: entry.clone(),
        });
        entries.push(entry);
    }
    apply_path_conflicts(request, &mut entries);
    let all_valid = entries.iter().all(|entry| {
        matches!(
            entry.status,
            GlbBatchFileStatus::Ready | GlbBatchFileStatus::ReadyWithWarnings
        )
    });
    (entries, all_valid)
}

fn preflight_entry(request: &GlbBatchRequest, input: &Path) -> GlbBatchEntry {
    let mut entry = empty_entry(input.to_path_buf());
    let bytes = match fs::read(input) {
        Ok(bytes) => bytes,
        Err(error) => {
            entry.error = Some(format!("Cannot read source: {error}"));
            entry.status = GlbBatchFileStatus::Error;
            return entry;
        }
    };
    entry.source_sha256 = Some(sha256_hex(&bytes));
    let document = match GlbDocument::load(input) {
        Ok(document) => document,
        Err(error) => {
            entry.error = Some(error.to_string());
            entry.status = GlbBatchFileStatus::Error;
            return entry;
        }
    };
    entry.summary = Some(document.summary());
    let selection = match request.recipe.resolve(&document) {
        Ok(selection) => selection,
        Err(error) => {
            entry.error = Some(error.to_string());
            entry.status = GlbBatchFileStatus::Error;
            return entry;
        }
    };
    let outputs = match build_outputs(request, &document, &selection, input) {
        Ok(outputs) => outputs,
        Err(error) => {
            entry.error = Some(error);
            entry.status = GlbBatchFileStatus::Error;
            return entry;
        }
    };
    for output in &outputs {
        let validation = document.validate_export_selection(&output.selection);
        if !validation.is_valid() {
            entry.error = Some(validation.errors.join("; "));
            entry.status = GlbBatchFileStatus::Error;
            return entry;
        }
        entry.warnings.extend(validation.warnings);
        match document.preview_export_selection(&output.selection) {
            Ok(report) => {
                entry.outputs.push(GlbBatchOutput {
                    path: output.path.clone(),
                    selection: output.selection.clone(),
                    report: Some(report),
                });
            }
            Err(error) => {
                entry.error = Some(error.to_string());
                entry.status = GlbBatchFileStatus::Error;
                return entry;
            }
        }
    }
    entry.status = if entry.warnings.is_empty() {
        GlbBatchFileStatus::Ready
    } else {
        GlbBatchFileStatus::ReadyWithWarnings
    };
    entry
}

#[derive(Clone)]
struct PlannedOutput {
    path: PathBuf,
    selection: GlbExportSelection,
}

fn build_outputs(
    request: &GlbBatchRequest,
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

fn apply_path_conflicts(
    request: &GlbBatchRequest,
    entries: &mut [GlbBatchEntry],
) {
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
                && entry.status == GlbBatchFileStatus::Ready
            {
                entry.status = GlbBatchFileStatus::ReadyWithWarnings;
            }
            continue;
        }
        entry.error = Some(errors.join("; "));
        entry.status = GlbBatchFileStatus::Error;
    }
}

fn run_export(
    request: &GlbBatchRequest,
    entries: &[GlbBatchEntry],
    task_id: u64,
    generation: u64,
    sender: &mpsc::Sender<GlbBatchMessage>,
) -> Result<(), String> {
    if entries.iter().any(|entry| {
        !matches!(
            entry.status,
            GlbBatchFileStatus::Ready | GlbBatchFileStatus::ReadyWithWarnings
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
        let _ =
            sender.send(GlbBatchMessage::ExportStarted { generation, index });
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
        let _ = sender.send(GlbBatchMessage::ExportFinished {
            generation,
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

fn empty_entry(input: PathBuf) -> GlbBatchEntry {
    GlbBatchEntry {
        input,
        source_sha256: None,
        outputs: Vec::new(),
        summary: None,
        warnings: Vec::new(),
        error: None,
        status: GlbBatchFileStatus::Error,
        completed_outputs: Vec::new(),
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

fn clean_filename_component(value: &str) -> String {
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
        let request = GlbBatchRequest {
            input_root: root.clone(),
            inputs: vec![input.clone()],
            output_root: root.clone(),
            recipe: GlbBatchRecipe {
                preset: GlbExportPreset::SkeletonAnimation,
                ..Default::default()
            },
            overwrite_existing: false,
        };
        let mut entries = vec![GlbBatchEntry {
            input,
            source_sha256: None,
            outputs: vec![GlbBatchOutput {
                path: output.clone(),
                selection: GlbExportSelection::default(),
                report: None,
            }],
            summary: None,
            warnings: Vec::new(),
            error: None,
            status: GlbBatchFileStatus::Ready,
            completed_outputs: Vec::new(),
        }];
        apply_path_conflicts(&request, &mut entries);
        assert_eq!(entries[0].status, GlbBatchFileStatus::Error);
        assert!(entries[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("already exists")));
        let _ = fs::remove_dir_all(root);
    }
}
