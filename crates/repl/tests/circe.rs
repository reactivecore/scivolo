//! The transcript `tests/repl/deps/circe.check`, run with Circe on the classpath; skipped outside
//! CI when Coursier cannot provide it.

mod transcript;

use std::path::Path;
use transcript::{MISSING, answer, check, non_blank, session};

const DEPS: [&str; 1] = ["io.circe::circe-parser::0.14.10"];

fn run() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deps: Vec<String> = DEPS.map(str::to_owned).to_vec();
    let cp = libs::deps::fetch(&deps).map_err(|e| format!("{MISSING}{e}"))?;
    let mut session = session(&cp)?;
    let text =
        std::fs::read_to_string(root.join("tests/repl/deps/circe.check")).map_err(|e| e.to_string())?;
    let expected = non_blank(text.lines().map(str::to_owned));
    let actual = non_blank(answer(&mut session, &text).into_iter());
    session.shutdown().map_err(|e| e.to_string())?;
    match actual == expected {
        true => Ok(()),
        false => Err(format!("transcript differs, actual output:\n\n{}", actual.join("\n"))),
    }
}

#[test]
fn circe_transcript_matches() {
    check(run);
}
