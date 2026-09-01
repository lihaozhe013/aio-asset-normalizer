use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::mpsc;

use crate::app::{App, ExportTaskResult};
use crate::modules::glb::AnimationRuntime;
use crate::modules::retarget::{self, SkeletonDescriptor, SourceKind};

impl App {
    pub(crate) fn export_retarget_agent_prompt(&mut self) {
        if self.task_busy {
            self.log.append(
                "[retarget_agent] Wait for the current background task",
            );
            return;
        }
        let Some(bvh) = self.bvh.clone() else {
            self.log.append("[retarget_agent] Open a BVH first");
            return;
        };
        let Some(target_document) = self.bvh_target_glb.clone() else {
            self.log.append("[retarget_agent] Load a target GLB first");
            return;
        };
        let target_skin_index = self.retarget_target_skin_index;
        let target_skin = match target_document.skin_data_at(target_skin_index)
        {
            Ok(skin) => skin,
            Err(error) => {
                self.log.append(&format!("[retarget_agent] {error}"));
                return;
            }
        };
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
            self.log.append(
                "[retarget_agent] Refusing to overwrite a source asset",
            );
            return;
        }
        let source_up_axis = self.bvh_up_axis.clone();
        let source_forward_axis = self.bvh_forward_axis.clone();
        let source_unit = self.bvh_unit.clone();
        let mapping = self.retarget_mapping.clone();
        let (sender, receiver) = mpsc::channel();
        self.task_rx = Some(receiver);
        self.task_busy = true;
        self.log.append(
            "[retarget_agent] Building BVH mapping prompt in background",
        );
        let result_path = path.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let source = SkeletonDescriptor::from_bvh(
                    &bvh,
                    file_hash(source_path.as_deref()),
                    source_up_axis,
                    source_forward_axis,
                    source_unit,
                )
                .map_err(|error| error.to_string())?;
                let target = SkeletonDescriptor::from_skin(
                    &target_skin,
                    SourceKind::Glb,
                    file_hash(target_path.as_deref()),
                    String::new(),
                    "Y",
                    "-Z",
                    "m",
                    &HashSet::new(),
                )
                .map_err(|error| error.to_string())?;
                let prompt = retarget::build_bvh_agent_prompt(
                    &bvh,
                    &source,
                    &target,
                    mapping.as_ref(),
                )
                .map_err(|error| error.to_string())?;
                retarget::save_agent_prompt(&result_path, &prompt)
                    .map_err(|error| error.to_string())
            })();
            let _ = sender.send(ExportTaskResult {
                kind: "Agent BVH mapping prompt".to_owned(),
                path: result_path,
                result,
            });
        });
    }

    pub(crate) fn export_glb_retarget_agent_prompt(&mut self) {
        if self.task_busy {
            self.log.append(
                "[retarget_agent] Wait for the current background task",
            );
            return;
        }
        if self.glb.is_none() {
            self.log.append("[retarget_agent] Open a source GLB first");
            return;
        }
        let Some(target_document) = self.glb_retarget_target.clone() else {
            self.log
                .append("[retarget_agent] Choose a target GLB first");
            return;
        };
        let Some(source_path) = self.glb_path.clone() else {
            self.log
                .append("[retarget_agent] Source GLB path is unavailable");
            return;
        };
        let Some(target_path) = self.glb_retarget_target_path.clone() else {
            self.log
                .append("[retarget_agent] Target GLB path is unavailable");
            return;
        };
        let source_clip_index = self.glb_animation_index;
        let source_skin_index = self.retarget_source_skin_index;
        let target_skin_index = self.retarget_target_skin_index;
        let source_snapshot = match self.build_glb_retarget_source_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.log.append(&format!("[retarget_agent] {error}"));
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
            self.log.append(
                "[retarget_agent] Refusing to overwrite a source asset",
            );
            return;
        }
        let mapping = self.retarget_mapping.clone();
        let (sender, receiver) = mpsc::channel();
        self.task_rx = Some(receiver);
        self.task_busy = true;
        self.log.append(
            "[retarget_agent] Building GLB mapping prompt in background",
        );
        let result_path = path.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let source_bytes = source_snapshot
                    .to_bytes()
                    .map_err(|error| error.to_string())?;
                let runtime = AnimationRuntime::from_bytes_skeleton_only(
                    &source_bytes,
                    source_path.parent(),
                )
                .map_err(|error| error.to_string())?;
                let clip =
                    runtime.clips.get(source_clip_index).ok_or_else(|| {
                        format!("Animation {source_clip_index} does not exist")
                    })?;
                let animated_nodes = clip
                    .channels
                    .iter()
                    .map(|channel| channel.node)
                    .collect::<HashSet<_>>();
                let source = SkeletonDescriptor::from_runtime(
                    &runtime,
                    &source_snapshot,
                    source_skin_index,
                    &animated_nodes,
                    retarget::sha256_hex(&source_bytes),
                    "Y".to_owned(),
                    "-Z".to_owned(),
                    "m".to_owned(),
                )
                .map_err(|error| error.to_string())?;
                let target_skin = target_document
                    .skin_data_at(target_skin_index)
                    .map_err(|error| error.to_string())?;
                let target = SkeletonDescriptor::from_skin(
                    &target_skin,
                    SourceKind::Glb,
                    file_hash(Some(&target_path)),
                    String::new(),
                    "Y",
                    "-Z",
                    "m",
                    &HashSet::new(),
                )
                .map_err(|error| error.to_string())?;
                let prompt = retarget::build_agent_prompt(
                    &source,
                    &target,
                    Some(clip),
                    mapping.as_ref(),
                )
                .map_err(|error| error.to_string())?;
                retarget::save_agent_prompt(&result_path, &prompt)
                    .map_err(|error| error.to_string())
            })();
            let _ = sender.send(ExportTaskResult {
                kind: "Agent GLB mapping prompt".to_owned(),
                path: result_path,
                result,
            });
        });
    }
}

fn file_hash(path: Option<&Path>) -> String {
    path.and_then(|path| fs::read(path).ok())
        .map(|bytes| retarget::sha256_hex(&bytes))
        .unwrap_or_default()
}

fn same_path(path: &Path, source: Option<&Path>) -> bool {
    source.is_some_and(|source| {
        let left =
            fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let right =
            fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        #[cfg(windows)]
        {
            left.to_string_lossy()
                .eq_ignore_ascii_case(&right.to_string_lossy())
        }
        #[cfg(not(windows))]
        {
            left == right
        }
    })
}
