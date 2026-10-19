//! The transcript `tests/repl/deps/circe.check`, run with Circe on the classpath; skipped outside
//! CI when Coursier cannot provide it.

mod deps;
mod transcript;

#[test]
fn circe_transcript_matches() {
    transcript::check(|| {
        deps::check_with_deps(&["io.circe::circe-parser::0.14.10"], "tests/repl/deps/circe.check")
    });
}
