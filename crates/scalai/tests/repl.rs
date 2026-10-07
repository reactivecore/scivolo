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

/// The exit code, stdout, stderr and wall time of `scalai` reading `input`, with environment
/// `env` added.
fn repl_with(input: &[u8], env: &[(&str, &str)]) -> (Option<i32>, String, String, f64) {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (out.status.code(), text(out.stdout), text(out.stderr), start.elapsed().as_secs_f64())
}

/// The exit code, stdout and wall time of `scalai` reading `input`.
fn repl(input: &str) -> (Option<i32>, String, f64) {
    let (code, out, _, secs) = repl_with(input.as_bytes(), &[]);
    (code, out, secs)
}

fn libs_missing() -> bool {
    let missing = libs::jars().is_err() && std::env::var_os("CI").is_none();
    if missing {
        eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
    }
    missing
}

#[test]
fn piped_inputs() {
    if libs_missing() {
        return;
    }
    let (_, _, warm) = repl("");
    let (_, first, startup) = repl("1 + 1\n");
    assert_eq!(first, "val res0: Int = 2\n");
    eprintln!("scalai REPL: {warm:.3} s empty (warm-up), {startup:.3} s to the result of `1 + 1`");
    let (code, out, _) = repl(INPUT);
    assert_eq!(out, OUTPUT);
    assert_eq!(code, Some(0));
    let (code, out, _) = repl("val a = 1\nSystem.exit(3)\nprintln(\"not reached\")\n");
    assert_eq!(out, "val a: Int = 1\n");
    assert_eq!(code, Some(3));
}

#[test]
fn piped_line_endings() {
    if libs_missing() {
        return;
    }
    let (code, out, _) = repl("def inc(x: Int): Int =\r\n  x + 1\r\ninc(1)\r\nval b = (1 +");
    let mut lines = out.lines();
    assert_eq!(lines.next(), Some("def inc(x: Int): Int"));
    assert_eq!(lines.next(), Some("val res0: Int = 2"));
    assert!(lines.next().is_some_and(|l| l.contains("error")), "{out}");
    assert_eq!(code, Some(0));
    let (code, out, _, _) = repl_with(b"val s = \"\xff\"\ns.length\n", &[]);
    assert_eq!(out, "val s: String = \"\u{fffd}\"\nval res0: Int = 1\n");
    assert_eq!(code, Some(0));
}

#[test]
fn missing_libraries() {
    let empty = env!("CARGO_TARGET_TMPDIR");
    let env = [("SCALAI_LIBS", empty), ("COURSIER_CACHE", empty), ("HOME", empty)];
    let (code, out, err, _) = repl_with(b"1 + 1\n", &env);
    assert_eq!(code, Some(1));
    assert_eq!(out, "");
    assert!(err.starts_with("scalai: auxlib"), "{err}");
}
