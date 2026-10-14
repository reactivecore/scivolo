use libimage::{Image, Source, Spec};
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

/// The image of the TASTy of `jars` with symbol tables, built afresh in directory `name` of
/// `CARGO_TARGET_TMPDIR`.
fn image(jars: &[PathBuf], name: &str) -> &'static Image {
    let sources: Vec<Source> = jars.iter().cloned().map(Source::File).collect();
    let tag = format!("tasty test, symbol tables {}", tasty::SYMBOL_TABLE_VERSION);
    let spec = Spec {
        sources: &sources,
        keep: |n| n.ends_with(".tasty"),
        extra: &|_, data| tasty::symbol_table(data),
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
    let mut from_image = Universe::open_image(image(&jars, "tasty-tables"));
    let decoded = symbols(&mut from_jars);
    let from_tables = symbols(&mut from_image);
    assert!(decoded.len() > 100_000, "{} symbols", decoded.len());
    if let Some(i) = (0..decoded.len()).find(|&i| decoded.get(i) != from_tables.get(i)) {
        panic!("symbol {i}: decoded {:?}, from tables {:?}", decoded.get(i), from_tables.get(i));
    }
    assert_eq!(decoded.len(), from_tables.len());
    assert_eq!(from_jars.loaded_files(), from_image.loaded_files());
    assert!(from_image.decoded_files() * 100 < from_image.loaded_files(), "{}", from_image.decoded_files());
    for f in 0..from_image.loaded_files() {
        from_image.file(FileId(f as u32));
    }
}

#[test]
#[ignore = "measurement"]
fn open_times() {
    let Some(jars) = jars() else { return };
    let image = image(&jars, "tasty-times");
    for _ in 0..3 {
        let start = Instant::now();
        let _ = Universe::open(&jars).unwrap();
        let from_jars = start.elapsed();
        let start = Instant::now();
        let _ = Universe::open_image(image);
        eprintln!("jars {from_jars:?}, image {:?}", start.elapsed());
    }
}
