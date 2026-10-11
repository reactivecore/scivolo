//! Expanding the inline calls of small inputs in `tests/inlined`, against the `Inlined` trees of
//! dotty 3.9.0's `inlining` phase recorded next to each in a `.check` file (`just typer-checks`).

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::{dump, library_jars};

#[test]
fn inlined_trees_match_dotty() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let jars = library_jars(&root.join("../../ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return;
    }
    let mut inputs: Vec<_> = std::fs::read_dir(root.join("tests/inlined"))
        .expect("tests/inlined")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "scala"))
        .collect();
    inputs.sort();
    assert!(!inputs.is_empty());
    for input in inputs {
        let mut u = Universe::open(&jars).expect("the library");
        let expected = std::fs::read_to_string(input.with_extension("check")).expect("a .check file");
        let text = std::fs::read_to_string(&input).expect("a test input");
        let actual = dump(&mut u, &SourceFile::new(input.to_string_lossy(), text.as_str()), &[], false, true);
        assert_eq!(actual, expected, "{}", input.display());
    }
}
