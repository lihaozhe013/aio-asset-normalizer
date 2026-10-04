use std::path::Path;
use std::sync::mpsc;

use crate::app::{App, ExportTaskResult, TaskKind};
use crate::modules::logging::{next_task_id, safe_path_label};

impl App {
    pub(crate) fn export_retarget_agent_prompt(&mut self) {
        if self.task_busy {
            tracing::warn!(
                target: "retarget_agent",
                "Wait for the current background task"
            );
            return;
        }
        let Some(bvh) = self.bvh.clone() else {
            tracing::warn!(target: "retarget_agent", "Open a BVH first");
            return;
        };
        let Some(target_document) = self.bvh_target_glb.clone() else {
            tracing::warn!(
                target: "retarget_agent",
                "Load a target GLB first"
            );
            return;
        };
        let target_skin_index = self.retarget_target_skin_index;
        let source_path = bvh.source_path.clone();
        let target_path = target_document.source_path.clone();
        let source_name = source_path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|value| value.to_str())
            .unwrap_or("source");
        let target_name = target_path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|value| value.to_str())
            .unwrap_or("target");
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Markdown", &["md"])
            .set_file_name(format!(
                "{source_name}-to-{target_name}.aio-retarget-agent.md"
            ))
            .save_file()
        else {
            return;
        };
        if same_path(&path, source_path.as_deref())
            || same_path(&path, target_path.as_deref())
        {
            tracing::error!(
                target: "retarget_agent",
                output = %safe_path_label(&path),
                "Refusing to overwrite a source asset"
            );
            return;
        }
        let source_up_axis = self.bvh_up_axis.clone();
        let source_forward_axis = self.bvh_forward_axis.clone();
        let source_unit = self.bvh_unit.clone();
        let mapping = self.retarget_mapping.clone();
        let mapping_path = self
            .retarget_mapping_path
            .clone()
            .or_else(|| self.mapping_path.clone());
        let (sender, receiver) = mpsc::channel();
        self.task_rx = Some(receiver);
        self.task_busy = true;
        let task_id = next_task_id();
        self.active_task_id = Some(task_id);
        tracing::info!(
            target: "retarget_agent",
            task_id,
            "Building BVH mapping prompt in background"
        );
        let result_path = path.clone();
        let overwrite = self.output_overwrite;
        std::thread::spawn(move || {
            let result = (|| {
                let source = crate::modules::operations::retarget::bvh_source(
                    bvh,
                    [&source_up_axis, &source_forward_axis, &source_unit],
                )
                .map_err(|e| e.to_string())?;
                let fingerprint =
                    crate::modules::operations::retarget::document_fingerprint(
                        &target_document,
                    )
                    .map_err(|e| e.to_string())?;
                let target = crate::modules::operations::retarget::target(
                    target_document,
                    target_skin_index,
                    fingerprint,
                    ["Y", "-Z", "m"],
                )
                .map_err(|e| e.to_string())?;
                let prompt = crate::modules::operations::retarget::prompt(
                    &source,
                    &target,
                    mapping.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                crate::modules::operations::check_output(
                    &result_path,
                    &source_path
                        .as_deref()
                        .into_iter()
                        .chain(target_path.as_deref())
                        .chain(mapping_path.as_deref())
                        .collect::<Vec<_>>(),
                    overwrite,
                )
                .map_err(|e| e.to_string())?;
                crate::modules::atomic_file::write(
                    &result_path,
                    prompt.as_bytes(),
                    overwrite,
                )
                .map_err(|e| e.to_string())
            })();
            let _ = sender.send(ExportTaskResult {
                task_id,
                kind: TaskKind::RetargetAgent,
                paths: vec![result_path],
                details: Vec::new(),
                result,
            });
        });
    }

    pub(crate) fn export_glb_retarget_agent_prompt(&mut self) {
        if self.task_busy {
            tracing::warn!(
                target: "retarget_agent",
                "Wait for the current background task"
            );
            return;
        }
        if self.glb.is_none() {
            tracing::warn!(
                target: "retarget_agent",
                "Open a source GLB first"
            );
            return;
        }
        let Some(target_document) = self.glb_retarget_target.clone() else {
            tracing::warn!(
                target: "retarget_agent",
                "Choose a target GLB first"
            );
            return;
        };
        let Some(source_path) = self.glb_path.clone() else {
            tracing::error!(
                target: "retarget_agent",
                "Source GLB path is unavailable"
            );
            return;
        };
        let Some(target_path) = self.glb_retarget_target_path.clone() else {
            tracing::error!(
                target: "retarget_agent",
                "Target GLB path is unavailable"
            );
            return;
        };
        let source_clip_index = self.glb_animation_index;
        let source_skin_index = self.retarget_source_skin_index;
        let target_skin_index = self.retarget_target_skin_index;
        let source_snapshot = match self.build_glb_retarget_source_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::error!(
                    target: "retarget_agent",
                    error = %error,
                    "Failed to build source snapshot"
                );
                return;
            }
        };
        let source_name = source_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("source")
            .to_owned();
        let target_name = target_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("target")
            .to_owned();
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Markdown", &["md"])
            .set_file_name(format!(
                "{source_name}-to-{target_name}.aio-retarget-agent.md"
            ))
            .save_file()
        else {
            return;
        };
        if same_path(&path, Some(&source_path))
            || same_path(&path, Some(&target_path))
        {
            tracing::error!(
                target: "retarget_agent",
                output = %safe_path_label(&path),
                "Refusing to overwrite a source asset"
            );
            return;
        }
        let mapping = self.retarget_mapping.clone();
        let mapping_path = self
            .retarget_mapping_path
            .clone()
            .or_else(|| self.mapping_path.clone());
        let (sender, receiver) = mpsc::channel();
        self.task_rx = Some(receiver);
        self.task_busy = true;
        let task_id = next_task_id();
        self.active_task_id = Some(task_id);
        tracing::info!(
            target: "retarget_agent",
            task_id,
            "Building GLB mapping prompt in background"
        );
        let result_path = path.clone();
        let overwrite = self.output_overwrite;
        std::thread::spawn(move || {
            let result = (|| {
                let source = crate::modules::operations::retarget::glb_source(
                    &source_snapshot,
                    source_path.parent(),
                    source_clip_index,
                    source_skin_index,
                    &Default::default(),
                    ["Y", "-Z", "m"],
                )
                .map_err(|e| e.to_string())?;
                let fingerprint =
                    crate::modules::operations::retarget::document_fingerprint(
                        &target_document,
                    )
                    .map_err(|e| e.to_string())?;
                let target = crate::modules::operations::retarget::target(
                    target_document,
                    target_skin_index,
                    fingerprint,
                    ["Y", "-Z", "m"],
                )
                .map_err(|e| e.to_string())?;
                let prompt = crate::modules::operations::retarget::prompt(
                    &source,
                    &target,
                    mapping.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                crate::modules::operations::check_output(
                    &result_path,
                    &Some(source_path.as_path())
                        .into_iter()
                        .chain(Some(target_path.as_path()))
                        .chain(mapping_path.as_deref())
                        .collect::<Vec<_>>(),
                    overwrite,
                )
                .map_err(|e| e.to_string())?;
                crate::modules::atomic_file::write(
                    &result_path,
                    prompt.as_bytes(),
                    overwrite,
                )
                .map_err(|e| e.to_string())
            })();
            let _ = sender.send(ExportTaskResult {
                task_id,
                kind: TaskKind::RetargetAgent,
                paths: vec![result_path],
                details: Vec::new(),
                result,
            });
        });
    }
}

fn same_path(path: &Path, source: Option<&Path>) -> bool {
    source.is_some_and(|source| {
        crate::modules::operations::same_path(path, source)
    })
}
