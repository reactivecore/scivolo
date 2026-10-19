//! Transcripts of `tests/repl/deps`, run with libraries fetched by Coursier; one per test binary,
//! since a process holds one VM.

use crate::transcript::{MISSING, answer, non_blank, session};
use std::path::Path;

/// Checks transcript `check` (a path from the repository root) with the libraries `deps`
/// fetched by Coursier.
pub fn check_with_deps(deps: &[&str], check: &str) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deps: Vec<String> = deps.iter().map(|&d| d.to_owned()).collect();
    let cp = libs::deps::fetch(&deps).map_err(|e| format!("{MISSING}{e}"))?;
    let mut session = session(&cp)?;
    let text = std::fs::read_to_string(root.join(check)).map_err(|e| e.to_string())?;
    let expected = non_blank(text.lines().map(str::to_owned));
    let actual = non_blank(answer(&mut session, &text).into_iter());
    session.shutdown().map_err(|e| e.to_string())?;
    match actual == expected {
        true => Ok(()),
        false => Err(format!("transcript differs, actual output:\n\n{}", actual.join("\n"))),
    }
}
