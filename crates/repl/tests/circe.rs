//! The Circe transcripts of `tests/repl/deps`, run with Circe on the classpath; skipped outside
//! CI when Coursier cannot provide it.

mod transcript;

use std::path::Path;
use transcript::{MISSING, answer, check, non_blank, session};

const PARSER: &str = "io.circe::circe-parser::0.14.10";
const GENERIC: &str = "io.circe::circe-generic::0.14.10";

/// Checks transcript `check` (a path from the repository root) with the libraries `deps`
/// fetched by Coursier, macro calls expanded if `macros`.
fn check_with_deps(deps: &[&str], check: &str, macros: bool) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deps: Vec<String> = deps.iter().map(|&d| d.to_owned()).collect();
    let cp = libs::deps::fetch(&deps).map_err(|e| format!("{MISSING}{e}"))?;
    let mut session = session(&cp)?;
    session.expand_macros(macros);
    let text = std::fs::read_to_string(root.join(check)).map_err(|e| e.to_string())?;
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
    check(|| check_with_deps(&[PARSER], "tests/repl/deps/circe.check", false));
}

#[test]
fn circe_derives_transcript_matches() {
    check(|| check_with_deps(&[PARSER, GENERIC], "tests/repl/deps/circe-derives.check", true));
}
