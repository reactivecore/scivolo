//! The transcripts in `tests/repl/*.check`. All share one session, reset between files, run in
//! the repository root.

mod transcript;

use repl::Session;
use std::path::{Path, PathBuf};
use transcript::{answer, non_blank};
use vm::{Classpath, Vm};

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
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let mut session = Session::new(vm, image, 80).map_err(|e| e.to_string())?;
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
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(run).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}
