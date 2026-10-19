//! Transcripts in the format of dotty's `test-resources/repl`: `scala> ` input lines, `     | `
//! continuation lines (`     |` an empty one), the expected output; blank lines do not count.

use repl::Session;
use std::path::PathBuf;
use vm::{Classpath, Vm};

const PROMPT: &str = "scala> ";
const CONTINUATION: &str = "     |";
/// The start of the error of a run whose libraries are missing.
pub const MISSING: &str = "missing libraries: ";

pub fn non_blank(lines: impl Iterator<Item = String>) -> Vec<String> {
    lines.map(|l| l.trim_end().to_owned()).filter(|l| !l.is_empty()).collect()
}

/// The transcript of the inputs of `expected` as `session` answers them.
pub fn answer(session: &mut Session, expected: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = expected.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(first) = line.strip_prefix(PROMPT) else { continue };
        let mut input = vec![first.to_owned()];
        out.push(line.to_owned());
        while let Some(more) = lines.peek().and_then(|l| l.strip_prefix(CONTINUATION)) {
            input.push(more.strip_prefix(' ').unwrap_or(more).to_owned());
            out.push(lines.next().unwrap().to_owned());
        }
        out.extend(session.eval(&input.join("\n")));
    }
    out
}

/// A session over the standard libraries and the jars `cp`.
pub fn session(cp: &[PathBuf]) -> Result<Session, String> {
    let mut jars = libs::jars()?;
    jars.extend(cp.iter().cloned());
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(cp, image).map_err(|e| e.to_string())?;
    let vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    Session::new(vm, image, 80).map_err(|e| e.to_string())
}

/// Runs `run` on a thread with a large stack; skipped outside CI if its libraries are missing.
pub fn check(run: fn() -> Result<(), String>) {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(run).unwrap().join().unwrap();
    match run {
        Err(e) if missing(&e) && std::env::var_os("CI").is_none() => eprintln!("skipped: {e}"),
        run => run.unwrap(),
    }
}

/// Whether error `e` says that libraries are missing.
fn missing(e: &str) -> bool {
    e.starts_with(MISSING) || e.contains("or the Coursier cache")
}
