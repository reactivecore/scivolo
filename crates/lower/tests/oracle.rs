//! The lowering's NIR against Scala Native's for the same sources (research 10 §5): the same
//! files, each with the same definitions, instructions, positions, local names and scopes, and
//! the same bytes.

use nir::{Caches, Pool, Reader, show};
use std::path::{Path, PathBuf};
use syntax::source::SourceFile;
use tasty::Universe;

/// The programs of `tests/programs` the lowering covers.
const PROGRAMS: [&str; 1] = ["hello"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// NIR file `data` as text: whether it has entry points, then each definition.
fn decoded(data: &[u8]) -> Vec<String> {
    let (mut caches, mut pool) = (Caches::default(), Pool::default());
    let mut reader = Reader::new(data, &mut caches, &mut pool).unwrap();
    let entry_points = format!("entry points: {}", reader.prelude.has_entry_points);
    let defns = reader.all().unwrap();
    std::iter::once(entry_points).chain(defns.iter().map(|d| show::defn_with_debug_info(&pool, d))).collect()
}

#[test]
fn programs_lower_as_scala_native_compiles_them() {
    let jars = typer::dump::library_jars(&root().join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let mut u = Universe::open(&jars).unwrap();
    for program in PROGRAMS {
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
            assert_eq!(decoded(&f.bytes), decoded(&reference), "{program}: {}", f.name);
            assert!(f.bytes == reference, "{program}: {} differs in its bytes", f.name);
        }
    }
}
