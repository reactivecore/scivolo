//! The lowering's NIR against Scala Native's for the same sources (research 10 §5): the same
//! files, each with the same classes and fields in order, the same methods in any order, with the
//! same instructions and positions and the same local names, scopes and variable slots in any
//! order.

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
    let mut methods: Vec<String> =
        methods.iter().map(|d| vars_by_name(&show::defn_with_debug_info(&pool, d))).collect();
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
        let mut theirs = nir_names(&dir, "");
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "{program}: files");
        for f in &files {
            let reference = std::fs::read(dir.join(format!("{}.nir", f.name.replace('.', "/")))).unwrap();
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

/// The top-level names of the `.nir` files under `dir`, those in package directories prefixed by
/// their package and `prefix`.
fn nir_names(dir: &Path, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().to_string_lossy().into_owned();
        if e.file_type().unwrap().is_dir() {
            out.extend(nir_names(&e.path(), &format!("{prefix}{name}.")));
        } else if let Some(top) = name.strip_suffix(".nir") {
            out.push(format!("{prefix}{top}"));
        }
    }
    out
}

/// Method text `text` with its local variable slots in name order: nscplugin keeps them in a
/// `Set`, whose order beyond four elements follows dotty's symbol ids.
fn vars_by_name(text: &str) -> String {
    let var = |line: &str| -> Option<(u64, String)> {
        let rest = line.strip_prefix("  %")?;
        let (id, rest) = rest.split_once(" <")?;
        let (name, rest) = rest.split_once('>')?;
        rest.starts_with(" = var[")
            .then(|| (id.parse().ok(), name.to_owned()))
            .and_then(|(i, n)| Some((i?, n)))
    };
    let vars: Vec<(u64, String)> = text.lines().filter_map(var).collect();
    if vars.len() < 2 {
        return text.to_owned();
    }
    let mut ids: Vec<u64> = vars.iter().map(|&(i, _)| i).collect();
    ids.sort_unstable();
    let mut by_name = vars.clone();
    by_name.sort_by(|a, b| a.1.cmp(&b.1));
    let map: std::collections::HashMap<u64, u64> =
        by_name.iter().zip(&ids).map(|(&(old, _), &new)| (old, new)).collect();
    let renumber = |line: &str| -> String {
        let mut out = String::new();
        let mut rest = line;
        while let Some(i) = rest.find('%') {
            out.push_str(&rest[..=i]);
            rest = &rest[i + 1..];
            let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            let id = rest[..digits].parse().ok().and_then(|n: u64| map.get(&n).copied());
            match id {
                Some(new) => out.push_str(&new.to_string()),
                None => out.push_str(&rest[..digits]),
            }
            rest = &rest[digits..];
        }
        out + rest
    };
    let mut lines: Vec<String> = text.lines().map(renumber).collect();
    let slots: Vec<usize> =
        lines.iter().enumerate().filter(|(_, l)| var(l).is_some()).map(|(i, _)| i).collect();
    let mut var_lines: Vec<String> = slots.iter().map(|&i| lines[i].clone()).collect();
    var_lines.sort_by_key(|l| var(l).map(|(i, _)| i));
    for (&i, line) in slots.iter().zip(var_lines) {
        lines[i] = line;
    }
    for line in lines.iter_mut().filter(|l| l.starts_with("names [")) {
        let mut names: Vec<(u64, String)> = line["names [".len()..line.len() - 1]
            .split("), (")
            .filter_map(|e| {
                let (id, name) = e.trim_matches(|c| c == '(' || c == ')').split_once(", ")?;
                let id: u64 = id.parse().ok()?;
                Some((map.get(&id).copied().unwrap_or(id), name.to_owned()))
            })
            .collect();
        names.sort();
        let entries: Vec<String> = names.iter().map(|(i, n)| format!("({i}, {n})")).collect();
        *line = format!("names [{}]", entries.join(", "));
    }
    lines.join("\n")
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
