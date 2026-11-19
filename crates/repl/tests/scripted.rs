//! The transcripts in `tests/repl/*.check`. All share one session, reset between files, run in
//! the repository root.

mod transcript;

use transcript::{check, run_all, session};

fn run() -> Result<(), String> {
    run_all(session(&[])?)
}

#[test]
fn transcripts_match() {
    check(run);
}
