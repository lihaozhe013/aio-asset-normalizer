use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon/aio-asset-normalizer.ico");
    watch_git_metadata();

    let commit = git_commit_hash().unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=AIO_ASSET_NORMALIZER_COMMIT={commit}");

    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("assets/icon/aio-asset-normalizer.ico");
        resource
            .compile()
            .expect("failed to embed the application icon");
    }
}

fn git_commit_hash() -> Option<String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("git")
        .current_dir(manifest_dir)
        .args(["rev-parse", "--verify", "HEAD^{commit}"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let hash = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    let valid_length = matches!(hash.len(), 40 | 64);
    if valid_length && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(hash)
    } else {
        None
    }
}

fn watch_git_metadata() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(git_dir) = resolve_git_dir(manifest_dir) else {
        return;
    };

    let head = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());

    let Ok(head_contents) = fs::read_to_string(&head) else {
        return;
    };
    let Some(reference) = head_contents.strip_prefix("ref: ").map(str::trim)
    else {
        return;
    };

    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join(reference).display()
    );
}

fn resolve_git_dir(manifest_dir: &Path) -> Option<PathBuf> {
    let dot_git = manifest_dir.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }

    let git_file = fs::read_to_string(dot_git).ok()?;
    let git_dir = git_file.strip_prefix("gitdir:")?.trim();
    let git_dir = PathBuf::from(git_dir);
    if git_dir.is_absolute() {
        Some(git_dir)
    } else {
        Some(manifest_dir.join(git_dir))
    }
}
