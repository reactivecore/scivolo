//! Sets `SCIVOLO_BUILD`, a hash of the workspace's crate sources, manifests, lockfile and `libs/`.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let mut inputs = vec![root.join("Cargo.lock"), root.join("libs")];
    for dir in std::fs::read_dir(root.join("crates")).unwrap().flatten().map(|e| e.path()) {
        if dir.join("Cargo.toml").exists() {
            inputs.extend([dir.join("Cargo.toml"), dir.join("src"), dir.join("build.rs")]);
        }
    }
    let mut files = Vec::new();
    for input in inputs.iter().filter(|i| i.exists()) {
        println!("cargo::rerun-if-changed={}", input.display());
        collect(input, &mut files);
    }
    files.sort();
    let mut h = DefaultHasher::new();
    for file in &files {
        file.strip_prefix(&root).unwrap_or(file).hash(&mut h);
        std::fs::read(file).unwrap_or_default().hash(&mut h);
    }
    println!("cargo::rustc-env=SCIVOLO_BUILD={:016x}", h.finish());
}

/// Adds `path`, or the files under it, to `files`.
fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    match std::fs::read_dir(path) {
        Ok(entries) => entries.flatten().for_each(|e| collect(&e.path(), files)),
        Err(_) if path.is_file() => files.push(path.to_owned()),
        Err(_) => {}
    }
}
