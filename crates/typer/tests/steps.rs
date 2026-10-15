//! Inputs whose typing must stay within a budget of subtype comparison steps.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::{dump, library_jars};

/// The subtype steps of typing `text`.
fn steps(text: &str) -> Option<u64> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let jars = library_jars(&root.join("../../ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    let mut u = Universe::open(&jars).expect("the library");
    dump(&mut u, &SourceFile::new("steps.scala", text), &[], false, false);
    Some(u.subtype_steps)
}

/// dotty's tests/pos/i20521: a map literal of 200 entries of sequences of sets of constants.
#[test]
fn large_map_of_sets_of_constants() {
    let entries: Vec<String> = (2..200)
        .map(|i| format!("{i} -> Seq(Set({i}, {}), Set({i}, {}, {}, 2))", i - 1, i - 1, i / 2))
        .collect();
    let text = format!(
        "object LFSR {{\n  lazy val taps: Map[Int, Seq[Set[Int]]] = first\n  private def first = Map(\n    {}\n  )\n}}\n",
        entries.join(",\n    ")
    );
    let Some(steps) = steps(&text) else { return };
    assert!(steps < 20_000_000, "took {steps} steps");
}
