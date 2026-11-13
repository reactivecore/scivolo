//! Lowers Scala snippets and shows or compares the NIR of their classes.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

/// Finds the library jars; `None` when they are not in ref/libs outside CI.
pub fn jars() -> Option<Vec<PathBuf>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return None;
    }
    Some(jars)
}

/// Lowers `sources` together and shows the NIR of class `name`.
pub fn dump(jars: &[PathBuf], sources: &[SourceFile], name: &str) -> String {
    let mut u = Universe::open(jars).unwrap();
    let files = match sources {
        [one] => lower::compile(&mut u, one, &[]).map_err(|r| r.what),
        many => lower::compile_sources(&mut u, many, Config::from_options(&[]), None)
            .map(|(files, _)| files)
            .map_err(|r| r.what),
    };
    let files = files.unwrap_or_else(|what| panic!("{what}"));
    let file = files.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} missing"));
    let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
    let defs = nir::Reader::new(&file.bytes, &mut caches, &mut pool).unwrap().all().unwrap();
    defs.iter().map(|d| nir::show::defn_with_debug_info(&pool, d)).collect::<Vec<_>>().join("\n")
}

/// Lowers `source` and shows the NIR of class `class`; `None` without the library jars.
pub fn lowered(source: &str, class: &str) -> Option<String> {
    Some(dump(&jars()?, &[SourceFile::new("snippet.scala", source)], class))
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
