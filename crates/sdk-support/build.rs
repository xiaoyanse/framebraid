use std::{env, error::Error, fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn source_identity(root: &Path) -> (String, &'static str) {
    // 源码包可能位于另一仓库之下，不能误报外层仓库的提交。
    let own_repository = git(root, &["rev-parse", "--show-toplevel"])
        .and_then(|p| fs::canonicalize(p).ok())
        .zip(fs::canonicalize(root).ok())
        .is_some_and(|(git_root, source_root)| git_root == source_root);
    if !own_repository {
        return ("unknown".into(), "unknown");
    }
    for marker in ["HEAD", "index", "packed-refs"] {
        watch_git_path(root, marker);
    }
    if let Some(branch) = git(root, &["symbolic-ref", "-q", "HEAD"]) {
        watch_git_path(root, &branch);
    }
    let commit = git(root, &["rev-parse", "HEAD"])
        .filter(|value| value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".into());
    let state = match git(
        root,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            "Cargo.toml",
            "Cargo.lock",
            "crates",
            "src",
        ],
    ) {
        Some(value) if value.is_empty() => "clean",
        Some(_) => "dirty",
        None => "unknown",
    };
    (commit, state)
}

fn watch_git_path(root: &Path, marker: &str) {
    if let Some(filename) = git(root, &["rev-parse", "--git-path", marker]) {
        let path = root.join(filename);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = env::var("CARGO_MANIFEST_DIR")?;
    let root = Path::new(&manifest)
        .parent()
        .and_then(Path::parent)
        .ok_or("SDK workspace root missing")?;
    for input in ["Cargo.toml", "Cargo.lock", "src", "crates"] {
        let path = root.join(input);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SHARED_FRAMES");
    let version = env::var("CARGO_PKG_VERSION")?;
    let target = env::var("TARGET")?;
    let profile = env::var("PROFILE")?;
    let shared = if env::var_os("CARGO_FEATURE_SHARED_FRAMES").is_some() {
        "enabled"
    } else {
        "disabled"
    };
    let (commit, state) = source_identity(root);
    let notes = format!("source_commit={commit};source_state={state};profile={profile};support_shared_frames={shared};runtimes=on-demand");
    let generated = format!(
        "pub const PACKAGE_VERSION: &std::ffi::CStr = c{version:?};\npub const BUILD_TARGET: &std::ffi::CStr = c{target:?};\npub const BUILD_NOTES: &std::ffi::CStr = c{notes:?};\n"
    );
    fs::write(
        Path::new(&env::var("OUT_DIR")?).join("sdk_version.rs"),
        generated,
    )?;
    Ok(())
}
