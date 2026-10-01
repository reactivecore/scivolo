use nir::encode::Writer;
use nir::{Caches, Global, Pool, Reader, show};
use std::path::{Path, PathBuf};

/// The committed NIR files: the test programs' and the overlay's.
fn nir_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Vec::new();
    let mut stack = vec![root.join("tests/programs"), root.join("overlay/nir")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "nir") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Each definition of `data` as text with its debug information, and whether `data` has entry
/// points.
fn decoded(data: &[u8]) -> (Vec<String>, bool) {
    let (mut caches, mut pool) = (Caches::default(), Pool::default());
    let mut reader = Reader::new(data, &mut caches, &mut pool).unwrap();
    let entry_points = reader.prelude.has_entry_points;
    let defns = reader.all().unwrap();
    (defns.iter().map(|d| show::defn_with_debug_info(&pool, d)).collect(), entry_points)
}

#[test]
fn written_files_read_back_as_the_originals() {
    let files = nir_files();
    assert!(files.len() > 20, "{} NIR files", files.len());
    for path in files {
        let data = std::fs::read(&path).unwrap();
        let (mut caches, mut pool) = (Caches::default(), Pool::default());
        let mut reader = Reader::new(&data, &mut caches, &mut pool).unwrap();
        let entry_points = reader.prelude.has_entry_points;
        let defns = reader.all().unwrap();
        let none = nir::GlobalId(pool.globals.intern(Global::None));
        let written = Writer::new(&pool).file(&defns, none, entry_points);
        assert_eq!(decoded(&written), decoded(&data), "{}", path.display());
    }
}
