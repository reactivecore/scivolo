//! Embeds the library overlay: `overlay/nir/**/*.nir` as `(top-level name, bytes)` pairs.

use std::fmt::Write;
use std::path::{Path, PathBuf};

fn nir_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            nir_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "nir") {
            out.push(path);
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../overlay/nir");
    let root = root.canonicalize().unwrap_or(root);
    println!("cargo::rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    nir_files(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no overlay NIR in {}: run `just overlay`", root.display());
    let mut code = String::from("pub const FILES: &[(&str, &[u8])] = &[\n");
    for file in &files {
        println!("cargo::rerun-if-changed={}", file.display());
        let rel = file.strip_prefix(&root).expect("under the overlay root").with_extension("");
        let top = rel.to_string_lossy().replace(['/', '\\'], ".");
        writeln!(code, "    ({top:?}, include_bytes!({:?})),", file.display().to_string()).expect("a String");
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("overlay.rs");
    std::fs::write(out, code).expect("OUT_DIR is writable");
}
