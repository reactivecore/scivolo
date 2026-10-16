//! Compiles Scala Native's C runtime sources from `native/` into one shared library and one
//! static archive.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

fn check(cmd: &mut Command, what: &str) {
    let status = cmd.status().unwrap_or_else(|e| panic!("running {what}: {e}"));
    assert!(status.success(), "{what} failed");
}

/// Compiles every source into its own object under `obj_dir`, `jobs` at a time.
fn objects(cc: &str, native: &Path, files: &[PathBuf], obj_dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(obj_dir).expect("creating the object directory");
    let jobs = std::env::var("NUM_JOBS")
        .ok()
        .and_then(|n| n.parse().ok())
        .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
        .unwrap_or(4)
        .max(1);
    let objs: Vec<PathBuf> =
        files.iter().enumerate().map(|(i, f)| object_name(obj_dir, native, i, f)).collect();
    let mut failed = Vec::new();
    for chunk in files.iter().zip(&objs).collect::<Vec<_>>().chunks(jobs) {
        let children: Vec<_> =
            chunk.iter().map(|(file, obj)| (compile(cc, native, file, obj), obj)).collect();
        for (mut child, obj) in children {
            let ok = child.wait().is_ok_and(|s| s.success());
            if !ok {
                failed.push(obj.display().to_string());
            }
        }
    }
    assert!(failed.is_empty(), "compiling {} failed", failed.join(", "));
    objs
}

/// `obj_dir/<index>_<path under native with _ for />.o`.
fn object_name(obj_dir: &Path, native: &Path, index: usize, file: &Path) -> PathBuf {
    let rel = file.strip_prefix(native).expect("source under native/");
    obj_dir.join(format!("{index}_{}", rel.to_string_lossy().replace('/', "_"))).with_extension("o")
}

fn compile(cc: &str, native: &Path, file: &Path, obj: &Path) -> std::process::Child {
    Command::new(cc)
        .args(["-std=gnu11", "-O2", "-fPIC", "-w", "-c"])
        .args(["-DSCALANATIVE_COMPILE_ALWAYS", "-D__SCALANATIVE_JAVALIB_Z", "-DSCALANATIVE_GC_IMMIX"])
        .arg("-I")
        .arg(native.join("nativelib/scala-native"))
        .arg("-I")
        .arg(native.join("nativelib/scala-native/gc"))
        .arg(file)
        .arg("-o")
        .arg(obj)
        .stdin(Stdio::null())
        .spawn()
        .expect("running the C compiler")
}

fn main() {
    let native = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let mut files = Vec::new();
    sources(&native, &mut files);
    files.sort();
    let cc = std::env::var("CC").unwrap_or_else(|_| "clang".into());
    let objs = objects(&cc, &native, &files, &out_dir.join("obj"));
    let lib = out_dir.join(if cfg!(target_os = "macos") { "libsnrt.dylib" } else { "libsnrt.so" });
    let mut cmd = Command::new(&cc);
    cmd.args(["-shared"]).args(&objs).arg("-lz").arg("-o").arg(&lib);
    if cfg!(target_os = "macos") {
        cmd.args(["-undefined", "dynamic_lookup"]);
    } else {
        cmd.arg("-lpthread");
    }
    check(&mut cmd, "linking the Scala Native C runtime");
    let archive = out_dir.join("libsnrt.a");
    let _ = std::fs::remove_file(&archive);
    let ar = std::env::var("AR").unwrap_or_else(|_| "ar".into());
    check(Command::new(ar).arg("crs").arg(&archive).args(&objs), "archiving the Scala Native C runtime");
    println!("cargo:rerun-if-changed={}", native.display());
    println!("cargo:rustc-env=SNRT_PATH={}", lib.display());
    println!("cargo:rustc-env=SNRT_ARCHIVE={}", archive.display());
}
