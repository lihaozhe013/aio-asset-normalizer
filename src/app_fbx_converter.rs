use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::app::App;
use crate::modules::blender::bridge::{self, BlenderError};
use crate::modules::blender::task::{
    normalized_output_path, ConversionTask, ConverterMessage,
};
use crate::modules::glb::GlbDocument;
use crate::modules::logging::{next_task_id, safe_path_label};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConverterStatus {
    Pending,
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone)]
pub(crate) struct ConverterFileState {
    pub(crate) task_id: Option<u64>,
    pub(crate) input: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) status: ConverterStatus,
    pub(crate) error: Option<String>,
}

/// Apply one worker message to the per-file status list. Kept free of `App`
/// so the state transitions are unit-testable without a window context.
fn apply_message(
    results: &mut [ConverterFileState],
    message: &ConverterMessage,
) {
    match message {
        ConverterMessage::FileStarted { task_id, input } => {
            if let Some(state) =
                results.iter_mut().find(|state| &state.input == input)
            {
                state.task_id = Some(*task_id);
                state.status = ConverterStatus::Running;
                state.error = None;
            }
        }
        ConverterMessage::FileFinished {
            task_id,
            input,
            result,
        } => {
            if let Some(state) =
                results.iter_mut().find(|state| &state.input == input)
            {
                state.task_id = Some(*task_id);
                match result {
                    Ok(_) => state.status = ConverterStatus::Success,
                    Err(error) => {
                        state.status = ConverterStatus::Failed;
                        state.error = Some(error.clone());
                    }
                }
            }
        }
        ConverterMessage::Finished => {}
    }
}

impl App {
    pub(crate) fn start_converter_batch(&mut self) {
        if self.converter_busy {
            return;
        }
        let files = self.converter_file_tree.selected_files();
        if files.is_empty() {
            tracing::warn!(
                target: "fbx_converter",
                "Select at least one file to convert"
            );
            return;
        }
        if bridge::find_blender(self.blender_path.as_deref()).is_none() {
            tracing::error!(
                target: "fbx_converter",
                "Blender executable not found; install Blender or set its path on this page"
            );
            return;
        }

        let tasks: Vec<ConversionTask> = files
            .iter()
            .map(|file| ConversionTask {
                task_id: next_task_id(),
                input: file.clone(),
                output: normalized_output_path(file),
                config_json: crate::modules::blender::task::default_config_json(
                ),
                blender_path: self.blender_path.clone(),
            })
            .collect();
        self.converter_results = tasks
            .iter()
            .map(|task| ConverterFileState {
                task_id: Some(task.task_id),
                input: task.input.clone(),
                output: task.output.clone(),
                status: ConverterStatus::Pending,
                error: None,
            })
            .collect();

        let (tx, rx) = mpsc::channel();
        self.converter_rx = Some(rx);
        self.converter_busy = true;
        tracing::info!(
            target: "fbx_converter",
            task_count = tasks.len(),
            "Starting conversion batch"
        );

        std::thread::spawn(move || {
            for task in tasks {
                let input = task.input.clone();
                let output = task.output.clone();
                let _ = tx.send(ConverterMessage::FileStarted {
                    task_id: task.task_id,
                    input: input.clone(),
                });
                tracing::info!(
                    target: "fbx_converter",
                    task_id = task.task_id,
                    input = %safe_path_label(&input),
                    "Conversion file started"
                );
                let result = run_and_validate(&task, &output);
                let _ = tx.send(ConverterMessage::FileFinished {
                    task_id: task.task_id,
                    input,
                    result: result.map(|()| output),
                });
            }
            let _ = tx.send(ConverterMessage::Finished);
        });
    }

    /// Drain converter messages; called from `App::poll_tasks` each frame.
    pub(crate) fn poll_converter(&mut self) {
        let Some(receiver) = self.converter_rx.take() else {
            return;
        };
        let mut batch_finished = false;
        loop {
            let message = match receiver.try_recv() {
                Ok(message) => message,
                // Empty: the batch is still running; put the receiver back.
                Err(mpsc::TryRecvError::Empty) => break,
                // Disconnected without a Finished message means the worker
                // thread died; never leave the page stuck in the busy state.
                Err(mpsc::TryRecvError::Disconnected) => {
                    tracing::warn!(
                        target: "fbx_converter",
                        "Converter worker stopped unexpectedly"
                    );
                    batch_finished = true;
                    break;
                }
            };
            match &message {
                ConverterMessage::Finished => {
                    batch_finished = true;
                    break;
                }
                other => {
                    apply_message(&mut self.converter_results, other);
                    if let ConverterMessage::FileFinished {
                        task_id,
                        input,
                        result,
                    } = other
                    {
                        match result {
                            Ok(output) => tracing::info!(
                                target: "fbx_converter",
                                task_id = *task_id,
                                input = %safe_path_label(input),
                                output = %safe_path_label(output),
                                "File conversion completed"
                            ),
                            Err(error) => tracing::error!(
                                target: "fbx_converter",
                                task_id = *task_id,
                                input = %safe_path_label(input),
                                error = %error,
                                "File conversion failed"
                            ),
                        }
                    }
                }
            }
        }
        if batch_finished {
            self.converter_busy = false;
            self.file_tree.refresh();
            self.bvh_file_tree.refresh();
            self.converter_file_tree.refresh();
            tracing::info!(target: "fbx_converter", "Conversion batch finished");
        } else {
            self.converter_rx = Some(receiver);
        }
    }

    pub(crate) fn converter_blender_status(&self) -> Option<PathBuf> {
        bridge::find_blender(self.blender_path.as_deref())
    }

    pub(crate) fn browse_converter_blender(&mut self) {
        let dialog =
            rfd::FileDialog::new().set_title("Select the Blender executable");
        #[cfg(target_os = "windows")]
        let dialog = dialog.add_filter("Blender", &["exe"]);
        if let Some(path) = dialog.pick_file() {
            self.set_converter_blender_path(Some(
                path.to_string_lossy().into_owned(),
            ));
        }
    }

    pub(crate) fn set_converter_blender_path(&mut self, path: Option<String>) {
        self.blender_path = path.clone().filter(|value| {
            let trimmed = value.trim();
            !trimmed.is_empty()
        });
        if let Some(path) = self.blender_path.as_ref() {
            tracing::info!(
                target: "fbx_converter",
                blender = %safe_path_label(std::path::Path::new(path)),
                "Blender path set"
            );
        } else {
            tracing::info!(
                target: "fbx_converter",
                "Blender path cleared; using auto detection"
            );
        }
        self.needs_save = true;
    }
}

/// Run one Blender task and re-parse the produced GLB through the project
/// reader before the caller reports success (repository data-safety rule).
fn run_and_validate(
    task: &ConversionTask,
    output: &Path,
) -> Result<(), String> {
    bridge::run_task(task).map_err(|error: BlenderError| error.to_string())?;
    GlbDocument::load(output)
        .map_err(|error| {
            format!("conversion produced an unreadable GLB: {error}")
        })
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_results() -> Vec<ConverterFileState> {
        vec![
            ConverterFileState {
                task_id: None,
                input: PathBuf::from("/assets/rig.fbx"),
                output: PathBuf::from("/assets/rig_normalized.glb"),
                status: ConverterStatus::Pending,
                error: None,
            },
            ConverterFileState {
                task_id: None,
                input: PathBuf::from("/assets/prop.obj"),
                output: PathBuf::from("/assets/prop_normalized.glb"),
                status: ConverterStatus::Pending,
                error: None,
            },
        ]
    }

    #[test]
    fn file_messages_update_only_the_matching_entry() {
        let mut results = sample_results();
        apply_message(
            &mut results,
            &ConverterMessage::FileStarted {
                task_id: 1,
                input: PathBuf::from("/assets/rig.fbx"),
            },
        );
        assert_eq!(results[0].status, ConverterStatus::Running);
        assert_eq!(results[0].task_id, Some(1));
        assert_eq!(results[1].status, ConverterStatus::Pending);

        apply_message(
            &mut results,
            &ConverterMessage::FileFinished {
                task_id: 1,
                input: PathBuf::from("/assets/rig.fbx"),
                result: Err("boom".to_owned()),
            },
        );
        assert_eq!(results[0].status, ConverterStatus::Failed);
        assert_eq!(results[0].task_id, Some(1));
        assert_eq!(results[0].error.as_deref(), Some("boom"));

        apply_message(
            &mut results,
            &ConverterMessage::FileFinished {
                task_id: 2,
                input: PathBuf::from("/assets/prop.obj"),
                result: Ok(PathBuf::from("/assets/prop_normalized.glb")),
            },
        );
        assert_eq!(results[1].status, ConverterStatus::Success);
        assert_eq!(results[1].task_id, Some(2));
        assert_eq!(results[0].status, ConverterStatus::Failed);
    }
}
