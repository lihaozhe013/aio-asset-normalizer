use std::fmt;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

use super::task::{ConversionTask, ConverterMessage};

const SCRIPT_FILE_NAME: &str = "normalize_to_glb.py";
const SCRIPT_SOURCE: &str =
    include_str!("../../../blender_scripts/normalize_to_glb.py");

/// Failure modes for one headless Blender conversion. Every variant is
/// reported to the user instead of falling back to a partial write.
#[derive(Debug)]
pub enum BlenderError {
    BlenderNotFound,
    InputMissing(PathBuf),
    ScriptWrite(String),
    Spawn(String),
    ExitCode(Option<i32>),
    OutputMissing,
}

impl fmt::Display for BlenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlenderError::BlenderNotFound => write!(
                formatter,
                "Blender executable not found. Install Blender or set its \
                 path on the FBX Converter page."
            ),
            BlenderError::InputMissing(path) => write!(
                formatter,
                "Input file is not readable: {}",
                path.display()
            ),
            BlenderError::ScriptWrite(message) => write!(
                formatter,
                "Failed to stage the embedded Blender script: {message}"
            ),
            BlenderError::Spawn(message) => {
                write!(formatter, "Failed to start Blender: {message}")
            }
            BlenderError::ExitCode(code) => write!(
                formatter,
                "Blender exited with {} (see the log lines above)",
                match code {
                    Some(code) => format!("code {code}"),
                    None => "a signal".to_owned(),
                }
            ),
            BlenderError::OutputMissing => write!(
                formatter,
                "Blender reported success but no output file was created"
            ),
        }
    }
}

impl std::error::Error for BlenderError {}

/// Locate a Blender executable: user preference first, then common install
/// locations, then the system PATH.
pub fn find_blender(preferred: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = preferred {
        if let Some(resolved) = resolve_blender_path(path) {
            return Some(resolved);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let candidates = [
            "C:\\Program Files\\Blender Foundation\\Blender 5.1\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 5.0\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 4.5\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 4.4\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 4.3\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 4.2\\blender.exe",
        ];
        for candidate in candidates {
            if let Some(resolved) = resolve_blender_path(candidate) {
                return Some(resolved);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = ["/Applications/Blender.app"];
        for candidate in candidates {
            if let Some(resolved) = resolve_blender_path(candidate) {
                return Some(resolved);
            }
        }
    }

    if let Some(path) = find_in_path("blender") {
        return Some(path);
    }

    None
}

fn resolve_blender_path<P: AsRef<Path>>(path: P) -> Option<PathBuf> {
    let p = path.as_ref();

    if p.is_file() {
        return Some(p.to_path_buf());
    }

    #[cfg(target_os = "macos")]
    {
        if p.is_dir() && p.extension().is_some_and(|e| e == "app") {
            let inner = p.join("Contents").join("MacOS");
            if let Ok(entries) = std::fs::read_dir(&inner) {
                for entry in entries.flatten() {
                    let candidate = entry.path();
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
    }

    None
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var("PATH").ok()?;
    let exe_name = if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(&exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Write the embedded normalization script to a stable temporary location and
/// return its path. Rewriting on each call keeps the staged copy in sync with
/// the application build.
pub fn materialize_script() -> Result<PathBuf, BlenderError> {
    let dir = std::env::temp_dir().join("aio-asset-normalizer");
    std::fs::create_dir_all(&dir)
        .map_err(|error| BlenderError::ScriptWrite(error.to_string()))?;
    let path = dir.join(SCRIPT_FILE_NAME);
    std::fs::write(&path, SCRIPT_SOURCE)
        .map_err(|error| BlenderError::ScriptWrite(error.to_string()))?;
    Ok(path)
}

/// Run one conversion by invoking `blender -b -P <script> -- <in> <out>
/// <config>`. Output lines are streamed to `tx` as `ConverterMessage::Log`
/// messages; callers must not interpret them as success on their own.
pub fn run_task(
    task: &ConversionTask,
    tx: &mpsc::Sender<ConverterMessage>,
) -> Result<(), BlenderError> {
    if !task.input.is_file() {
        return Err(BlenderError::InputMissing(task.input.clone()));
    }

    let blender = find_blender(task.blender_path.as_deref())
        .ok_or(BlenderError::BlenderNotFound)?;
    let script = materialize_script()?;

    let _ = tx.send(ConverterMessage::Log(format!(
        "Blender: {}",
        blender.display()
    )));
    let _ = tx.send(ConverterMessage::Log(format!(
        "Input:   {}",
        task.input.display()
    )));
    let _ = tx.send(ConverterMessage::Log(format!(
        "Output:  {}",
        task.output.display()
    )));

    // Paths are passed as OsStr so non-UTF-8 inputs never panic here.
    let mut child = Command::new(&blender)
        .arg("-b")
        .arg("-P")
        .arg(&script)
        .arg("--")
        .arg(&task.input)
        .arg(&task.output)
        .arg(&task.config_json)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| BlenderError::Spawn(error.to_string()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BlenderError::Spawn("missing stdout pipe".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| BlenderError::Spawn("missing stderr pipe".into()))?;

    let tx_stdout = tx.clone();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                let _ = tx_stdout.send(ConverterMessage::Log(line));
            }
        }
    });

    let tx_stderr = tx.clone();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                let _ = tx_stderr.send(ConverterMessage::Log(line));
            }
        }
    });

    let status = child
        .wait()
        .map_err(|error| BlenderError::Spawn(error.to_string()))?;
    if !status.success() {
        return Err(BlenderError::ExitCode(status.code()));
    }
    if !task.output.exists() {
        return Err(BlenderError::OutputMissing);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_blender_path_rejects_directories_and_missing_files() {
        assert!(resolve_blender_path("definitely-not-blender-xyz").is_none());
        // A plain directory without the macOS bundle layout resolves to None.
        assert!(resolve_blender_path(std::env::temp_dir()).is_none());
    }

    #[test]
    fn resolve_blender_path_accepts_existing_file() {
        let file = std::env::temp_dir().join("aio-blender-test-exe");
        std::fs::write(&file, b"").unwrap();
        let expected = file.clone();
        assert_eq!(resolve_blender_path(&file), Some(expected));
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn materialize_script_writes_embedded_copy() {
        let path = materialize_script().expect("script stages");
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("def main():"));
        assert!(contents.contains("export_scene.gltf"));
    }

    #[test]
    fn run_task_reports_missing_input() {
        let (tx, _rx) = mpsc::channel();
        let task = ConversionTask {
            input: PathBuf::from("this-file-does-not-exist.fbx"),
            output: PathBuf::from("unused.glb"),
            config_json: "{}".to_owned(),
            blender_path: None,
        };
        let error = run_task(&task, &tx).unwrap_err();
        assert!(matches!(error, BlenderError::InputMissing(_)));
    }
}
