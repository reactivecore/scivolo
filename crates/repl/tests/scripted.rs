//! The transcripts in `tests/repl/*.check`. All share one session, reset between files, run in
//! the repository root.

mod transcript;

use std::path::{Path, PathBuf};
use transcript::{answer, check, non_blank, session};

fn transcripts() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/repl");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.retain(|f| f.extension().is_some_and(|e| e == "check"));
    files.sort();
    files
}

fn run() -> Result<(), String> {
    std::env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .map_err(|e| e.to_string())?;
    let mut session = session(&[])?;
    let mut failed = Vec::new();
    for file in transcripts() {
        let text = std::fs::read_to_string(&file).unwrap();
        let expected = non_blank(text.lines().map(str::to_owned));
        let actual = non_blank(answer(&mut session, &text).into_iter());
        if actual != expected {
            failed.push(format!("{}:\n{}", file.display(), actual.join("\n")));
        }
        session.reset();
    }
    session.shutdown().map_err(|e| e.to_string())?;
    match failed.is_empty() {
        true => Ok(()),
        false => Err(format!("transcripts differ, actual output:\n\n{}", failed.join("\n\n"))),
    }
}

#[test]
fn transcripts_match() {
    check(run);
}
