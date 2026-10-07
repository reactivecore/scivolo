//! `scalai` without a target: the REPL reading inputs from piped stdin.

#[allow(dead_code)]
#[path = "../src/libs.rs"]
mod libs;

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

const INPUT: &str = "1 + 1
def twice(x: Int): Int =
  x * 2

twice(21)
val s = \"a\" + 1
:quit
println(\"not reached\")
";

const OUTPUT: &str = "val res0: Int = 2
def twice(x: Int): Int
val res1: Int = 42
val s: String = \"a1\"
";

/// The exit code, stdout and wall time of `scalai` reading `input`.
fn repl(input: &str) -> (Option<i32>, String, f64) {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.code(), String::from_utf8(out.stdout).unwrap(), start.elapsed().as_secs_f64())
}

#[test]
fn piped_inputs() {
    if libs::jars().is_err() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        return;
    }
    let (_, _, warm) = repl("");
    let (_, first, startup) = repl("1 + 1\n");
    assert_eq!(first, "val res0: Int = 2\n");
    eprintln!("scalai REPL: {warm:.3} s empty (warm-up), {startup:.3} s to the result of `1 + 1`");
    let (code, out, _) = repl(INPUT);
    assert_eq!(out, OUTPUT);
    assert_eq!(code, Some(0));
}
