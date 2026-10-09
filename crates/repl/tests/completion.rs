//! The completion corpus `tests/repl-completion/*.tab` (format in docs/research/11-completion.md
//! §4): `scala>` lines run in one session, reset between files; each `tab>` line asks at `@@`.
//! An entry in scope must list dotty's answer less its `miss>` labels, in the same order; an
//! entry out of scope is skipped with its reason, and still lists no label dotty does not. The
//! `scala>` lines of files in scope must compile and run.

#[path = "../../scalai/src/libs.rs"]
mod libs;

use repl::{Group, Session};
use std::collections::BTreeMap;
use std::path::Path;
use vm::{Classpath, Vm};

const PROMPT: &str = "scala> ";
const TAB: &str = "tab> ";
const MISS: &str = "miss> ";
const CONTINUATION: &str = "     |";
const ANSWER: &str = "  ";
const CURSOR: &str = "@@";

/// The files out of this stage's scope, and why.
const SKIPPED_FILES: [(&str, &str); 4] = [
    ("scope.tab", "scope completion (stage 4)"),
    ("imports.tab", "import completion (stage 4)"),
    ("types.tab", "type-position completion (stage 4)"),
    ("views.tab", "conversions, extensions and JDK members (stage 5)"),
];

/// The entries out of this stage's scope in the other files, by file and input, and why.
const SKIPPED: [(&str, &str, &str); 8] = [
    ("members.tab", "x.hea@@x", "JLine's typo fallback, off while labels are missed (stage 5)"),
    ("members.tab", "x.ma@@q", "JLine's typo fallback, off while labels are missed (stage 5)"),
    ("members.tab", "_root_.@@", "the root package: dotty's classpath and the JDK (stage 4)"),
    ("members.tab", "scala.math.Ordering.Int.@@", "a JDK base class, `java.util.Comparator` (stage 5)"),
    ("unfinished.tab", "va@@", "scope completion (stage 4)"),
    ("unfinished.tab", "\"ab@@\"", "scope completion in a string literal (stage 4)"),
    ("unfinished.tab", "\"pri@@\"", "scope completion in a string literal (stage 4)"),
    ("unfinished.tab", "s\"pri@@\"", "scope completion in a string literal (stage 4)"),
];

struct Entry {
    input: String,
    expected: Vec<Group>,
    misses: Vec<String>,
}

#[derive(Default)]
struct Tally {
    passed: usize,
    with_misses: usize,
    /// The files of `SKIPPED_FILES` and the entries of `SKIPPED` found.
    found_files: Vec<&'static str>,
    found_skips: Vec<(&'static str, &'static str)>,
    skipped: BTreeMap<&'static str, usize>,
    failed: Vec<String>,
}

/// The labels of an answer line: tokens between spaces, a backticked label one token.
fn tokens(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut quoted = false;
    for part in line.split(' ').filter(|p| !p.is_empty()) {
        match out.last_mut() {
            Some(last) if quoted => {
                last.push(' ');
                last.push_str(part);
            }
            _ => out.push(part.to_owned()),
        }
        let ticks = part.matches('`').count();
        quoted ^= ticks % 2 == 1;
    }
    out
}

/// The groups of the answer lines `lines`.
fn answer(lines: &[&str]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for line in lines {
        let mut labels = tokens(line);
        if labels == ["<none>"] {
            return Vec::new();
        }
        let header = labels.first().and_then(|h| h.strip_prefix('[')?.strip_suffix(']')).map(str::to_owned);
        if let Some(h) = header {
            labels.remove(0);
            let header = if h == "original" { "original" } else { "others" };
            groups.push(Group { header: Some(header), labels });
        } else if let Some(last) = groups.last_mut() {
            last.labels.extend(labels);
        } else {
            groups.push(Group { header: None, labels });
        }
    }
    groups
}

/// The text of an input of `first` and continuation `lines`, with the prompt `prompt`.
fn text(first: &str, more: &[&str], prompt: &str) -> String {
    let more = more
        .iter()
        .map(|l| l.strip_prefix(CONTINUATION).unwrap_or(l))
        .map(|l| l.strip_prefix(' ').unwrap_or(l));
    std::iter::once(first.strip_prefix(prompt).unwrap_or(first)).chain(more).collect::<Vec<_>>().join("\n")
}

/// Runs the corpus file `name` of `content` through `session`, counting into `tally`.
fn run_file(session: &mut Session, name: &str, content: &str, tally: &mut Tally) {
    let lines: Vec<&str> = content.lines().collect();
    let skipped_file = SKIPPED_FILES.iter().find(|(f, _)| *f == name);
    tally.found_files.extend(skipped_file.map(|&(f, _)| f));
    let skipped_file = skipped_file.map(|&(_, why)| why);
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let more = lines[i + 1..].iter().take_while(|l| l.starts_with(CONTINUATION)).count();
        let cont = &lines[i + 1..i + 1 + more];
        i += 1 + more;
        if line.starts_with(PROMPT) {
            let input = text(line, cont, PROMPT);
            let out = session.eval(&input);
            if skipped_file.is_none() && out.iter().any(|l| fails(l)) {
                tally.failed.push(format!("{name}: `{input}` fails:\n{}", out.join("\n")));
            }
            continue;
        }
        if !line.starts_with(TAB) {
            continue;
        }
        let answers = lines[i..].iter().take_while(|l| l.starts_with(ANSWER) && !l.starts_with(CONTINUATION));
        let answers: Vec<&str> = answers.copied().collect();
        i += answers.len();
        let misses = lines[i..].iter().take_while(|l| l.starts_with(MISS) || l.starts_with("//"));
        let misses = misses.filter_map(|l| l.strip_prefix(MISS)).flat_map(tokens);
        let entry =
            Entry { input: text(line, cont, TAB), expected: answer(&answers), misses: misses.collect() };
        let skipped = SKIPPED.iter().find(|(f, input, _)| *f == name && *input == entry.input);
        tally.found_skips.extend(skipped.map(|&(f, input, _)| (f, input)));
        check(session, name, &entry, skipped_file.or(skipped.map(|&(_, _, why)| why)), tally);
    }
}

/// Whether output line `l` reports an error, a construct outside the subset or an exception.
fn fails(l: &str) -> bool {
    let class = l.split(": ").next().unwrap_or(l);
    let thrown = !class.contains(' ') && (class.ends_with("Exception") || class.ends_with("Error"));
    l.starts_with("-- ") && !l.contains("Warning") || thrown
}

/// Asks `session` for `entry` and compares the answer, exactly if in scope, else for extras only.
fn check(session: &mut Session, name: &str, entry: &Entry, skipped: Option<&'static str>, tally: &mut Tally) {
    let cursor = entry.input.find(CURSOR).expect("a cursor in each entry");
    let ours = session.complete(&entry.input.replace(CURSOR, ""), cursor);
    let theirs: Vec<&String> = entry.expected.iter().flat_map(|g| &g.labels).collect();
    let listed: Vec<&String> = ours.iter().flat_map(|g| &g.labels).collect();
    let extra: Vec<&&String> = listed.iter().filter(|l| !theirs.contains(l)).collect();
    if !extra.is_empty() {
        tally.failed.push(format!("{name}: `{}` lists labels dotty does not: {extra:?}", entry.input));
        return;
    }
    if let Some(why) = skipped {
        *tally.skipped.entry(why).or_default() += 1;
        return;
    }
    if let Some(m) = entry.misses.iter().find(|m| !theirs.contains(m)) {
        tally.failed.push(format!("{name}: `{}` misses `{m}`, which dotty does not list", entry.input));
        return;
    }
    let mut expected: Vec<Group> = Vec::new();
    for g in &entry.expected {
        let labels: Vec<String> = g.labels.iter().filter(|l| !entry.misses.contains(l)).cloned().collect();
        if !labels.is_empty() {
            expected.push(Group { header: g.header, labels });
        }
    }
    if ours != expected {
        let missing: Vec<&String> =
            expected.iter().flat_map(|g| &g.labels).filter(|l| !listed.contains(l)).collect();
        let order = format!("expected {expected:?}\n  actual   {ours:?}");
        let diff = if missing.is_empty() { order } else { format!("missing {missing:?}") };
        tally.failed.push(format!("{name}: `{}`\n  {diff}", entry.input));
        return;
    }
    tally.passed += 1;
    tally.with_misses += usize::from(!entry.misses.is_empty());
}

fn session() -> Result<Session, String> {
    std::env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .map_err(|e| e.to_string())?;
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    Session::new(vm, image, 80).map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    let mut session = session()?;
    let dir = Path::new("tests/repl-completion");
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.retain(|f| f.extension().is_some_and(|e| e == "tab"));
    files.sort();
    let mut tally = Tally::default();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        run_file(&mut session, &name, &std::fs::read_to_string(&file).unwrap(), &mut tally);
        session.reset();
    }
    unchanged_by_looks(&mut session, &mut tally.failed);
    session.shutdown().map_err(|e| e.to_string())?;
    eprintln!(
        "completion corpus: {} passed ({} with misses), skipped {:?}",
        tally.passed, tally.with_misses, tally.skipped
    );
    for (file, _) in SKIPPED_FILES.iter().filter(|(f, _)| !tally.found_files.contains(f)) {
        tally.failed.push(format!("skipped file {file} not in the corpus"));
    }
    for (file, input, _) in SKIPPED.iter().filter(|&&(f, i, _)| !tally.found_skips.contains(&(f, i))) {
        tally.failed.push(format!("skipped entry `{input}` not in {file}"));
    }
    match tally.failed.is_empty() {
        true => Ok(()),
        false => Err(format!("{} entries fail:\n\n{}", tally.failed.len(), tally.failed.join("\n\n"))),
    }
}

/// Inputs whose output a completion, or a `:type` command, before them must not change: a look
/// at a definition binds no name.
const UNCHANGED: [(&str, &str); 3] =
    [("val yy = x.toSt", "yz"), ("val zz = 1; x.toSt", "zz"), (":type val yy = 1", "yz")];

/// Checks `UNCHANGED` in `session`, which it resets, into `failed`.
fn unchanged_by_looks(session: &mut Session, failed: &mut Vec<String>) {
    for (look, input) in UNCHANGED {
        session.eval("val x = List(1, 2)");
        let expected = session.eval(input);
        session.reset();
        session.eval("val x = List(1, 2)");
        if look.starts_with(':') {
            session.eval(look);
        } else {
            session.complete(look, look.len());
        }
        let actual = session.eval(input);
        if actual != expected {
            failed.push(format!("`{look}` changes `{input}`: {expected:?} became {actual:?}"));
        }
        session.reset();
    }
}

/// Runs `f` on a thread with a large stack, a missing library cache skipping it outside CI.
fn on_big_stack(f: fn() -> Result<(), String>) {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(f).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}

#[test]
fn corpus_matches() {
    on_big_stack(run);
}
