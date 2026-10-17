use super::*;

impl App {
    pub(super) fn configure_batch_session(
        &mut self,
        request: AutomationBatchRequest,
    ) -> BridgeResponse {
        for (path, label) in std::iter::once((&request.input_root, "batch input root"))
            .chain(std::iter::once((&request.output_root, "batch output root")))
            .chain(request.inputs.iter().map(|path| (path, "batch input")))
        {
            if let Err(error) = require_absolute(path, label) {
                return self.session_failure("validation", error);
            }
        }
        if !request.input_root.is_dir() || !request.output_root.is_dir() {
            return self.session_failure(
                "validation",
                "Batch input and output roots must be existing directories",
            );
        }
        if request.recipe.is_some() && request.selection.is_some() {
            return self.session_failure(
                "validation",
                "Recipe and explicit selection are mutually exclusive",
            );
        }
        if request.inputs.is_empty() {
            return self.session_failure("validation", "Batch input list is empty");
        }
        if let Some(path) = request.inputs.iter().find(|path| !path.is_file()) {
            return self.session_failure(
                "io",
                format!("Batch input is unavailable: {}", path.display()),
            );
        }
        self.file_tree.open_folder(request.input_root.clone());
        for input in &request.inputs {
            self.file_tree.select_file(input);
        }
        self.page = Page::GlbEditor;
        self.glb_batch.scope = GlbInspectorScope::Batch;
        if let Some(recipe) = request.recipe.as_ref() {
            self.glb_batch.recipe = recipe.to_options().to_recipe();
        }
        self.glb_batch.output_root = Some(request.output_root.clone());
        self.glb_batch.overwrite_existing = request.overwrite;
        self.invalidate_glb_batch_preflight();
        if let Some(state) = self.automation.as_mut() {
            state.batch_request = Some(request.clone());
        }
        self.commit_session_revision();
        self.session_success(Some(
            json!({"configured":true,"input_count":request.inputs.len()}),
        ))
    }

    pub(super) fn start_batch_session_job(&mut self, export: bool) -> BridgeResponse {
        if self.task_busy || self.converter_busy || self.glb_batch.is_busy() {
            return self.session_failure("busy", "Another GUI asset task is already running");
        }
        let Some(config) = self
            .automation
            .as_ref()
            .and_then(|state| state.batch_request.clone())
        else {
            return self.session_failure("validation", "Configure a batch before running it");
        };
        let request = batch_request(config);
        let worker = move |cancel: Arc<AtomicBool>,
                           sender: mpsc::Sender<JobEvent>,
                           job_id: String| {
            let task_id = crate::modules::logging::next_task_id();
            let mut progress = |event: batch_runner::BatchProgress| match event {
                batch_runner::BatchProgress::PreflightFile { index, entry } => {
                    let percent = (index + 1) as f32 / request.inputs.len().max(1) as f32 * 0.4;
                    let _ = sender.send(JobEvent::Progress {
                        job_id: job_id.clone(),
                        progress: percent,
                        message: format!("Preflighted {}", entry.input.display()),
                    });
                }
                batch_runner::BatchProgress::ExportFinished { index, .. } if export => {
                    let percent =
                        0.4 + (index + 1) as f32 / request.inputs.len().max(1) as f32 * 0.6;
                    let _ = sender.send(JobEvent::Progress {
                        job_id: job_id.clone(),
                        progress: percent,
                        message: format!("Exported batch file {}", index + 1),
                    });
                }
                _ => {}
            };
            if cancel.load(Ordering::Acquire) {
                return Err(job_error("cancelled", "Batch cancelled before preflight"));
            }
            let (entries, all_valid) =
                batch_runner::run_preflight_cancellable(&request, task_id, &cancel, &mut progress);
            if cancel.load(Ordering::Acquire) {
                return Err(job_error(
                    "cancelled",
                    format!("Batch preflight cancelled after {} file(s)", entries.len()),
                ));
            }
            if export && all_valid {
                if cancel.load(Ordering::Acquire) {
                    return Err(job_error(
                        "cancelled",
                        "Batch cancelled after preflight; no outputs were written",
                    ));
                }
                if let Err(error) = batch_runner::run_export_cancellable(
                    &request,
                    &entries,
                    task_id,
                    &cancel,
                    &mut progress,
                ) {
                    if cancel.load(Ordering::Acquire) {
                        return Err(job_error("cancelled", error.to_string()));
                    }
                    return Err(operation_job_error(error));
                }
            }
            Ok(batch_result_json(&entries, all_valid, export && all_valid))
        };
        let Some(state) = self.automation.as_mut() else {
            return self.session_failure("session_unavailable", "GUI session is not registered");
        };
        match state.launch_job(
            if export {
                "batch_export"
            } else {
                "batch_preflight"
            },
            worker,
        ) {
            Ok(job_id) => self.session_success(Some(
                json!({"job_id":job_id,"status":"running","export":export}),
            )),
            Err(error) => self.session_failure(&error.kind, error.message),
        }
    }

    pub(super) fn configure_converter_session(
        &mut self,
        inputs: Vec<PathBuf>,
        output_root: PathBuf,
        blender: Option<PathBuf>,
        overwrite: bool,
    ) -> BridgeResponse {
        if let Err(error) = require_absolute(&output_root, "Converter output root") {
            return self.session_failure("validation", error);
        }
        if !output_root.is_dir() {
            return self
                .session_failure("io", "Converter output root must be an existing directory");
        }
        if let Some(blender) = blender.as_ref() {
            if let Err(error) = require_absolute(blender, "Blender executable") {
                return self.session_failure("validation", error);
            }
        }
        if inputs.is_empty() {
            return self.session_failure("validation", "Converter input list is empty");
        }
        for input in &inputs {
            if let Err(error) = require_absolute(input, "Converter input") {
                return self.session_failure("validation", error);
            }
            let extension = input
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !["fbx", "obj", "blend"].contains(&extension.as_str()) || !input.is_file() {
                return self.session_failure(
                    "validation",
                    format!(
                        "Unsupported or missing Converter input: {}",
                        input.display()
                    ),
                );
            }
        }
        self.converter_file_tree
            .open_folder(inputs[0].parent().unwrap_or(Path::new(".")).to_path_buf());
        for input in &inputs {
            self.converter_file_tree.select_file(input);
        }
        self.page = Page::FbxConverter;
        self.blender_path = blender
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        if let Some(state) = self.automation.as_mut() {
            state.converter_inputs = inputs.clone();
            state.converter_output_root = Some(output_root.clone());
            state.converter_blender = blender;
            state.converter_overwrite = overwrite;
        }
        self.commit_session_revision();
        self.session_success(Some(
            json!({"configured":true,"input_count":inputs.len(),"output_root":output_root}),
        ))
    }

    pub(super) fn start_converter_session_job(&mut self) -> BridgeResponse {
        if self.task_busy || self.converter_busy || self.glb_batch.is_busy() {
            return self.session_failure("busy", "Another GUI asset task is already running");
        }
        let Some(state) = self.automation.as_ref() else {
            return self.session_failure("session_unavailable", "GUI session is not registered");
        };
        let inputs = state.converter_inputs.clone();
        let Some(output_root) = state.converter_output_root.clone() else {
            return self.session_failure("validation", "Configure the Converter output root first");
        };
        let blender = state.converter_blender.clone();
        let overwrite = state.converter_overwrite;
        if inputs.is_empty() {
            return self
                .session_failure("validation", "Configure at least one Converter input first");
        }
        let requests = inputs
            .iter()
            .map(|input| converter_ops::ConversionRequest {
                input: input.clone(),
                output: converter_output_path(&output_root, input),
                blender: blender.clone(),
                overwrite,
            })
            .collect::<Vec<_>>();
        if let Err(error) = converter_ops::validate_batch_outputs(&requests) {
            return self.session_failure(
                match error.kind {
                    operations::ErrorKind::Validation => "validation",
                    operations::ErrorKind::Io => "io",
                    operations::ErrorKind::External => "external",
                },
                error.to_string(),
            );
        }
        let worker = move |cancel: Arc<AtomicBool>,
                           sender: mpsc::Sender<JobEvent>,
                           job_id: String| {
            let mut results = Vec::with_capacity(requests.len());
            let total = requests.len().max(1);
            for (index, request) in requests.iter().enumerate() {
                if cancel.load(Ordering::Acquire) {
                    return Err(job_error(
                        "cancelled",
                        format!(
                            "Conversion stopped after {} completed file(s)",
                            results
                                .iter()
                                .filter(|entry: &&Value| entry["ok"] == true)
                                .count()
                        ),
                    ));
                }
                let result = converter_ops::execute_cancellable(
                    request,
                    crate::modules::logging::next_task_id(),
                    &cancel,
                );
                match result {
                    Ok(summary)=>results.push(json!({"input":request.input,"output":request.output,"ok":true,"summary":summary})),
                    Err(_) if cancel.load(Ordering::Acquire)=>return Err(job_error("cancelled",format!("Blender conversion stopped after {} completed file(s)",results.iter().filter(|entry:&&Value|entry["ok"]==true).count()))),
                    Err(error)=>results.push(json!({"input":request.input,"output":request.output,"ok":false,"error":{"kind":match error.kind{operations::ErrorKind::Validation=>"validation",operations::ErrorKind::Io=>"io",operations::ErrorKind::External=>"external"},"message":error.to_string()}})),
                }
                let _ = sender.send(JobEvent::Progress {
                    job_id: job_id.clone(),
                    progress: (index + 1) as f32 / total as f32,
                    message: format!("Converted file {} of {total}", index + 1),
                });
            }
            Ok(
                json!({"files":results,"succeeded":results.iter().filter(|entry|entry["ok"]==true).count(),"failed":results.iter().filter(|entry|entry["ok"]==false).count()}),
            )
        };
        let Some(state) = self.automation.as_mut() else {
            return self.session_failure("session_unavailable", "GUI session is not registered");
        };
        match state.launch_job("converter_batch", worker) {
            Ok(job_id) => self.session_success(Some(
                json!({"job_id":job_id,"status":"running","file_count":inputs.len()}),
            )),
            Err(error) => self.session_failure(&error.kind, error.message),
        }
    }

    pub(super) fn start_converter_session_preflight(&mut self) -> BridgeResponse {
        if self.task_busy || self.converter_busy || self.glb_batch.is_busy() {
            return self.session_failure("busy", "Another GUI asset task is already running");
        }
        let Some(state) = self.automation.as_ref() else {
            return self.session_failure("session_unavailable", "GUI session is not registered");
        };
        let inputs = state.converter_inputs.clone();
        let Some(output_root) = state.converter_output_root.clone() else {
            return self.session_failure("validation", "Configure the Converter output root first");
        };
        let blender = state.converter_blender.clone();
        let overwrite = state.converter_overwrite;
        if inputs.is_empty() {
            return self
                .session_failure("validation", "Configure at least one Converter input first");
        }
        let requests = inputs
            .iter()
            .map(|input| converter_ops::ConversionRequest {
                input: input.clone(),
                output: converter_output_path(&output_root, input),
                blender: blender.clone(),
                overwrite,
            })
            .collect::<Vec<_>>();
        if let Err(error) = converter_ops::validate_batch_outputs(&requests) {
            return self.session_failure(
                match error.kind {
                    operations::ErrorKind::Validation => "validation",
                    operations::ErrorKind::Io => "io",
                    operations::ErrorKind::External => "external",
                },
                error.to_string(),
            );
        }
        let worker = move |cancel: Arc<AtomicBool>,
                           sender: mpsc::Sender<JobEvent>,
                           job_id: String| {
            let mut files = Vec::with_capacity(requests.len());
            let total = requests.len().max(1);
            for (index, request) in requests.iter().enumerate() {
                if cancel.load(Ordering::Acquire) {
                    return Err(job_error(
                        "cancelled",
                        format!("Preflight stopped after {} file(s)", files.len()),
                    ));
                }
                let result = converter_ops::preflight(request);
                files.push(match result {
                    Ok(()) => json!({"input":request.input,"output":request.output,"ready":true}),
                    Err(error) => json!({
                        "input":request.input,
                        "output":request.output,
                        "ready":false,
                        "error":{
                            "kind":match error.kind {
                                operations::ErrorKind::Validation=>"validation",
                                operations::ErrorKind::Io=>"io",
                                operations::ErrorKind::External=>"external",
                            },
                            "message":error.to_string()
                        }
                    }),
                });
                let _ = sender.send(JobEvent::Progress {
                    job_id: job_id.clone(),
                    progress: (index + 1) as f32 / total as f32,
                    message: format!("Preflighted file {} of {total}", index + 1),
                });
            }
            let ready = files.iter().all(|entry| entry["ready"] == true);
            Ok(json!({"all_valid":ready,"files":files,"starts_blender":false}))
        };
        let Some(state) = self.automation.as_mut() else {
            return self.session_failure("session_unavailable", "GUI session is not registered");
        };
        match state.launch_job("converter_preflight", worker) {
            Ok(job_id) => self.session_success(Some(
                json!({"job_id":job_id,"status":"running","starts_blender":false}),
            )),
            Err(error) => self.session_failure(&error.kind, error.message),
        }
    }

    pub(super) fn configure_bvh_session(
        &mut self,
        up_axis: String,
        forward_axis: String,
        unit: String,
        trim: Option<[f32; 2]>,
        reduce_keys: Option<f32>,
    ) -> BridgeResponse {
        if !matches!(unit.as_str(), "m" | "cm" | "mm") {
            return self.session_failure("validation", "BVH unit must be m, cm, or mm");
        }
        if let Err(error) = crate::modules::retarget::convert_bvh_position_to_glb(
            [0.0; 3],
            &up_axis,
            &forward_axis,
            &unit,
        ) {
            return self.session_failure("validation", error.to_string());
        }
        if let Some(tolerance) = reduce_keys {
            if !tolerance.is_finite() || tolerance <= 0.0 {
                return self.session_failure(
                    "validation",
                    "Key reduction tolerance must be a finite positive value",
                );
            }
        }
        if let Some([start, end]) = trim {
            let Some(source) = self.bvh.as_ref() else {
                return self.session_failure("validation", "Open a BVH before configuring a trim");
            };
            let mut edited = source.clone();
            if let Err(error) = edited.trim(start, end) {
                return self.session_failure("validation", error.to_string());
            }
            self.bvh = Some(edited);
            self.bvh_trim_start = 0.0;
            self.bvh_trim_end = self
                .bvh
                .as_ref()
                .map(|document| document.duration().max(document.frame_time))
                .unwrap_or(0.0);
            self.bvh_frame = 0;
            self.needs_bvh_skeleton_reload = true;
            self.bvh_camera_focus_pending = true;
        }
        self.bvh_up_axis = up_axis;
        self.bvh_forward_axis = forward_axis;
        self.set_bvh_unit_from_ui(&unit);
        if let Some(tolerance) = reduce_keys {
            self.bvh_reduce_keys = true;
            self.bvh_key_tolerance = tolerance;
        } else {
            self.bvh_reduce_keys = false;
        }
        self.refresh_v2_retarget_mapping();
        self.commit_session_revision();
        self.session_success(Some(
            json!({"configured":true,"trim_applied":trim.is_some()}),
        ))
    }
}
