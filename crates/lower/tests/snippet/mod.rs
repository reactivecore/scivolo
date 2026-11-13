//! Lowers a Scala snippet and compares its methods with Scala Native's NIR of them.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;

/// Lowers `source` and shows the NIR of class `class`; `None` without the library jars.
pub fn lowered(source: &str, class: &str) -> Option<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return None;
    }
    let mut u = Universe::open(&jars).unwrap();
    let files = lower::compile(&mut u, &SourceFile::new("snippet.scala", source), &[])
        .unwrap_or_else(|r| panic!("{}", r.what));
    let file = files.iter().find(|f| f.name == class).unwrap_or_else(|| panic!("{class} missing"));
    let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
    let defs = nir::Reader::new(&file.bytes, &mut caches, &mut pool).unwrap().all().unwrap();
    Some(defs.iter().map(|d| nir::show::defn_with_debug_info(&pool, d)).collect::<Vec<_>>().join("\n"))
}

/// Asserts that `dump` holds each definition of `expected` as written, found by its first line.
pub fn assert_defs(dump: &str, expected: &[&str]) {
    for want in expected {
        let want = want.trim();
        let head = want.lines().next().unwrap();
        let start = dump
            .find(&format!("\n{head}\n"))
            .map(|i| i + 1)
            .or_else(|| dump.starts_with(&format!("{head}\n")).then_some(0))
            .unwrap_or_else(|| panic!("no `{head}` in\n{dump}"));
        let end = dump[start..].find("\n}").map_or(dump.len(), |i| start + i + 2);
        assert_eq!(&dump[start..end], want);
    }
}
