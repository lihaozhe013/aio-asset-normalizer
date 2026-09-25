use std::path::PathBuf;
use std::sync::mpsc;

use crate::app::App;
use crate::modules::glb::batch_runner::{
    empty_entry, run_export, run_preflight, BatchEntry as GlbBatchEntry,
    BatchProgress, BatchRequest as GlbBatchRequest,
};
use crate::modules::glb::GlbBatchRecipe;
use crate::modules::logging::next_task_id;

pub(crate) use crate::modules::glb::batch_runner::{
    BatchFileStatus as GlbBatchFileStatus, BatchPreflight as GlbBatchPreflight,
};

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
            let mut progress = |event: BatchProgress| {
                if let BatchProgress::PreflightFile { index, entry } = event {
                    let _ = sender.send(GlbBatchMessage::PreflightProgress {
                        generation,
                        index,
                        entry,
                    });
                }
            };
            let (entries, all_valid) =
                run_preflight(&request, task_id, &mut progress);
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
            let mut progress = |event: BatchProgress| match event {
                BatchProgress::ExportStarted { index } => {
                    let _ = sender.send(GlbBatchMessage::ExportStarted {
                        generation,
                        index,
                    });
                }
                BatchProgress::ExportFinished {
                    index,
                    completed,
                    error,
                } => {
                    let _ = sender.send(GlbBatchMessage::ExportFinished {
                        generation,
                        index,
                        completed,
                        error,
                    });
                }
                BatchProgress::PreflightFile { .. }
                | BatchProgress::PreflightFinished { .. } => {}
            };
            let result = run_export(
                &request,
                &preflight.entries,
                task_id,
                &mut progress,
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
