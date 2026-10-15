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

/// Compiles every source into its own object under `obj_dir`, in parallel.
fn objects(cc: &str, native: &Path, files: &[PathBuf], obj_dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(obj_dir).expect("creating the object directory");
    let mut jobs = Vec::new();
    for file in files {
        let rel = file.strip_prefix(native).expect("source under native/");
        let obj = obj_dir.join(rel.to_string_lossy().replace('/', "_")).with_extension("o");
        let child = Command::new(cc)
            .args(["-std=gnu11", "-O2", "-fPIC", "-w", "-c"])
            .args(["-DSCALANATIVE_COMPILE_ALWAYS", "-D__SCALANATIVE_JAVALIB_Z", "-DSCALANATIVE_GC_IMMIX"])
            .arg("-I")
            .arg(native.join("nativelib/scala-native"))
            .arg("-I")
            .arg(native.join("nativelib/scala-native/gc"))
            .arg(file)
            .arg("-o")
            .arg(&obj)
            .stdin(Stdio::null())
            .spawn()
            .expect("running the C compiler");
        jobs.push((child, obj));
    }
    jobs.into_iter()
        .map(|(mut child, obj)| {
            let status = child.wait().expect("waiting for the C compiler");
            assert!(status.success(), "compiling {} failed", obj.display());
            obj
        })
        .collect()
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
    check(Command::new("ar").arg("crs").arg(&archive).args(&objs), "archiving the Scala Native C runtime");
    println!("cargo:rerun-if-changed={}", native.display());
    println!("cargo:rustc-env=SNRT_PATH={}", lib.display());
    println!("cargo:rustc-env=SNRT_ARCHIVE={}", archive.display());
}
