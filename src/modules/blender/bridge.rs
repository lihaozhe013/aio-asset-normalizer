use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

const SCRIPT_RELATIVE_PATH: &str = "blender_scripts/normalize_v1.py";

pub fn find_blender() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("BLENDER_PATH") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let candidates = [
            "C:\\Program Files\\Blender Foundation\\Blender 4.4\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 4.3\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 4.2\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 4.1\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 4.0\\blender.exe",
            "C:\\Program Files\\Blender Foundation\\Blender 3.6\\blender.exe",
        ];
        for candidate in candidates {
            let p = PathBuf::from(candidate);
            if p.exists() {
                return Some(p);
            }
        }
    }

    if let Some(path) = find_in_path("blender") {
        return Some(path);
    }

    None
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var("PATH").ok()?;
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}.exe", name)
    } else {
        name.to_owned()
    };
    for dir in paths.split(if cfg!(target_os = "windows") { ';' } else { ':' }) {
        let candidate = PathBuf::from(dir).join(&exe_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn resolve_script_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)?;

    let candidate = exe_dir.join(SCRIPT_RELATIVE_PATH);
    if candidate.exists() {
        return Some(candidate);
    }

    let dev_candidate = PathBuf::from(SCRIPT_RELATIVE_PATH);
    if dev_candidate.exists() {
        return Some(dev_candidate);
    }

    None
}

pub fn run_task(
    task: &super::task::ConversionTask,
    tx: &mpsc::Sender<String>,
) -> std::io::Result<bool> {
    let blender = find_blender().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Blender executable not found. Set BLENDER_PATH environment variable.",
        )
    })?;

    let script = resolve_script_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Script not found: {}", SCRIPT_RELATIVE_PATH),
        )
    })?;

    let _ = tx.send(format!("[Bridge] Blender: {}", blender.display()));
    let _ = tx.send(format!("[Bridge] Script: {}", script.display()));
    let _ = tx.send(format!("[Bridge] Input:  {}", task.input.display()));
    let _ = tx.send(format!("[Bridge] Output: {}", task.output.display()));

    let mut child = Command::new(&blender)
        .args([
            "-b",
            "-P",
            script.to_str().unwrap(),
            "--",
            task.input.to_str().unwrap(),
            task.output.to_str().unwrap(),
            &task.config_json,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let tx_stdout = tx.clone();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx_stdout.send(l);
            }
        }
    });

    let tx_stderr = tx.clone();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx_stderr.send(l);
            }
        }
    });

    let status = child.wait()?;
    Ok(status.success())
}
