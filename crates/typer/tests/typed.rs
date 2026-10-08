//! Typing small inputs in `tests/typed`, against dotty 3.9.0's output recorded next to each in a
//! `.check` file (`just typer-checks`): the typed trees, or only the diagnostics of an input with
//! errors; `just typer-report` compares on dotty's tests.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::{dump, library_jars};

fn typed(u: &mut Universe, path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("a test input");
    let options: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("//> using options "))
        .flat_map(str::split_whitespace)
        .collect();
    dump(u, &SourceFile::new(path.to_string_lossy(), text.as_str()), &options)
}

#[test]
fn typed_trees_and_diagnostics_match_dotty() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let jars = library_jars(&root.join("../../ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return;
    }
    let mut inputs: Vec<_> = std::fs::read_dir(root.join("tests/typed"))
        .expect("tests/typed")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "scala"))
        .collect();
    inputs.sort();
    assert!(!inputs.is_empty());
    for input in inputs {
        let mut u = Universe::open(&jars).expect("the library");
        let expected = std::fs::read_to_string(input.with_extension("check")).expect("a .check file");
        let actual = typed(&mut u, &input);
        if expected.contains("\n== E ") {
            let diagnostics =
                |s: &str| s.lines().filter(|l| l.starts_with("== ")).collect::<Vec<_>>().join("\n");
            assert_eq!(diagnostics(&actual), diagnostics(&expected), "{}", input.display());
        } else {
            assert_eq!(actual, expected, "{}", input.display());
        }
    }
}
