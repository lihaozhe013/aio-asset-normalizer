//! File-oriented MCP tools backed by the shared domain operations.

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, OnceLock,
    },
    thread,
    thread::JoinHandle,
};

use aio_asset_normalizer::modules::{
    atomic_file,
    bvh::BvhDocument,
    glb::{
        batch_runner::{self, BatchEntry, BatchFileStatus, BatchRequest},
        GlbDocument,
    },
    operations::{
        self, bvh as bvh_ops, converter as converter_ops, glb as glb_ops, retarget as retarget_ops,
        spec::{EditJobFile, RecipeSpec, SelectionSpec},
    },
    retarget::SkeletonMapping,
};
use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

static ASSET_OPERATION: OnceLock<Mutex<()>> = OnceLock::new();
static FILE_JOBS: OnceLock<Mutex<HashMap<String, FileJob>>> = OnceLock::new();

struct FileJob {
    kind: String,
    status: String,
    progress: f32,
    message: Option<String>,
    result: Option<Value>,
    error: Option<Value>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone)]
struct FileJobReporter {
    job_id: String,
}

impl FileJobReporter {
    fn update(&self, progress: f32, message: impl Into<String>) {
        if let Some(job) = FILE_JOBS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(&self.job_id)
        {
            job.progress = progress.clamp(0.0, 1.0);
            job.message = Some(message.into());
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlbInspectRequest {
    /// Absolute path to the source GLB.
    pub input: PathBuf,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlbEditRequest {
    /// Typed GLB edit job. Every path in the job must be absolute.
    pub job: EditJobFile,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BvhInspectRequest {
    /// Absolute path to the source BVH.
    pub input: PathBuf,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetargetAction {
    Prompt,
    Suggest,
    Validate,
    Run,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetargetRequest {
    /// Selects one existing shared retarget workflow.
    pub action: RetargetAction,
    /// Typed retarget request. Its asset paths must be absolute.
    pub job: retarget_ops::RetargetJob,
    /// Optional Mapping v2 object. When present, it takes precedence over the job's mapping file.
    #[serde(default)]
    pub mapping: Option<SkeletonMapping>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GlbBatchAction {
    Preflight,
    Export,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlbBatchRequest {
    /// Preflight only or preflight followed by export.
    pub action: GlbBatchAction,
    /// Absolute root containing input files.
    pub input_root: PathBuf,
    /// Absolute input paths.
    pub inputs: Vec<PathBuf>,
    /// Absolute output root.
    pub output_root: PathBuf,
    /// Export recipe, mutually exclusive with explicit selection.
    #[serde(default)]
    pub recipe: Option<RecipeSpec>,
    /// Explicit export selection, mutually exclusive with recipe options.
    #[serde(default)]
    pub selection: Option<SelectionSpec>,
    /// Allow replacement of existing output files.
    #[serde(default)]
    pub overwrite: bool,
}

fn enter_local_asset_operation() -> Result<MutexGuard<'static, ()>, Value> {
    match ASSET_OPERATION.get_or_init(|| Mutex::new(())).try_lock() {
        Ok(guard) => Ok(guard),
        Err(_) => Err(error_value(
            "busy",
            "Another asset operation is already running",
        )),
    }
}

fn enter_asset_operation() -> Result<
    (
        aio_asset_normalizer::modules::task_lock::AssetTaskGuard,
        MutexGuard<'static, ()>,
    ),
    Value,
> {
    use aio_asset_normalizer::modules::task_lock::AssetTaskLockError;
    let host_lock = match aio_asset_normalizer::modules::task_lock::AssetTaskGuard::try_acquire() {
        Ok(guard) => guard,
        Err(AssetTaskLockError::Busy) => {
            return Err(error_value(
                "busy",
                "Another asset computation is active for this user",
            ));
        }
        Err(error) => return Err(error_value("external", error.to_string())),
    };
    let local_lock = enter_local_asset_operation()?;
    Ok((host_lock, local_lock))
}

fn require_absolute(path: &Path, label: &str) -> Result<(), Value> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(error_value(
            "validation",
            format!("{label} must be an absolute path: {}", path.display()),
        ))
    }
}

fn success(value: Value) -> CallToolResult {
    CallToolResult::structured(value)
}

fn failure(error: Value) -> CallToolResult {
    CallToolResult::structured_error(json!({ "ok": false, "error": error }))
}

fn error_value(kind: &str, message: impl Into<String>) -> Value {
    json!({ "kind": kind, "message": message.into() })
}

fn start_file_job<F>(kind: &str, work: F) -> CallToolResult
where
    F: FnOnce(
            Arc<AtomicBool>,
            aio_asset_normalizer::modules::task_lock::AssetTaskGuard,
            FileJobReporter,
        ) -> CallToolResult
        + Send
        + 'static,
{
    use aio_asset_normalizer::modules::task_lock::{AssetTaskGuard, AssetTaskLockError};
    let host_task_lock = match AssetTaskGuard::try_acquire() {
        Ok(guard) => guard,
        Err(AssetTaskLockError::Busy) => {
            return failure(error_value(
                "busy",
                "Another asset computation is active for this user",
            ));
        }
        Err(error) => return failure(error_value("external", error.to_string())),
    };
    let jobs = FILE_JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut records = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if records
        .values()
        .any(|job| job.status == "running" || job.status == "cancelling")
    {
        return failure(error_value(
            "busy",
            "Another file-oriented asset job is running in this MCP process",
        ));
    }
    if records.len() >= 100 {
        let completed_id = records
            .iter()
            .find(|(_, job)| {
                job.status == "succeeded" || job.status == "failed" || job.status == "cancelled"
            })
            .map(|(id, _)| id.clone());
        if let Some(completed_id) = completed_id {
            if let Some(mut completed) = records.remove(&completed_id) {
                if let Some(handle) = completed.thread.take() {
                    let _ = handle.join();
                }
            }
        } else {
            return failure(error_value(
                "busy",
                "The file job history is full; restart the MCP server to clear it",
            ));
        }
    }
    let job_id = uuid::Uuid::new_v4().to_string();
    let cancellation = Arc::new(AtomicBool::new(false));
    records.insert(
        job_id.clone(),
        FileJob {
            kind: kind.to_owned(),
            status: "running".to_owned(),
            progress: 0.0,
            message: Some("Preparing immutable job snapshot".to_owned()),
            result: None,
            error: None,
            cancel: cancellation.clone(),
            thread: None,
        },
    );
    drop(records);

    let worker_id = job_id.clone();
    let worker_cancel = cancellation.clone();
    let reporter = FileJobReporter {
        job_id: job_id.clone(),
    };
    let worker_reporter = reporter.clone();
    let spawn_result = thread::Builder::new()
        .name(format!("mcp-file-job-{}", kind))
        .spawn(move || {
            let response = work(worker_cancel.clone(), host_task_lock, worker_reporter);
            let payload = response.structured_content.clone().unwrap_or_else(|| {
                serde_json::to_value(&response).unwrap_or_else(|_| json!({"ok":false}))
            });
            let response_failed = response.is_error.unwrap_or(false);
            let error_kind = payload
                .get("error")
                .and_then(|value| value.get("kind"))
                .and_then(Value::as_str);
            if let Some(job) = FILE_JOBS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_mut(&worker_id)
            {
                job.progress = 1.0;
                job.result = Some(payload.clone());
                if error_kind == Some("cancelled") || worker_cancel.load(Ordering::Acquire) {
                    job.status = "cancelled".to_owned();
                    job.message = Some("Cancelled at an operation boundary".to_owned());
                    job.error = Some(json!({
                        "kind":"cancelled",
                        "message":"Cancellation was observed at an operation boundary; inspect result for completed outputs"
                    }));
                } else if response_failed {
                    job.status = "failed".to_owned();
                    job.message = Some("Job failed".to_owned());
                    job.error = Some(payload);
                } else {
                    job.status = "succeeded".to_owned();
                    job.message = Some("Job completed".to_owned());
                    job.error = None;
                }
            }
        });

    match spawn_result {
        Ok(handle) => {
            if let Some(job) = FILE_JOBS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_mut(&job_id)
            {
                job.thread = Some(handle);
            }
            CallToolResult::structured(json!({
                "job_id":job_id,
                "kind":kind,
                "status":"running"
            }))
        }
        Err(error) => {
            if let Some(job) = FILE_JOBS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_mut(&job_id)
            {
                job.status = "failed".to_owned();
                job.message = Some("Worker could not start".to_owned());
                job.error = Some(error_value("external", error.to_string()));
            }
            failure(error_value("external", error.to_string()))
        }
    }
}

pub fn file_job_get(job_id: &str) -> CallToolResult {
    let jobs = FILE_JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    let records = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(job) = records.get(job_id) else {
        return failure(error_value(
            "job_not_found",
            format!("No file job has id {job_id}"),
        ));
    };
    success(json!({
        "job_id":job_id,
        "kind":job.kind,
        "status":job.status,
        "progress":job.progress,
        "message":job.message,
        "result":job.result,
        "error":job.error,
    }))
}

pub fn file_job_cancel(job_id: &str) -> CallToolResult {
    let jobs = FILE_JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut records = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(job) = records.get_mut(job_id) else {
        return failure(error_value(
            "job_not_found",
            format!("No file job has id {job_id}"),
        ));
    };
    match job.status.as_str() {
        "running" => {
            job.status = "cancelling".to_owned();
            job.cancel.store(true, Ordering::Release);
            success(json!({"job_id":job_id,"status":"cancelling"}))
        }
        "cancelling" => success(json!({"job_id":job_id,"status":"cancelling"})),
        status => success(json!({"job_id":job_id,"status":status,"cancelled":false})),
    }
}

pub fn shutdown_file_jobs() {
    let jobs = FILE_JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    let handles = {
        let mut records = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for job in records.values_mut() {
            if job.status == "running" || job.status == "cancelling" {
                job.cancel.store(true, Ordering::Release);
                job.status = "cancelling".to_owned();
            }
        }
        records
            .values_mut()
            .filter_map(|job| job.thread.take())
            .collect::<Vec<_>>()
    };
    for handle in handles {
        let _ = handle.join();
    }
}

fn operation_error(error: operations::OperationError) -> Value {
    let kind = match error.kind {
        operations::ErrorKind::Validation => "validation",
        operations::ErrorKind::Io => "io",
        operations::ErrorKind::External => "external",
    };
    error_value(kind, error.message)
}

fn glb_error(error: impl std::fmt::Display) -> Value {
    error_value("validation", error.to_string())
}

pub fn glb_inspect(request: GlbInspectRequest) -> CallToolResult {
    let _guard = match enter_asset_operation() {
        Ok(guard) => guard,
        Err(error) => return failure(error),
    };
    if let Err(error) = require_absolute(&request.input, "input") {
        return failure(error);
    }
    let result = (|| -> Result<Value, Value> {
        let document = GlbDocument::load(&request.input).map_err(glb_error)?;
        let catalog = document.export_catalog().map_err(glb_error)?;
        let skins = catalog
            .skins
            .iter()
            .map(|skin| match document.skin_data_at(skin.index) {
                Ok(data) => json!({
                    "index": data.index,
                    "name": data.name,
                    "skeleton": data.skeleton,
                    "joints": data.joints,
                    "mesh_nodes": data.mesh_nodes,
                    "nodes": data.nodes.iter().map(|node| json!({
                        "index": node.index,
                        "name": node.name,
                        "parent": node.parent,
                        "translation": node.translation,
                        "rotation": node.rotation,
                        "scale": node.scale,
                    })).collect::<Vec<_>>(),
                }),
                Err(error) => {
                    json!({"index": skin.index, "name": skin.name, "error": error.to_string()})
                }
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "ok": true,
            "path": request.input,
            "summary": document.summary(),
            "catalog": {
                "scenes": catalog.scenes.iter().map(|v| json!({"index":v.index,"name":v.name,"roots":v.roots})).collect::<Vec<_>>(),
                "nodes": catalog.nodes.iter().map(|v| json!({"index":v.index,"name":v.name,"parent":v.parent,"children":v.children,"mesh":v.mesh,"skin":v.skin})).collect::<Vec<_>>(),
                "meshes": catalog.meshes.iter().map(|v| json!({"index":v.index,"name":v.name,"primitives":v.primitives.iter().map(|p| json!({"index":p.index,"material":p.material})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
                "skins": catalog.skins.iter().map(|v| json!({"index":v.index,"name":v.name,"joint_count":v.joint_count})).collect::<Vec<_>>(),
                "animations": catalog.animations.iter().map(|v| json!({"index":v.index,"name":v.name})).collect::<Vec<_>>(),
            },
            "skin_hierarchies": skins,
        }))
    })();
    result.map(success).unwrap_or_else(failure)
}

pub fn glb_edit(request: GlbEditRequest) -> CallToolResult {
    start_file_job("glb_edit", move |cancel, _host_lock, reporter| {
        glb_edit_sync(request, &cancel, &reporter)
    })
}

fn glb_edit_sync(
    request: GlbEditRequest,
    cancellation: &AtomicBool,
    reporter: &FileJobReporter,
) -> CallToolResult {
    let _guard = match enter_local_asset_operation() {
        Ok(guard) => guard,
        Err(error) => return failure(error),
    };
    let job = request.job;
    if job
        .command
        .as_deref()
        .is_some_and(|value| value != "glb.edit")
    {
        return failure(error_value(
            "validation",
            "GLB edit job declares a different command",
        ));
    }
    for (path, label) in [(&job.input, "input"), (&job.output, "output")] {
        if let Err(error) = require_absolute(path, label) {
            return failure(error);
        }
    }
    for texture in &job.edits.textures {
        if let Err(error) = require_absolute(&texture.image, "texture image") {
            return failure(error);
        }
    }
    let result = (|| -> Result<Value, Value> {
        reporter.update(0.1, "Loading and applying GLB edits");
        let source = GlbDocument::load(&job.input).map_err(glb_error)?;
        let edited = glb_ops::edit_snapshot(&source, &job.edits).map_err(operation_error)?;
        let selection =
            glb_ops::resolve_selection(&edited, job.export.as_ref(), job.selection.as_ref())
                .map_err(operation_error)?;
        let sources = [job.input.as_path()];
        let jobs = glb_ops::prepare_export(
            &edited,
            &job.edits.export_edits(),
            &selection,
            &job.output,
            &sources,
            job.overwrite,
        )
        .map_err(operation_error)?;
        let mut outputs = Vec::with_capacity(jobs.len());
        let total = jobs.len().max(1);
        for (index, export_job) in jobs.into_iter().enumerate() {
            if cancellation.load(Ordering::Acquire) {
                return Err(error_value(
                    "cancelled",
                    format!(
                        "GLB edit cancelled after {} completed output(s)",
                        outputs.len()
                    ),
                ));
            }
            let report =
                glb_ops::execute_job(&export_job, job.overwrite).map_err(operation_error)?;
            outputs.push(json!({"path":export_job.path,"report":report}));
            reporter.update(
                0.2 + (index + 1) as f32 / total as f32 * 0.8,
                format!("Wrote output {} of {total}", index + 1),
            );
        }
        Ok(json!({"ok":true,"input":job.input,"outputs":outputs,"warnings":[]}))
    })();
    result.map(success).unwrap_or_else(failure)
}

pub fn bvh_inspect(request: BvhInspectRequest) -> CallToolResult {
    let _guard = match enter_asset_operation() {
        Ok(guard) => guard,
        Err(error) => return failure(error),
    };
    if let Err(error) = require_absolute(&request.input, "input") {
        return failure(error);
    }
    match BvhDocument::load(&request.input) {
        Ok(document) => success(json!({
            "ok": true,
            "path": request.input,
            "frame_count": document.frames.len(),
            "joint_count": document.joints.len(),
            "frame_time": document.frame_time,
            "duration": document.duration(),
            "joints": document.joints.iter().enumerate().map(|(index, joint)| json!({"index":index,"name":joint.name,"parent":joint.parent,"offset":joint.offset,"channels":joint.channels})).collect::<Vec<_>>(),
        })),
        Err(error) => failure(glb_error(error)),
    }
}

pub fn bvh_process(request: bvh_ops::BvhProcessRequest) -> CallToolResult {
    start_file_job("bvh_process", move |cancel, _host_lock, reporter| {
        bvh_process_sync(request, &cancel, &reporter)
    })
}

fn bvh_process_sync(
    request: bvh_ops::BvhProcessRequest,
    cancellation: &AtomicBool,
    reporter: &FileJobReporter,
) -> CallToolResult {
    let _guard = match enter_local_asset_operation() {
        Ok(guard) => guard,
        Err(error) => return failure(error),
    };
    if request
        .command
        .as_deref()
        .is_some_and(|value| value != "bvh.process")
    {
        return failure(error_value(
            "validation",
            "BVH request declares a different command",
        ));
    }
    for (path, label) in [(&request.input, "input"), (&request.output, "output")] {
        if let Err(error) = require_absolute(path, label) {
            return failure(error);
        }
    }
    let result = (|| -> Result<Value, Value> {
        reporter.update(0.2, "Loading and trimming BVH");
        let input = BvhDocument::load(&request.input).map_err(glb_error)?;
        let output = bvh_ops::prepare(&input, request.trim).map_err(operation_error)?;
        if cancellation.load(Ordering::Acquire) {
            return Err(error_value(
                "cancelled",
                "BVH processing cancelled before output",
            ));
        }
        reporter.update(0.8, "Writing validated BVH output");
        bvh_ops::write(&output, &request.output, &request.input, request.overwrite)
            .map_err(operation_error)?;
        let reparsed = BvhDocument::load(&request.output).map_err(glb_error)?;
        Ok(
            json!({"ok":true,"input":request.input,"output":request.output,"frame_count":reparsed.frames.len(),"joint_count":reparsed.joints.len(),"duration":reparsed.duration()}),
        )
    })();
    result.map(success).unwrap_or_else(failure)
}

pub fn retarget(request: RetargetRequest) -> CallToolResult {
    start_file_job("retarget", move |cancel, _host_lock, reporter| {
        retarget_sync(request, &cancel, &reporter)
    })
}

fn retarget_sync(
    request: RetargetRequest,
    cancellation: &AtomicBool,
    reporter: &FileJobReporter,
) -> CallToolResult {
    let _guard = match enter_local_asset_operation() {
        Ok(guard) => guard,
        Err(error) => return failure(error),
    };
    let job = request.job;
    for (path, label) in [
        Some((&job.source, "source")),
        Some((&job.target, "target")),
        job.mapping.as_ref().map(|v| (v, "mapping file")),
        job.out.as_ref().map(|v| (v, "output")),
    ]
    .into_iter()
    .flatten()
    {
        if let Err(error) = require_absolute(path, label) {
            return failure(error);
        }
    }
    let result = (|| -> Result<Value, Value> {
        reporter.update(0.15, "Loading retarget source and target");
        let (source, target) = (
            retarget_ops::load_source(&job).map_err(operation_error)?,
            retarget_ops::load_target(&job).map_err(operation_error)?,
        );
        let mapping = match request.mapping {
            Some(mapping) => Some(mapping),
            None if job.mapping.is_some() => Some(
                retarget_ops::load_mapping(
                    job.mapping.as_deref().unwrap_or_else(|| Path::new("")),
                    &source,
                    &target,
                )
                .map_err(operation_error)?,
            ),
            None => None,
        };
        match request.action {
            RetargetAction::Prompt => {
                let prompt = retarget_ops::prompt(&source, &target, mapping.as_ref())
                    .map_err(operation_error)?;
                reporter.update(0.7, "Prepared retarget prompt");
                if let Some(output) = job.out.as_ref() {
                    let mut inputs = vec![job.source.as_path(), job.target.as_path()];
                    if let Some(path) = job.mapping.as_deref() {
                        inputs.push(path);
                    }
                    operations::check_output(output, &inputs, job.overwrite)
                        .map_err(operation_error)?;
                    if cancellation.load(Ordering::Acquire) {
                        return Err(error_value(
                            "cancelled",
                            "Retarget prompt cancelled before output",
                        ));
                    }
                    atomic_file::write(output, prompt.as_bytes(), job.overwrite)
                        .map_err(|error| error_value("io", error.to_string()))?;
                }
                Ok(
                    json!({"ok":true,"action":"prompt","prompt":prompt,"output":job.out,"bytes":prompt.len()}),
                )
            }
            RetargetAction::Suggest => {
                let retarget_ops::Source::Bvh { document, .. } = &source else {
                    return Err(error_value(
                        "validation",
                        "Name suggestions require a BVH source; use the prompt action for GLB",
                    ));
                };
                let suggestions = document.suggest_mapping(&target.skin).iter().map(|item| json!({"source_joint":item.source_joint,"target_node":item.target_node,"confidence":format!("{:?}",item.confidence).to_ascii_lowercase()})).collect::<Vec<_>>();
                reporter.update(0.8, "Generated mapping suggestions");
                Ok(json!({"ok":true,"action":"suggest","suggestions":suggestions}))
            }
            RetargetAction::Validate => {
                let mapping = mapping.ok_or_else(|| {
                    error_value(
                        "validation",
                        "A Mapping object or absolute mapping file is required",
                    )
                })?;
                let report = retarget_ops::validate(&source, &target, &mapping);
                reporter.update(0.9, "Validated Mapping v2");
                Ok(
                    json!({"ok":report.is_valid(),"action":"validate","valid":report.is_valid(),"mapped_count":report.mapped_count,"unmapped_source_nodes":report.unmapped_source_nodes,"errors":report.errors,"warnings":report.warnings}),
                )
            }
            RetargetAction::Run => {
                let mapping = mapping.ok_or_else(|| {
                    error_value(
                        "validation",
                        "A Mapping object or absolute mapping file is required",
                    )
                })?;
                let validation = retarget_ops::validate(&source, &target, &mapping);
                let export_job = retarget_ops::prepare(&source, &target, &mapping, &job)
                    .map_err(operation_error)?;
                reporter.update(0.8, "Prepared retargeted animation");
                if cancellation.load(Ordering::Acquire) {
                    return Err(error_value("cancelled", "Retarget cancelled before output"));
                }
                let report =
                    glb_ops::execute_job(&export_job, job.overwrite).map_err(operation_error)?;
                Ok(
                    json!({"ok":true,"action":"run","output":export_job.path,"report":report,"warnings":validation.warnings}),
                )
            }
        }
    })();
    result.map(success).unwrap_or_else(failure)
}

pub fn glb_batch(request: GlbBatchRequest) -> CallToolResult {
    start_file_job("glb_batch", move |cancel, _host_lock, reporter| {
        glb_batch_sync(request, &cancel, &reporter)
    })
}

fn glb_batch_sync(
    request: GlbBatchRequest,
    cancellation: &AtomicBool,
    reporter: &FileJobReporter,
) -> CallToolResult {
    let _guard = match enter_local_asset_operation() {
        Ok(guard) => guard,
        Err(error) => return failure(error),
    };
    for (path, label) in std::iter::once((&request.input_root, "input_root"))
        .chain(std::iter::once((&request.output_root, "output_root")))
        .chain(request.inputs.iter().map(|v| (v, "input")))
    {
        if let Err(error) = require_absolute(path, label) {
            return failure(error);
        }
    }
    if request.selection.is_some() && request.recipe.is_some() {
        return failure(error_value(
            "validation",
            "Recipe options and explicit selection must not both be supplied",
        ));
    }
    let total_files = request.inputs.len().max(1);
    let batch = BatchRequest {
        input_root: request.input_root,
        inputs: request.inputs,
        output_root: request.output_root,
        recipe: request.recipe.unwrap_or_default().to_options().to_recipe(),
        selection: request.selection,
        overwrite_existing: request.overwrite,
    };
    let (entries, all_valid) = batch_runner::run_preflight_cancellable(
        &batch,
        aio_asset_normalizer::modules::logging::next_task_id(),
        cancellation,
        &mut |event| {
            if let batch_runner::BatchProgress::PreflightFile { index, entry } = event {
                reporter.update(
                    (index + 1) as f32 / total_files as f32 * 0.4,
                    format!("Preflighted {}", entry.input.display()),
                );
            }
        },
    );
    if cancellation.load(Ordering::Acquire) {
        return failure(error_value(
            "cancelled",
            format!("Batch preflight cancelled after {} file(s)", entries.len()),
        ));
    }
    let mut exported = false;
    let mut export_error = None;
    if matches!(request.action, GlbBatchAction::Export) && all_valid {
        exported = true;
        if let Err(error) = batch_runner::run_export_cancellable(
            &batch,
            &entries,
            aio_asset_normalizer::modules::logging::next_task_id(),
            cancellation,
            &mut |event| {
                if let batch_runner::BatchProgress::ExportFinished { index, .. } = event {
                    reporter.update(
                        0.4 + (index + 1) as f32 / total_files as f32 * 0.6,
                        format!("Exported batch file {}", index + 1),
                    );
                }
            },
        ) {
            export_error = Some(operation_error(error));
        }
    }
    success(batch_value(&entries, all_valid, exported, export_error))
}

fn batch_value(
    entries: &[BatchEntry],
    all_valid: bool,
    exported: bool,
    export_error: Option<Value>,
) -> Value {
    let files = entries.iter().map(|entry| json!({
        "input":entry.input,
        "status":match entry.status { BatchFileStatus::Ready=>"ready",BatchFileStatus::ReadyWithWarnings=>"ready_with_warnings",BatchFileStatus::Error=>"error",BatchFileStatus::Exporting=>"exporting",BatchFileStatus::Succeeded=>"succeeded",BatchFileStatus::Failed=>"failed",BatchFileStatus::Skipped=>"skipped" },
        "outputs":entry.outputs.iter().map(|v| &v.path).collect::<Vec<_>>(),
        "warnings":entry.warnings,
        "error":entry.error,
        "completed_outputs":entry.completed_outputs,
    })).collect::<Vec<_>>();
    json!({"ok":all_valid && export_error.is_none(),"all_valid":all_valid,"exported":exported && export_error.is_none(),"files":files,"export_error":export_error})
}

pub fn fbx_convert(request: converter_ops::ConversionRequest) -> CallToolResult {
    start_file_job("fbx_convert", move |cancel, _host_lock, reporter| {
        fbx_convert_sync(request, &cancel, &reporter)
    })
}

fn fbx_convert_sync(
    request: converter_ops::ConversionRequest,
    cancellation: &AtomicBool,
    reporter: &FileJobReporter,
) -> CallToolResult {
    let _guard = match enter_local_asset_operation() {
        Ok(guard) => guard,
        Err(error) => return failure(error),
    };
    for (path, label) in [(&request.input, "input"), (&request.output, "output")] {
        if let Err(error) = require_absolute(path, label) {
            return failure(error);
        }
    }
    if let Some(path) = request.blender.as_ref() {
        if let Err(error) = require_absolute(path, "blender") {
            return failure(error);
        }
    }
    reporter.update(0.1, "Starting Blender conversion");
    match converter_ops::execute_cancellable(
        &request,
        aio_asset_normalizer::modules::logging::next_task_id(),
        cancellation,
    ) {
        Ok(summary) => success(
            json!({"ok":true,"input":request.input,"output":request.output,"summary":summary}),
        ),
        Err(error) => failure(operation_error(error)),
    }
}
