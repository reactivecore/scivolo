//! The transcript `tests/repl/deps/circe-derives.check`: Circe's `Codec` derived through its
//! macros, run in the VM; skipped outside CI when Coursier cannot provide Circe.

mod deps;
mod transcript;

const DEPS: [&str; 2] = ["io.circe::circe-parser::0.14.10", "io.circe::circe-generic::0.14.10"];

#[test]
fn circe_derives_transcript_matches() {
    transcript::check(|| deps::check_with_deps(&DEPS, "tests/repl/deps/circe-derives.check"));
}
