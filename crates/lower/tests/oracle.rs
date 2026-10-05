//! The lowering's NIR against Scala Native's for the same sources (research 10 §5): the same
//! files, each with the same classes and fields in order, the same methods in any order, with the
//! same instructions and positions and the same local names and scopes in any order.

use nir::{Caches, Pool, Reader, show};
use std::path::{Path, PathBuf};
use syntax::source::SourceFile;
use tasty::Universe;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// NIR file `data` as text: whether it has entry points, the classes and fields in order, then
/// the methods sorted.
fn decoded(data: &[u8]) -> Vec<String> {
    let (mut caches, mut pool) = (Caches::default(), Pool::default());
    let mut reader = Reader::new(data, &mut caches, &mut pool).unwrap();
    let entry_points = format!("entry points: {}", reader.prelude.has_entry_points);
    let defns = reader.all().unwrap();
    let is_method =
        |d: &&nir::Defn| matches!(d.kind, nir::DefnKind::Define { .. } | nir::DefnKind::Declare { .. });
    let (methods, others): (Vec<_>, Vec<_>) = defns.iter().partition(is_method);
    let mut methods: Vec<String> = methods.iter().map(|d| show::defn_with_debug_info(&pool, d)).collect();
    methods.sort();
    let others = others.iter().map(|d| show::defn_with_debug_info(&pool, d));
    std::iter::once(entry_points).chain(others).chain(methods).collect()
}

#[test]
fn programs_lower_as_scala_native_compiles_them() {
    let jars = typer::dump::library_jars(&root().join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let programs = std::fs::read_to_string(root().join("tests/programs/lowered.txt")).unwrap();
    for program in programs.lines().filter(|p| !p.is_empty()) {
        let mut u = Universe::open(&jars).unwrap();
        let dir = root().join("tests/programs").join(program);
        let file = format!("{program}.scala");
        let text = std::fs::read_to_string(dir.join(&file)).unwrap();
        let files = lower::compile(&mut u, &SourceFile::new(file, text), &[]).unwrap();
        let mut ours: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        ours.sort_unstable();
        let mut theirs: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.unwrap().file_name().to_str()?.strip_suffix(".nir").map(str::to_owned))
            .collect();
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "{program}: files");
        for f in &files {
            let reference = std::fs::read(dir.join(format!("{}.nir", f.name))).unwrap();
            let (ours, theirs) = (decoded(&f.bytes), decoded(&reference));
            for (a, b) in ours.iter().zip(&theirs) {
                if a != b {
                    panic!("{program}: {}\n{}", f.name, first_difference(a, b));
                }
            }
            assert_eq!(ours.len(), theirs.len(), "{program}: {} definitions", f.name);
        }
    }
}

/// The first line where `ours` and `theirs` differ, with the definition's first line, then both
/// definitions.
fn first_difference(ours: &str, theirs: &str) -> String {
    let header = ours.lines().next().unwrap_or_default();
    let (a, b): (Vec<&str>, Vec<&str>) = (ours.lines().collect(), theirs.lines().collect());
    let i = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
    let line = |v: &[&str]| v.get(i).copied().unwrap_or("<end>").to_owned();
    format!(
        "{header}\nline {i}:\n  ours:   {}\n  theirs: {}\n\nours:\n{ours}\ntheirs:\n{theirs}",
        line(&a),
        line(&b)
    )
}
