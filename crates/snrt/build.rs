//! Compiles Scala Native's C runtime sources from `native/` into one shared library.

use std::path::{Path, PathBuf};
use std::process::Command;

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "c") {
            out.push(path);
        }
    }
}

fn main() {
    let native = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let mut files = Vec::new();
    sources(&native, &mut files);
    files.sort();
    let cc = std::env::var("CC").unwrap_or_else(|_| "clang".into());
    let lib = out_dir.join(if cfg!(target_os = "macos") { "libsnrt.dylib" } else { "libsnrt.so" });
    let mut cmd = Command::new(cc);
    cmd.args(["-std=gnu11", "-O2", "-fPIC", "-shared", "-w"])
        .args(["-DSCALANATIVE_COMPILE_ALWAYS", "-D__SCALANATIVE_JAVALIB_Z", "-DSCALANATIVE_GC_IMMIX"])
        .arg("-I")
        .arg(native.join("nativelib/scala-native"))
        .arg("-I")
        .arg(native.join("nativelib/scala-native/gc"))
        .args(&files)
        .arg("-lz")
        .arg("-o")
        .arg(&lib);
    if cfg!(target_os = "macos") {
        cmd.args(["-undefined", "dynamic_lookup"]);
    } else {
        cmd.arg("-lpthread");
    }
    let status = cmd.status().expect("running the C compiler");
    assert!(status.success(), "compiling the Scala Native C runtime failed");
    println!("cargo:rerun-if-changed={}", native.display());
    println!("cargo:rustc-env=SNRT_PATH={}", lib.display());
}
