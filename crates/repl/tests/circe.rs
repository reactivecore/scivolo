//! The Circe transcripts of `tests/repl/deps`, run with Circe on the classpath; skipped outside
//! CI when Coursier cannot provide it.

mod transcript;

use transcript::{check, check_with_deps};

const PARSER: &str = "io.circe::circe-parser::0.14.10";
const GENERIC: &str = "io.circe::circe-generic::0.14.10";

#[test]
fn circe_transcript_matches() {
    check(|| check_with_deps(&[PARSER], "tests/repl/deps/circe.check", false));
}

#[test]
fn circe_derives_transcript_matches() {
    check(|| check_with_deps(&[PARSER, GENERIC], "tests/repl/deps/circe-derives.check", true));
}
