use super::*;

pub(super) fn expected_revision(command: &SessionCommand) -> Option<u64> {
    match command {
        SessionCommand::GetState
        | SessionCommand::Capture { .. }
        | SessionCommand::JobGet { .. }
        | SessionCommand::JobCancel { .. } => None,
        SessionCommand::OpenGlb {
            expected_revision, ..
        }
        | SessionCommand::OpenBvh {
            expected_revision, ..
        }
        | SessionCommand::SetGlbEdits {
            expected_revision, ..
        }
        | SessionCommand::PreflightGlb {
            expected_revision, ..
        }
        | SessionCommand::ExportGlb {
            expected_revision, ..
        }
        | SessionCommand::ConfigureBvh {
            expected_revision, ..
        }
        | SessionCommand::ExportBvh {
            expected_revision, ..
        }
        | SessionCommand::OpenRetarget {
            expected_revision, ..
        }
        | SessionCommand::SetRetargetMapping {
            expected_revision, ..
        }
        | SessionCommand::RetargetAction {
            expected_revision, ..
        }
        | SessionCommand::ConfigureBatch {
            expected_revision, ..
        }
        | SessionCommand::RunBatch {
            expected_revision, ..
        }
        | SessionCommand::ConfigureConverter {
            expected_revision, ..
        }
        | SessionCommand::RunConverter { expected_revision }
        | SessionCommand::PreflightConverter { expected_revision }
        | SessionCommand::SetView {
            expected_revision, ..
        }
        | SessionCommand::Checkpoint {
            expected_revision, ..
        } => Some(*expected_revision),
    }
}

pub(super) fn require_absolute(path: &Path, label: &str) -> Result<(), String> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(format!(
            "{label} must be an absolute path: {}",
            path.display()
        ))
    }
}

pub(super) fn job_error(kind: &str, message: impl Into<String>) -> automation::BridgeError {
    automation::BridgeError {
        kind: kind.to_owned(),
        message: message.into(),
    }
}

pub(super) fn operation_job_error(error: operations::OperationError) -> automation::BridgeError {
    let kind = match error.kind {
        operations::ErrorKind::Validation => "validation",
        operations::ErrorKind::Io => "io",
        operations::ErrorKind::External => "external",
    };
    job_error(kind, error.to_string())
}

pub(super) fn batch_request(request: AutomationBatchRequest) -> batch_runner::BatchRequest {
    let recipe = request.recipe.unwrap_or_default().to_options().to_recipe();
    batch_runner::BatchRequest {
        input_root: request.input_root,
        inputs: request.inputs,
        output_root: request.output_root,
        recipe,
        selection: request.selection,
        overwrite_existing: request.overwrite,
    }
}

pub(super) fn batch_result_json(
    entries: &[batch_runner::BatchEntry],
    all_valid: bool,
    exported: bool,
) -> Value {
    let files = entries
        .iter()
        .map(|entry| {
            json!({
                "input":entry.input,
                "status":match entry.status {
                    batch_runner::BatchFileStatus::Ready=>"ready",
                    batch_runner::BatchFileStatus::ReadyWithWarnings=>"ready_with_warnings",
                    batch_runner::BatchFileStatus::Error=>"error",
                    batch_runner::BatchFileStatus::Exporting=>"exporting",
                    batch_runner::BatchFileStatus::Succeeded=>"succeeded",
                    batch_runner::BatchFileStatus::Failed=>"failed",
                    batch_runner::BatchFileStatus::Skipped=>"skipped",
                },
                "outputs":entry.outputs.iter().map(|output|&output.path).collect::<Vec<_>>(),
                "warnings":entry.warnings,
                "error":entry.error,
                "completed_outputs":entry.completed_outputs,
            })
        })
        .collect::<Vec<_>>();
    json!({"all_valid":all_valid,"exported":exported,"files":files})
}

pub(super) fn converter_output_path(root: &Path, input: &Path) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    root.join(format!("{stem}_normalized.glb"))
}

pub(super) fn restore_file_tree(
    tree: &mut crate::modules::ui::file_tree::FileTree,
    root: Option<PathBuf>,
    files: &[PathBuf],
) {
    if let Some(root) = root {
        tree.open_folder(root);
    } else {
        tree.clear();
    }
    let _ = tree.take_root_changed();
    for path in files {
        tree.select_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_file_tree_restores_root_and_selection_without_persisting_preferences() {
        let directory =
            std::env::temp_dir().join(format!("aio-checkpoint-tree-{}", uuid::Uuid::new_v4()));
        let original_root = directory.join("original");
        let changed_root = directory.join("changed");
        std::fs::create_dir_all(&original_root).expect("original root is created");
        std::fs::create_dir_all(&changed_root).expect("changed root is created");
        let selected = original_root.join("asset.glb");
        std::fs::write(&selected, b"fixture").expect("fixture is created");

        let mut tree = crate::modules::ui::file_tree::FileTree::new();
        tree.open_folder(original_root.clone());
        tree.select_file(&selected);
        tree.open_folder(changed_root);
        restore_file_tree(
            &mut tree,
            Some(original_root.clone()),
            std::slice::from_ref(&selected),
        );

        assert_eq!(tree.root(), Some(&original_root));
        assert_eq!(tree.selected_files(), vec![selected]);
        assert!(!tree.take_root_changed());
        std::fs::remove_dir_all(directory).expect("fixture directory is removed");
    }
}
