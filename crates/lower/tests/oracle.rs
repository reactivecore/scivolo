//! The lowering's NIR against Scala Native's for the same sources (research 10 §5): the same
//! files and definitions, bodies equal up to the numbering of locals, scopes and debug names.

use nir::{Caches, Pool, Reader, show};
use std::path::{Path, PathBuf};
use syntax::source::SourceFile;
use tasty::Universe;

/// The programs of `tests/programs` the lowering covers.
const PROGRAMS: [&str; 1] = ["hello"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Each definition of NIR file `data` as normalized text.
fn decoded(data: &[u8]) -> Vec<String> {
    let (mut caches, mut pool) = (Caches::default(), Pool::default());
    let mut reader = Reader::new(data, &mut caches, &mut pool).unwrap();
    let defns = reader.all().unwrap();
    defns.iter().map(|d| normalized(&show::defn(&pool, d, true))).collect()
}

/// `text` with locals renumbered by first appearance, without scope ids and local names.
fn normalized(text: &str) -> String {
    let mut ids: Vec<String> = Vec::new();
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        let rest = match trimmed.split_once(": ") {
            Some((scope, rest)) if scope.bytes().all(|b| b.is_ascii_digit()) => rest,
            _ => trimmed,
        };
        out.push_str(indent);
        let mut chars = rest.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            let digits: String = rest[i + 1..].chars().take_while(char::is_ascii_digit).collect();
            (0..digits.len()).for_each(|_| _ = chars.next());
            let n = ids.iter().position(|d| *d == digits).unwrap_or_else(|| {
                ids.push(digits.clone());
                ids.len() - 1
            });
            out.push_str(&format!("%{n}"));
            if rest[i + 1 + digits.len()..].starts_with(" <") {
                while chars.next().is_some_and(|(_, c)| c != '>') {}
            }
        }
        out.push('\n');
    }
    out
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
        }
    }
}
