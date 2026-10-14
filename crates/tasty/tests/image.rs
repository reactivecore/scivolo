use libimage::{Derived, Image, Source, Spec};
use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tasty::{FileId, Kind, SymbolId, Universe};

fn jars() -> Option<Vec<PathBuf>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fetched = [
        "scala-library-3.9.0.jar",
        "nativelib_native0.5_3-0.5.12.jar",
        "clib_native0.5_3-0.5.12.jar",
        "posixlib_native0.5_3-0.5.12.jar",
    ]
    .map(|j| root.join("ref/libs").join(j));
    if let Some(missing) = fetched.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    let mut jars = fetched.to_vec();
    jars.push(root.join("libs/javalib-tasty-0.5.12.jar"));
    Some(jars)
}

/// The image of the TASTy of `jars` with symbol tables, and the package table if `table`, built
/// afresh in directory `name` of `CARGO_TARGET_TMPDIR`.
fn image(jars: &[PathBuf], name: &str, table: bool) -> &'static Image {
    let sources: Vec<Source> = jars.iter().cloned().map(Source::File).collect();
    let tag = format!("tasty test, symbol tables {}", tasty::SYMBOL_TABLE_VERSION);
    let packages = |names: &[&str]| tasty::package_table(names).into_bytes();
    let version = tasty::package_table_version();
    let derived = [Derived { name: tasty::PACKAGE_TABLE, version: &version, contents: &packages }];
    let spec = Spec {
        sources: &sources,
        keep: |n| n.ends_with(".tasty"),
        extra: &|_, data| tasty::symbol_table(data),
        derived: if table { &derived } else { &[] },
        tag: &tag,
    };
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    Box::leak(Box::new(Image::open(&spec, &dir).unwrap()))
}

/// The packages of `u`, loading every file of each until no file is left.
fn load_all(u: &mut Universe) -> Vec<SymbolId> {
    loop {
        let (loaded, mut packages, mut stack) = (u.loaded_files(), Vec::new(), vec![u.root()]);
        while let Some(p) = stack.pop() {
            packages.push(p);
            stack.extend(u.decls(p).unwrap().into_iter().filter(|&d| u.symbol(d).kind == Kind::Package));
        }
        if u.loaded_files() == loaded {
            return packages;
        }
    }
}

/// Every symbol of `u`, with its owner, name, kind, flags, run-time name, tree and whether it is
/// extern and its name derived, in declaration order.
fn symbols(u: &mut Universe) -> Vec<String> {
    let mut stack = load_all(u);
    let mut out = Vec::new();
    while let Some(s) = stack.pop() {
        for &d in &u.symbol(s).decls {
            let t = u.symbol(d);
            let owner = u.full_name(s);
            out.push(format!(
                "{owner} {} {:?} {:?} {:?} {:?} {} {} {}",
                t.name,
                t.kind,
                t.flags,
                t.target,
                t.tree,
                t.is_extern,
                t.derived,
                u.is_scala2(d)
            ));
            if t.kind != Kind::Package {
                stack.push(d);
            }
        }
    }
    out
}

#[test]
fn symbol_tables_enter_what_decoding_enters() {
    let Some(jars) = jars() else { return };
    let mut from_jars = Universe::open(&jars).unwrap();
    let decoded = symbols(&mut from_jars);
    assert!(decoded.len() > 100_000, "{} symbols", decoded.len());
    for (name, table) in [("tasty-tables", true), ("tasty-tables-no-packages", false)] {
        let mut from_image = Universe::open_image(image(&jars, name, table));
        let from_tables = symbols(&mut from_image);
        if let Some(i) = (0..decoded.len()).find(|&i| decoded.get(i) != from_tables.get(i)) {
            panic!("symbol {i}: decoded {:?}, from tables {:?}", decoded.get(i), from_tables.get(i));
        }
        assert_eq!(decoded.len(), from_tables.len());
        assert_eq!(from_jars.loaded_files(), from_image.loaded_files());
        let decoded_files = from_image.decoded_files();
        assert!(decoded_files * 100 < from_image.loaded_files(), "{decoded_files}");
        for f in 0..from_image.loaded_files() {
            from_image.file(FileId(f as u32));
        }
    }
}

#[test]
fn the_package_table_is_that_of_the_jars() {
    let Some(jars) = jars() else { return };
    let mut names = BTreeSet::new();
    for jar in &jars {
        let zip = zip::ZipArchive::new(File::open(jar).unwrap()).unwrap();
        names.extend(zip.file_names().map(str::to_owned));
    }
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let (table, _) =
        image(&jars, "tasty-packages", true).find(tasty::PACKAGE_TABLE).expect("the package table");
    assert_eq!(std::str::from_utf8(table).unwrap(), tasty::package_table(&names));
}

#[test]
#[ignore = "measurement"]
fn open_times() {
    let Some(jars) = jars() else { return };
    let image = image(&jars, "tasty-times", true);
    for _ in 0..3 {
        let start = Instant::now();
        let _ = Universe::open(&jars).unwrap();
        let from_jars = start.elapsed();
        let start = Instant::now();
        let _ = Universe::open_image(image);
        eprintln!("jars {from_jars:?}, image {:?}", start.elapsed());
    }
}
